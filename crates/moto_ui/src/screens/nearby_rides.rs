//! Pantalla de solicitudes de viaje cercanas (conductor) — issue #16.
//!
//! Al entrar, primero valida que el conductor ya tenga un vehiculo
//! registrado con `GET /api/v1/me/vehicle` — mismo criterio y mismo 404
//! (`GetVehicleError::NotFound`) que usa `VehicleScreen` (issues #11/#12)
//! para decidir si redirige al registro. Un conductor sin vehiculo nunca
//! llega a ver la lista.
//!
//! Con vehiculo confirmado, se suscribe al canal privado `driver.{id}` de
//! Reverb (`id` = `User.id`, ver `moto_core::realtime::RealtimeClient`) y
//! escucha dos eventos, documentados por el backend en el propio issue:
//! `ride.requested` (agrega una solicitud a la lista) y `ride.unavailable`
//! (otro conductor la acepto primero, se quita de la lista). El transporte
//! de tiempo real no reconecta ni resuscribe solo (ver
//! `RealtimeClient`), asi que esta pantalla lo hace por su cuenta: llama
//! `reconnect()` explicitamente cada vez que el estado pasa a
//! `Reconnecting` y vuelve a suscribirse cada vez que pasa a `Connected`
//! sin una suscripcion vigente.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{ApiClient, AuthenticatedRequestError, GetVehicleError};
use moto_core::models::{NearbyRideRequest, RideNoLongerAvailable};
use moto_core::realtime::{ConnectionState, RealtimeClient, RealtimeConfig};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::register_vehicle::RegisterVehicleScreen;

/// Intervalo entre sondeos de `RealtimeClient::poll_events` — no hay forma de
/// que el transporte avise por si solo cuando llega un frame nuevo (ver
/// `.claude/STANDARDS.md`, el crate no tiene runtime propio), asi que la
/// pantalla lo consulta activamente a este ritmo.
const POLL_INTERVAL: Duration = Duration::from_millis(700);

#[component]
pub fn NearbyRidesScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut not_registered = use_signal(|| false);
    let mut driver_id = use_signal(|| None::<u64>);
    // Misma guarda que `VehicleScreen`/`ProfileScreen`: el efecto lee
    // `session.token()` en su primera corrida, lo que lo suscribe a ese
    // signal, y el fetch puede terminar escribiendo en `session` (refresh de
    // token, logout) — sin esta bandera reprogramaria el efecto en un loop.
    let mut has_fetched = use_signal(|| false);

    use_effect(move || {
        if has_fetched() {
            return;
        }

        let Some(token) = session.token() else {
            return;
        };
        has_fetched.set(true);

        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            load_error.set(None);

            // `GET /me` primero: esta pantalla necesita el id de cuenta para
            // el canal `driver.{id}`, y no se puede asumir que `ProfileScreen`
            // (issue #9) ya corrio antes en esta sesion — `Home` solo ofrece
            // esta pestana a conductores, pero eso no obliga un orden de
            // navegacion.
            let token_for_vehicle = match api_client.me(&token).await {
                Ok(fetch) => {
                    let mut current_token = token.clone();
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed.clone(), storage.as_ref());
                        current_token = refreshed;
                    }
                    driver_id.set(Some(fetch.data.id));
                    session.set_user(fetch.data);
                    current_token
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                    is_loading.set(false);
                    return;
                }
                Err(err) => {
                    load_error.set(Some(err.to_string()));
                    is_loading.set(false);
                    return;
                }
            };

            match api_client.get_vehicle(&token_for_vehicle).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                }
                Err(GetVehicleError::NotFound) => {
                    not_registered.set(true);
                }
                Err(GetVehicleError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    load_error.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    });

    rsx! {
        if not_registered() {
            RegisterVehicleScreen {}
        } else if is_loading() {
            div { class: "nearby-rides-screen", p { "Cargando..." } }
        } else if let Some(message) = load_error() {
            div { class: "nearby-rides-screen",
                p { class: "nearby-rides-error", role: "alert", "{message}" }
            }
        } else if let Some(id) = driver_id() {
            NearbyRidesList { driver_id: id }
        }
    }
}

/// Estado de la conexion en tiempo real, tal como lo muestra esta pantalla —
/// distinto de `moto_core::realtime::ConnectionState`: agrega el caso
/// `Unavailable` (config ausente o error irrecuperable) y no expone el
/// numero de intento de reconexion, que no le sirve al usuario final.
#[derive(Debug, Clone, PartialEq)]
enum RealtimeStatus {
    Connecting,
    Connected,
    Reconnecting,
    Unavailable(String),
}

#[derive(Props, Clone, PartialEq)]
struct NearbyRidesListProps {
    driver_id: u64,
}

#[component]
fn NearbyRidesList(props: NearbyRidesListProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let session = use_context::<SessionState>();
    let realtime_config = use_context::<RealtimeConfig>();

    let mut status = use_signal(|| RealtimeStatus::Connecting);
    let mut requests = use_signal(Vec::<NearbyRideRequest>::new);
    // El loop de sondeo corre para siempre mientras la pantalla este
    // montada (Dioxus cancela la tarea al desmontar el componente, ver
    // `Home` en `moto_ui/src/lib.rs`) — esta bandera solo evita levantar un
    // segundo loop si el efecto se vuelve a disparar.
    let mut started = use_signal(|| false);

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);

        let Some(ws_url) = realtime_config.ws_url.clone() else {
            status.set(RealtimeStatus::Unavailable(
                "El tiempo real no esta configurado en este entorno.".to_string(),
            ));
            return;
        };
        let Some(token) = session.token() else {
            status.set(RealtimeStatus::Unavailable(
                "La sesion expiro. Inicia sesion de nuevo.".to_string(),
            ));
            return;
        };

        let api_client = api_client.clone();
        let bare_channel = format!("driver.{}", props.driver_id);
        let full_channel = format!("private-{bare_channel}");

        spawn(async move {
            let mut client = match RealtimeClient::connect(api_client, ws_url) {
                Ok(client) => client,
                Err(err) => {
                    status.set(RealtimeStatus::Unavailable(err));
                    return;
                }
            };
            let mut subscribed = false;

            loop {
                for event in client.poll_events() {
                    if event.channel != full_channel {
                        continue;
                    }
                    match event.event.as_str() {
                        "ride.requested" => {
                            if let Ok(request) =
                                serde_json::from_str::<NearbyRideRequest>(&event.data)
                            {
                                requests.with_mut(|list| {
                                    if !list.iter().any(|r| r.ride_id == request.ride_id) {
                                        list.push(request);
                                    }
                                });
                            }
                        }
                        "ride.unavailable" => {
                            if let Ok(gone) =
                                serde_json::from_str::<RideNoLongerAvailable>(&event.data)
                            {
                                requests.with_mut(|list| {
                                    list.retain(|r| r.ride_id != gone.ride_id);
                                });
                            }
                        }
                        _ => {}
                    }
                }

                match client.state() {
                    ConnectionState::Connected if !subscribed => {
                        match client.subscribe(&token, &bare_channel).await {
                            Ok(()) => {
                                subscribed = true;
                                status.set(RealtimeStatus::Connected);
                            }
                            Err(err) => {
                                status.set(RealtimeStatus::Unavailable(err.to_string()));
                            }
                        }
                    }
                    ConnectionState::Connected => {}
                    ConnectionState::Connecting => {
                        subscribed = false;
                        status.set(RealtimeStatus::Connecting);
                    }
                    ConnectionState::Reconnecting { .. } => {
                        subscribed = false;
                        status.set(RealtimeStatus::Reconnecting);
                        // `RealtimeClient` no reconecta solo (ver doc comment
                        // de `reconnect()`): el caller debe reabrir la
                        // conexion explicitamente. El `Delay` de mas abajo ya
                        // espacia estos intentos a `POLL_INTERVAL`, asi que no
                        // hace falta backoff propio. Un fallo aqui suele ser
                        // la misma URL invalida que fallaria en cada intento
                        // (igual que el `connect()` inicial de mas arriba),
                        // asi que se reporta como no disponible y se corta el
                        // loop en vez de reintentar para siempre.
                        if let Err(err) = client.reconnect() {
                            status.set(RealtimeStatus::Unavailable(err));
                            break;
                        }
                    }
                }

                Delay::new(POLL_INTERVAL).await;
            }
        });
    });

    rsx! {
        div { class: "nearby-rides-screen",
            h2 { "Solicitudes cercanas" }
            match status() {
                RealtimeStatus::Connecting => rsx! {
                    p { "Conectando..." }
                },
                RealtimeStatus::Reconnecting => rsx! {
                    p { "Reconectando..." }
                },
                RealtimeStatus::Unavailable(message) => rsx! {
                    p { class: "nearby-rides-error", role: "alert", "{message}" }
                },
                RealtimeStatus::Connected => rsx! {
                    if requests().is_empty() {
                        p { class: "nearby-rides-empty", "No hay solicitudes disponibles por el momento." }
                    } else {
                        ul { class: "nearby-rides-list",
                            for request in requests() {
                                li { key: "{request.ride_id}", class: "nearby-ride-request",
                                    p {
                                        "Origen: {request.origin.latitude}, {request.origin.longitude}"
                                    }
                                    p {
                                        "Destino: {request.destination.latitude}, {request.destination.longitude}"
                                    }
                                    p { "Tarifa estimada: {request.currency} {request.estimated_fare}" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
