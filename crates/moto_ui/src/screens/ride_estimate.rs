//! Pantalla de tarifa estimada y solicitud de viaje (pasajero) — issues #13
//! y #14.
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

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, CancelRideError, EstimateRideError, RequestRideError};
use moto_core::models::{Coordinates, Ride, RideEstimate, RideStatus};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::{MapMarker, MapView};

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

/// Texto para el pasajero segun el estado del viaje recien solicitado
/// (`Ride::status`). Justo despues de `POST /api/v1/rides` el viaje siempre
/// nace `requested`; los demas casos quedan cubiertos para cuando esta misma
/// pantalla se reutilice con datos de un viaje ya en curso (historia #20,
/// fuera de alcance de #14).
fn ride_status_label(status: RideStatus) -> &'static str {
    match status {
        RideStatus::Requested => "Esperando a que un conductor lo acepte.",
        RideStatus::Accepted => "Un conductor acepto tu viaje.",
        RideStatus::InProgress => "Tu viaje esta en curso.",
        RideStatus::Completed => "Tu viaje ya se completo.",
        RideStatus::Cancelled => "Este viaje fue cancelado.",
    }
}
