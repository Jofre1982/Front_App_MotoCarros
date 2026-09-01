//! Pantalla de tarifa estimada, solicitud de viaje y seguimiento en tiempo
//! real (pasajero) — issues #13, #14 y #20.
//!
//! Consume `POST /api/v1/rides/estimate` (`ApiClient::estimate_ride`) para la
//! tarifa estimada, sin solicitar nada todavia. Una vez que hay una
//! estimacion, el pasajero puede confirmar y solicitar el viaje de verdad
//! con `POST /api/v1/rides` (`ApiClient::request_ride`, issue #14) — el
//! backend vuelve a calcular estos mismos numeros al crear el viaje. Mientras
//! el viaje solicitado sigue activo, esta pantalla no vuelve a ofrecer el
//! flujo de estimar/solicitar (un pasajero solo puede tener un viaje activo a
//! la vez, ver `openapi.yaml`).
//!
//! Origen y destino se eligen tocando el mapa (`MapView::on_click`, ver
//! `moto_ui/src/map.rs`), no con campos de texto: la historia depende
//! explicitamente del componente de mapa (issue #4).
//!
//! Mientras el viaje solicitado sigue en `requested` (nadie lo acepto
//! todavia), el pasajero puede desistir con `POST /api/v1/rides/{ride}/cancel`
//! (`ApiClient::cancel_ride`, issue #15), lo que devuelve la pantalla al
//! estado inicial para poder solicitar otro viaje. Una vez que el viaje pasa
//! a `accepted` este boton deja de ofrecerse — cancelar un viaje ya aceptado
//! es la historia "Cancelar un viaje aceptado (pasajero)" (#21), fuera de
//! alcance aca.
//!
//! Mientras el viaje siga activo (`requested`, `accepted` o `in_progress`),
//! `RideTrackingPanel` (issue #20) pide el estado actual por
//! `GET /api/v1/rides/{ride}` (`ApiClient::get_ride`) y se suscribe al canal
//! privado `ride.{id}` para el resto: cambios de estado (`status.changed`) y
//! la posicion del conductor asignado (`location.updated`, publicada por
//! `ShareLocationPanel` del lado del conductor, issue #19). Mismo patron de
//! sondeo/reconexion que `NearbyRidesList` (`nearby_rides.rs`); los eventos
//! se aplican con `moto_core::models::apply_ride_tracking_event`, logica
//! pura testeada aparte de Dioxus. El mapa reusa `MapView` (issue #4), que
//! desde esta historia actualiza sus marcadores reactivamente en vez de solo
//! al montarse (ver `moto_ui/src/map.rs`).

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    ApiClient, CancelRideError, EstimateRideError, GetRideError, RequestRideError,
};
use moto_core::models::{
    Coordinates, Ride, RideEstimate, RideStatus, RideTracking, apply_ride_tracking_event,
};
use moto_core::realtime::{
    ConnectionState, PollAction, RealtimeClient, RealtimeConfig, SubscribeFailureAction,
    decide_poll_action, decide_subscribe_failure,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::{MapMarker, MapView};

/// Intervalo entre sondeos de `RealtimeClient::poll_events` para el canal
/// `ride.{id}` (issue #20) — mismo valor y mismo motivo que el
/// `POLL_INTERVAL` de `NearbyRidesList` (`nearby_rides.rs`): el transporte
/// no avisa solo cuando llega un frame nuevo.
const RIDE_TRACKING_POLL_INTERVAL: Duration = Duration::from_millis(700);

/// Centro inicial del mapa mientras no exista geolocalizacion del
/// dispositivo (fuera de alcance de este issue): Bogota, el mismo punto que
/// ya usan los ejemplos de `openapi.yaml`.
const DEFAULT_CENTER_LAT: f64 = 4.710989;
const DEFAULT_CENTER_LNG: f64 = -74.072092;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PickTarget {
    Origin,
    Destination,
}

#[component]
pub fn RideEstimateScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut origin = use_signal(|| None::<(f64, f64)>);
    let mut destination = use_signal(|| None::<(f64, f64)>);
    let mut pick_target = use_signal(|| PickTarget::Origin);
    let mut estimate_error = use_signal(|| None::<EstimateRideError>);
    let mut estimate = use_signal(|| None::<RideEstimate>);
    let mut is_loading = use_signal(|| false);
    let mut ride_error = use_signal(|| None::<RequestRideError>);
    let mut requested_ride = use_signal(|| None::<Ride>);
    let mut is_requesting = use_signal(|| false);
    let mut cancel_error = use_signal(|| None::<CancelRideError>);
    let mut is_cancelling = use_signal(|| false);

    let on_map_click = move |(lat, lng): (f64, f64)| {
        estimate.set(None);
        estimate_error.set(None);

        match pick_target() {
            PickTarget::Origin => {
                origin.set(Some((lat, lng)));
                pick_target.set(PickTarget::Destination);
            }
            PickTarget::Destination => {
                destination.set(Some((lat, lng)));
            }
        }
    };

    let on_estimate_click = {
        let api_client = api_client.clone();
        let storage = storage.clone();
        move |_| {
            let Some(token) = session.token() else {
                return;
            };
            let Some((origin_lat, origin_lng)) = origin() else {
                return;
            };
            let Some((destination_lat, destination_lng)) = destination() else {
                return;
            };
            let api_client = api_client.clone();
            let storage = storage.clone();

            spawn(async move {
                is_loading.set(true);
                estimate_error.set(None);

                let origin_coords = Coordinates {
                    latitude: origin_lat,
                    longitude: origin_lng,
                };
                let destination_coords = Coordinates {
                    latitude: destination_lat,
                    longitude: destination_lng,
                };

                match api_client
                    .estimate_ride(&token, origin_coords, destination_coords)
                    .await
                {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        estimate.set(Some(fetch.data));
                    }
                    Err(EstimateRideError::SessionExpired) => {
                        session.logout(storage.as_ref());
                    }
                    Err(err) => {
                        estimate_error.set(Some(err));
                    }
                }

                is_loading.set(false);
            });
        }
    };

    let on_request_click = {
        let api_client = api_client.clone();
        let storage = storage.clone();
        move |_| {
            let Some(token) = session.token() else {
                return;
            };
            let Some((origin_lat, origin_lng)) = origin() else {
                return;
            };
            let Some((destination_lat, destination_lng)) = destination() else {
                return;
            };
            let api_client = api_client.clone();
            let storage = storage.clone();

            spawn(async move {
                is_requesting.set(true);
                ride_error.set(None);

                let origin_coords = Coordinates {
                    latitude: origin_lat,
                    longitude: origin_lng,
                };
                let destination_coords = Coordinates {
                    latitude: destination_lat,
                    longitude: destination_lng,
                };

                match api_client
                    .request_ride(&token, origin_coords, destination_coords)
                    .await
                {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        requested_ride.set(Some(fetch.data));
                    }
                    Err(RequestRideError::SessionExpired) => {
                        session.logout(storage.as_ref());
                    }
                    Err(err) => {
                        ride_error.set(Some(err));
                    }
                }

                is_requesting.set(false);
            });
        }
    };

    let on_cancel_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some(ride) = requested_ride() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_cancelling.set(true);
            cancel_error.set(None);

            match api_client.cancel_ride(&token, ride.id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    requested_ride.set(None);
                    estimate.set(None);
                    estimate_error.set(None);
                    ride_error.set(None);
                    origin.set(None);
                    destination.set(None);
                    pick_target.set(PickTarget::Origin);
                }
                Err(CancelRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    cancel_error.set(Some(err));
                }
            }

            is_cancelling.set(false);
        });
    };

    let markers: Vec<MapMarker> = [
        origin().map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Origen".to_string()),
        }),
        destination().map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Destino".to_string()),
        }),
    ]
    .into_iter()
    .flatten()
    .collect();

    let can_estimate = origin().is_some() && destination().is_some();

    let instructions = match pick_target() {
        PickTarget::Origin => "Toca el mapa para elegir el origen.",
        PickTarget::Destination => "Toca el mapa para elegir el destino.",
    };

    if let Some(ride) = requested_ride() {
        // Mientras el viaje sigue activo hay algo que seguir en tiempo real
        // (issue #20): una vez `completed`/`cancelled` no queda nada mas que
        // rastrear, y `RideTrackingPanel` se desmonta (Dioxus cancela su
        // loop de sondeo, mismo criterio que `ShareLocationPanel` en
        // `nearby_rides.rs`).
        let is_active = matches!(
            ride.status,
            RideStatus::Requested | RideStatus::Accepted | RideStatus::InProgress
        );

        return rsx! {
            div { class: "ride-estimate-screen",
                h2 { "Viaje solicitado" }
                p { class: "ride-request-status", "{ride_status_label(ride.status)}" }
                dl { class: "ride-request-result",
                    dt { "Distancia" }
                    dd { "{ride.distance_meters} m" }
                    dt { "Duracion" }
                    dd { "{ride.duration_seconds / 60} min" }
                    dt { "Tarifa estimada" }
                    dd { "{ride.currency} {ride.estimated_fare}" }
                }
                if ride.status == RideStatus::Requested {
                    button {
                        r#type: "button",
                        class: "ride-cancel-button",
                        disabled: is_cancelling(),
                        onclick: on_cancel_click,
                        if is_cancelling() {
                            "Cancelando..."
                        } else {
                            "Cancelar solicitud"
                        }
                    }
                    if let Some(err) = cancel_error() {
                        p { class: "ride-cancel-error", role: "alert", "{err}" }
                    }
                }
                if is_active {
                    RideTrackingPanel {
                        initial_ride: ride.clone(),
                        on_ride_updated: move |updated: Ride| {
                            requested_ride.set(Some(updated));
                        },
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "ride-estimate-screen",
            h2 { "Ver tarifa estimada" }
            p { class: "ride-estimate-instructions", "{instructions}" }
            div { class: "ride-estimate-target-buttons",
                button {
                    r#type: "button",
                    disabled: pick_target() == PickTarget::Origin,
                    onclick: move |_| pick_target.set(PickTarget::Origin),
                    "Elegir origen"
                }
                button {
                    r#type: "button",
                    disabled: pick_target() == PickTarget::Destination,
                    onclick: move |_| pick_target.set(PickTarget::Destination),
                    "Elegir destino"
                }
            }
            div { class: "ride-estimate-map", style: "height: 320px;",
                MapView {
                    center_lat: DEFAULT_CENTER_LAT,
                    center_lng: DEFAULT_CENTER_LNG,
                    markers,
                    on_click: on_map_click,
                }
            }
            button {
                r#type: "button",
                class: "ride-estimate-button",
                disabled: !can_estimate || is_loading(),
                onclick: on_estimate_click,
                if is_loading() {
                    "Calculando..."
                } else {
                    "Ver tarifa estimada"
                }
            }
            if let Some(err) = estimate_error() {
                p { class: "ride-estimate-error", role: "alert", "{err}" }
            } else if let Some(value) = estimate() {
                dl { class: "ride-estimate-result",
                    dt { "Distancia" }
                    dd { "{value.distance_meters} m" }
                    dt { "Duracion" }
                    dd { "{value.duration_seconds / 60} min" }
                    dt { "Tarifa estimada" }
                    dd { "{value.currency} {value.estimated_fare}" }
                }
                button {
                    r#type: "button",
                    class: "ride-request-button",
                    disabled: is_requesting(),
                    onclick: on_request_click,
                    if is_requesting() {
                        "Solicitando..."
                    } else {
                        "Confirmar y solicitar viaje"
                    }
                }
                if let Some(err) = ride_error() {
                    p { class: "ride-request-error", role: "alert", "{err}" }
                }
            } else if !can_estimate {
                p { class: "ride-estimate-empty",
                    "Elige un origen y un destino en el mapa para ver la tarifa."
                }
            }
        }
    }
}

/// Estado de la conexion en tiempo real que muestra esta pantalla — mismo
/// criterio que `nearby_rides::RealtimeStatus`: agrega `Unavailable` a
/// `moto_core::realtime::ConnectionState` y no expone el numero de intento
/// de reconexion, que no le sirve al pasajero.
#[derive(Debug, Clone, PartialEq)]
enum RideRealtimeStatus {
    Connecting,
    Connected,
    Reconnecting,
    Unavailable(String),
}

#[derive(Props, Clone, PartialEq)]
struct RideTrackingPanelProps {
    initial_ride: Ride,
    on_ride_updated: EventHandler<Ride>,
}

/// Seguimiento en tiempo real de un viaje activo (issue #20).
///
/// Al montarse vuelve a pedir el viaje por `GET /api/v1/rides/{ride}`: no
/// confia solo en `initial_ride` (el estado que ya tenia `RideEstimateScreen`
/// de cuando lo solicito), que puede haber quedado desactualizado si esta
/// pantalla se reabrio despues de un tiempo. Ese fetch no bloquea el resto
/// de la pantalla si falla por algo que no sea la sesion — ya hay algo que
/// mostrar con `initial_ride`, y el canal en tiempo real puede seguir
/// aportando actualizaciones igual.
///
/// Se suscribe al canal privado `ride.{id}` con el mismo patron de
/// sondeo/reconexion que `NearbyRidesList` (`nearby_rides.rs`): la decision
/// de cuando suscribirse, reconectar o cortar el loop vive en
/// `moto_core::realtime` (`decide_poll_action`, `decide_subscribe_failure`),
/// testeada aparte. Cada `status.changed` se le avisa al padre via
/// `on_ride_updated` para que decida si sigue ofreciendo "Cancelar
/// solicitud" (issue #15) y que texto de estado mostrar; `location.updated`
/// solo mueve el marcador del conductor en el mapa de esta misma pantalla.
#[component]
fn RideTrackingPanel(props: RideTrackingPanelProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let realtime_config = use_context::<RealtimeConfig>();

    let ride_id = props.initial_ride.id;
    let on_ride_updated = props.on_ride_updated;

    let mut tracking = use_signal(|| RideTracking::new(props.initial_ride.clone()));
    let mut status = use_signal(|| RideRealtimeStatus::Connecting);
    // Mismo criterio que el resto de los loops de sondeo de esta app: evita
    // levantar un segundo loop si el efecto se vuelve a disparar (p.ej.
    // porque `initial_ride` cambio de valor en un re-render).
    let mut started = use_signal(|| false);

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);

        let Some(token) = session.token() else {
            status.set(RideRealtimeStatus::Unavailable(
                "La sesion expiro. Inicia sesion de nuevo.".to_string(),
            ));
            return;
        };

        let api_client = api_client.clone();
        let storage = storage.clone();
        let realtime_config = realtime_config.clone();

        spawn(async move {
            let mut current_token = token;

            match api_client.get_ride(&current_token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed.clone(), storage.as_ref());
                        current_token = refreshed;
                    }
                    tracking.with_mut(|t| t.ride = fetch.data.clone());
                    on_ride_updated.call(fetch.data);
                }
                Err(GetRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                    return;
                }
                Err(_) => {}
            }

            let Some(ws_url) = realtime_config.ws_url.clone() else {
                status.set(RideRealtimeStatus::Unavailable(
                    "El tiempo real no esta configurado en este entorno.".to_string(),
                ));
                return;
            };

            let mut client = match RealtimeClient::connect(api_client, ws_url) {
                Ok(client) => client,
                Err(err) => {
                    status.set(RideRealtimeStatus::Unavailable(err));
                    return;
                }
            };
            let mut subscribed = false;
            let bare_channel = format!("ride.{ride_id}");
            let full_channel = format!("private-{bare_channel}");

            loop {
                for event in client.poll_events() {
                    if event.channel != full_channel {
                        continue;
                    }

                    let is_status_change = event.event == "status.changed";
                    tracking.with_mut(|t| {
                        apply_ride_tracking_event(t, &event.event, &event.data);
                    });
                    if is_status_change {
                        on_ride_updated.call(tracking.read().ride.clone());
                    }
                }

                let state = client.state();
                match &state {
                    ConnectionState::Connecting => status.set(RideRealtimeStatus::Connecting),
                    ConnectionState::Reconnecting { .. } => {
                        status.set(RideRealtimeStatus::Reconnecting)
                    }
                    ConnectionState::Connected => {}
                }

                let decision = decide_poll_action(&state, subscribed);
                subscribed = decision.subscribed;

                match decision.action {
                    PollAction::Wait => {}
                    PollAction::Subscribe => {
                        match client.subscribe(&current_token, &bare_channel).await {
                            Ok(()) => {
                                subscribed = true;
                                status.set(RideRealtimeStatus::Connected);
                            }
                            Err(err) => {
                                status.set(RideRealtimeStatus::Unavailable(err.to_string()));
                                match decide_subscribe_failure(&err) {
                                    SubscribeFailureAction::Retry => {}
                                    SubscribeFailureAction::LogoutAndStop => {
                                        session.logout(storage.as_ref());
                                        break;
                                    }
                                    SubscribeFailureAction::Stop => break,
                                }
                            }
                        }
                    }
                    // `RealtimeClient` no reconecta solo: el `Delay` de mas
                    // abajo ya espacia estos intentos, mismo criterio que
                    // `NearbyRidesList`.
                    PollAction::Reconnect => {
                        if let Err(err) = client.reconnect() {
                            status.set(RideRealtimeStatus::Unavailable(err));
                            break;
                        }
                    }
                }

                Delay::new(RIDE_TRACKING_POLL_INTERVAL).await;
            }
        });
    });

    let current = tracking.read();
    let mut markers = vec![
        MapMarker {
            lat: current.ride.origin.latitude,
            lng: current.ride.origin.longitude,
            label: Some("Origen".to_string()),
        },
        MapMarker {
            lat: current.ride.destination.latitude,
            lng: current.ride.destination.longitude,
            label: Some("Destino".to_string()),
        },
    ];
    // El mapa sigue al conductor cuando ya se conoce su posicion: es lo que
    // el pasajero quiere ver para saber cuando va a llegar (criterio de
    // aceptacion del issue). Sin ubicacion todavia (viaje `requested`, o
    // recien `accepted` antes del primer `location.updated`), centra en el
    // origen.
    let (center_lat, center_lng) = if let Some(location) = current.driver_location {
        markers.push(MapMarker {
            lat: location.latitude,
            lng: location.longitude,
            label: Some("Tu conductor".to_string()),
        });
        (location.latitude, location.longitude)
    } else {
        (current.ride.origin.latitude, current.ride.origin.longitude)
    };
    drop(current);

    rsx! {
        div { class: "ride-tracking-panel",
            match status() {
                RideRealtimeStatus::Connecting => rsx! {
                    p { class: "ride-tracking-status", "Conectando..." }
                },
                RideRealtimeStatus::Reconnecting => rsx! {
                    p { class: "ride-tracking-status", "Reconectando..." }
                },
                RideRealtimeStatus::Unavailable(message) => rsx! {
                    p { class: "ride-tracking-error", role: "alert", "{message}" }
                },
                RideRealtimeStatus::Connected => rsx! {
                    p { class: "ride-tracking-status", "Seguimiento en vivo." }
                },
            }
            div { class: "ride-tracking-map", style: "height: 320px;",
                MapView { center_lat, center_lng, markers }
            }
        }
    }
}

/// Texto para el pasajero segun el estado del viaje recien solicitado
/// (`Ride::status`). Justo despues de `POST /api/v1/rides` el viaje siempre
/// nace `requested`; los demas casos son los que reporta `RideTrackingPanel`
/// (issue #20) a medida que el viaje avanza.
fn ride_status_label(status: RideStatus) -> &'static str {
    match status {
        RideStatus::Requested => "Esperando a que un conductor lo acepte.",
        RideStatus::Accepted => "Un conductor acepto tu viaje.",
        RideStatus::InProgress => "Tu viaje esta en curso.",
        RideStatus::Completed => "Tu viaje ya se completo.",
        RideStatus::Cancelled => "Este viaje fue cancelado.",
    }
}
