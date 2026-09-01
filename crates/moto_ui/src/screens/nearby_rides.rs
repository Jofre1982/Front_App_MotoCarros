//! Pantalla de solicitudes de viaje cercanas (conductor) — issues #16 y #17.
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
//! de tiempo real no reconecta ni resuscribe solo (ver `RealtimeClient`),
//! asi que esta pantalla lo hace por su cuenta en cada vuelta de su loop de
//! sondeo — pero la decision de *cuando* suscribirse, reconectar o cortar
//! el loop, y el dedup de eventos por `ride_id`, vive en
//! `moto_core::realtime` (`decide_poll_action`, `decide_subscribe_failure`)
//! y `moto_core::models` (`apply_nearby_ride_event`): logica pura y
//! testeada ahi, no en este componente Dioxus.
//!
//! Cada solicitud de la lista ofrece un boton "Aceptar" que llama a
//! `POST /api/v1/rides/{ride}/accept` (`ApiClient::accept_ride`, issue #17).
//! Al aceptar con exito, la pantalla deja de mostrar la lista y muestra el
//! viaje aceptado (con conductor asignado) — aceptar un segundo viaje no es
//! posible mientras el conductor tenga uno activo (`AcceptRideError::Validation`
//! del backend), asi que no tiene sentido seguir ofreciendo la lista despues.
//! Si el backend responde 409 (`AcceptRideError::Conflict`, otro conductor lo
//! acepto primero — carrera documentada en `openapi.yaml`), la solicitud se
//! saca de la lista con un mensaje explicito en vez de un error generico, tal
//! como pide el criterio de aceptacion del issue.
//!
//! Mientras el viaje aceptado siga en `accepted`, se ofrece un boton
//! "Iniciar viaje" que llama a `POST /api/v1/rides/{ride}/start`
//! (`ApiClient::start_ride`, issue #18) — la condicion de estado es lo que
//! hace que el boton no este disponible si el viaje ya paso a otro estado
//! (criterio de aceptacion del issue). Al iniciar con exito, la pantalla
//! reemplaza el viaje en pantalla por la version devuelta por el backend
//! (ahora `in_progress`, con `started_at`) en vez de solo esconder el boton.
//!
//! Mientras el viaje siga en `in_progress`, `ShareLocationPanel` publica
//! periodicamente la posicion del conductor con
//! `POST /api/v1/rides/{ride}/location` (`ApiClient::share_ride_location`,
//! issue #19). La posicion misma la consigue un `LocationProvider` inyectado
//! por plataforma (web/movil, ver `moto_core::location`) — este componente
//! solo decide cuando pedirla y que hacer con el resultado, sin saber como
//! se obtiene. Si el proveedor devuelve `LocationError::PermissionDenied`,
//! el panel lo muestra explicitamente y deja de intentar: no tiene sentido
//! seguir pidiendo permiso sin que el usuario actue. Cualquier otro fallo
//! (de red, del backend, o del propio proveedor) se trata como transitorio y
//! se reintenta en el siguiente ciclo, sin dejar el panel en un estado de
//! error permanente.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    AcceptRideError, ApiClient, AuthenticatedRequestError, GetVehicleError, ShareRideLocationError,
    StartRideError,
};
use moto_core::location::{LocationError, LocationProvider};
use moto_core::models::{NearbyRideRequest, Ride, RideStatus, apply_nearby_ride_event};
use moto_core::realtime::{
    ConnectionState, PollAction, RealtimeClient, RealtimeConfig, SubscribeFailureAction,
    decide_poll_action, decide_subscribe_failure,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::register_vehicle::RegisterVehicleScreen;

/// Intervalo entre sondeos de `RealtimeClient::poll_events` — no hay forma de
/// que el transporte avise por si solo cuando llega un frame nuevo (ver
/// `.claude/STANDARDS.md`, el crate no tiene runtime propio), asi que la
/// pantalla lo consulta activamente a este ritmo.
const POLL_INTERVAL: Duration = Duration::from_millis(700);

/// Intervalo entre publicaciones de ubicacion de `ShareLocationPanel` (issue
/// #19). Mas espaciado que `POLL_INTERVAL`: a diferencia del sondeo de
/// eventos, pedirle la posicion al dispositivo tiene un costo real
/// (bateria, y en el navegador un posible reprompt de permiso), y el
/// pasajero no necesita una actualizacion mas fina que esta para seguir el
/// viaje en el mapa.
const LOCATION_SHARE_INTERVAL: Duration = Duration::from_secs(10);

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
    let mut session = use_context::<SessionState>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let realtime_config = use_context::<RealtimeConfig>();

    let mut status = use_signal(|| RealtimeStatus::Connecting);
    let mut requests = use_signal(Vec::<NearbyRideRequest>::new);
    // Viaje que este conductor acabo de aceptar (issue #17). Una vez
    // presente, la pantalla deja de mostrar la lista: el backend no deja
    // aceptar un segundo viaje mientras el conductor tenga uno activo, asi
    // que seguir ofreciendo la lista no serviria de nada.
    let mut accepted_ride = use_signal(|| None::<Ride>);
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
        let storage = storage.clone();
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
                    requests.with_mut(|list| {
                        apply_nearby_ride_event(list, &event.event, &event.data);
                    });
                }

                // La decision de cuando suscribirse/reconectar/cortar el
                // loop vive en `moto_core::realtime` (`decide_poll_action`,
                // `decide_subscribe_failure`) para poder testearla sin
                // Dioxus ni un socket real — este bloque solo ejecuta esa
                // decision y actualiza el estado visible.
                let state = client.state();
                match &state {
                    ConnectionState::Connecting => status.set(RealtimeStatus::Connecting),
                    ConnectionState::Reconnecting { .. } => {
                        status.set(RealtimeStatus::Reconnecting)
                    }
                    ConnectionState::Connected => {}
                }

                let decision = decide_poll_action(&state, subscribed);
                subscribed = decision.subscribed;

                match decision.action {
                    PollAction::Wait => {}
                    PollAction::Subscribe => match client.subscribe(&token, &bare_channel).await {
                        Ok(()) => {
                            subscribed = true;
                            status.set(RealtimeStatus::Connected);
                        }
                        Err(err) => {
                            status.set(RealtimeStatus::Unavailable(err.to_string()));
                            match decide_subscribe_failure(&err) {
                                SubscribeFailureAction::Retry => {}
                                SubscribeFailureAction::LogoutAndStop => {
                                    session.logout(storage.as_ref());
                                    break;
                                }
                                SubscribeFailureAction::Stop => break,
                            }
                        }
                    },
                    // `RealtimeClient` no reconecta solo (ver doc comment de
                    // `reconnect()`): el caller debe reabrir la conexion
                    // explicitamente. El `Delay` de mas abajo ya espacia
                    // estos intentos a `POLL_INTERVAL`, asi que no hace
                    // falta backoff propio. Un fallo aqui suele ser la misma
                    // URL invalida que fallaria en cada intento (igual que
                    // el `connect()` inicial de mas arriba), asi que se
                    // reporta como no disponible y se corta el loop en vez
                    // de reintentar para siempre.
                    PollAction::Reconnect => {
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
            if let Some(ride) = accepted_ride() {
                div { class: "nearby-ride-accepted",
                    p { "Aceptaste el viaje." }
                    p { "Tarifa estimada: {ride.currency} {ride.estimated_fare}" }
                    if ride.status == RideStatus::Accepted {
                        StartRideButton {
                            ride_id: ride.id,
                            on_started: move |started: Ride| {
                                accepted_ride.set(Some(started));
                            },
                        }
                    } else if ride.status == RideStatus::InProgress {
                        ShareLocationPanel { ride_id: ride.id }
                    }
                }
            } else {
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
                                    NearbyRideRow {
                                        key: "{request.ride_id}",
                                        request: request.clone(),
                                        on_accepted: move |ride: Ride| {
                                            let ride_id = ride.id;
                                            requests.with_mut(|list| list.retain(|r| r.ride_id != ride_id));
                                            accepted_ride.set(Some(ride));
                                        },
                                        on_unavailable: move |ride_id: u64| {
                                            requests.with_mut(|list| list.retain(|r| r.ride_id != ride_id));
                                        },
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NearbyRideRowProps {
    request: NearbyRideRequest,
    on_accepted: EventHandler<Ride>,
    on_unavailable: EventHandler<u64>,
}

/// Una solicitud de la lista con su boton "Aceptar" (issue #17). Componente
/// aparte (en vez de logica inline en el `for` de `NearbyRidesList`) para que
/// cada fila tenga su propio estado de carga/error sin afectar a las demas —
/// dos solicitudes pueden estar aceptandose (o fallando) al mismo tiempo.
#[component]
fn NearbyRideRow(props: NearbyRideRowProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_accepting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let ride_id = props.request.ride_id;
    let on_accepted = props.on_accepted;
    let on_unavailable = props.on_unavailable;

    let on_accept_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_accepting.set(true);
            error.set(None);

            match api_client.accept_ride(&token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_accepted.call(fetch.data);
                }
                Err(AcceptRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(AcceptRideError::Conflict) => {
                    on_unavailable.call(ride_id);
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }

            is_accepting.set(false);
        });
    };

    rsx! {
        li { class: "nearby-ride-request",
            p { "Origen: {props.request.origin.latitude}, {props.request.origin.longitude}" }
            p {
                "Destino: {props.request.destination.latitude}, {props.request.destination.longitude}"
            }
            p { "Tarifa estimada: {props.request.currency} {props.request.estimated_fare}" }
            button {
                r#type: "button",
                class: "nearby-ride-accept-button",
                disabled: is_accepting(),
                onclick: on_accept_click,
                if is_accepting() {
                    "Aceptando..."
                } else {
                    "Aceptar"
                }
            }
            if let Some(message) = error() {
                p { class: "nearby-ride-accept-error", role: "alert", "{message}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct StartRideButtonProps {
    ride_id: u64,
    on_started: EventHandler<Ride>,
}

/// Boton "Iniciar viaje" del viaje aceptado (issue #18). Componente aparte,
/// igual que `NearbyRideRow`, para que su estado de carga/error no dependa
/// del resto de la pantalla.
#[component]
fn StartRideButton(props: StartRideButtonProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_starting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let ride_id = props.ride_id;
    let on_started = props.on_started;

    let on_start_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_starting.set(true);
            error.set(None);

            match api_client.start_ride(&token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_started.call(fetch.data);
                }
                Err(StartRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }

            is_starting.set(false);
        });
    };

    rsx! {
        div { class: "nearby-ride-start",
            button {
                r#type: "button",
                class: "nearby-ride-start-button",
                disabled: is_starting(),
                onclick: on_start_click,
                if is_starting() {
                    "Iniciando..."
                } else {
                    "Iniciar viaje"
                }
            }
            if let Some(message) = error() {
                p { class: "nearby-ride-start-error", role: "alert", "{message}" }
            }
        }
    }
}

/// Estado visible de `ShareLocationPanel` (issue #19).
#[derive(Debug, Clone, PartialEq)]
enum SharingStatus {
    Sharing,
    PermissionDenied,
    Error(String),
}

#[derive(Props, Clone, PartialEq)]
struct ShareLocationPanelProps {
    ride_id: u64,
}

/// Publica la posicion del conductor mientras el viaje siga `in_progress`
/// (issue #19). Componente aparte, igual que `StartRideButton`, para que su
/// ciclo de vida (arranca al montarse, Dioxus lo cancela al desmontarse)
/// quede acotado a mientras el viaje este en curso: cuando el viaje pase a
/// `completed`/`cancelled`, `NearbyRidesList` deja de renderizar este
/// componente y el loop de mas abajo se corta solo.
#[component]
fn ShareLocationPanel(props: ShareLocationPanelProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let location_provider = use_context::<Arc<dyn LocationProvider>>();

    let mut status = use_signal(|| SharingStatus::Sharing);
    // Mismo criterio que el resto de los loops de esta pantalla: evita
    // levantar un segundo loop si el efecto se vuelve a disparar.
    let mut started = use_signal(|| false);

    let ride_id = props.ride_id;

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);

        let Some(token) = session.token() else {
            status.set(SharingStatus::Error(
                "La sesion expiro. Inicia sesion de nuevo.".to_string(),
            ));
            return;
        };

        let api_client = api_client.clone();
        let storage = storage.clone();
        let location_provider = location_provider.clone();

        spawn(async move {
            let mut current_token = token;

            loop {
                match location_provider.current_position().await {
                    Ok(coordinates) => {
                        match api_client
                            .share_ride_location(&current_token, ride_id, coordinates)
                            .await
                        {
                            Ok(fetch) => {
                                if let Some(refreshed) = fetch.refreshed_token {
                                    session.update_token(refreshed.clone(), storage.as_ref());
                                    current_token = refreshed;
                                }
                                status.set(SharingStatus::Sharing);
                            }
                            Err(ShareRideLocationError::SessionExpired) => {
                                session.logout(storage.as_ref());
                                break;
                            }
                            Err(err) => {
                                status.set(SharingStatus::Error(err.to_string()));
                            }
                        }
                    }
                    Err(LocationError::PermissionDenied) => {
                        status.set(SharingStatus::PermissionDenied);
                        break;
                    }
                    Err(LocationError::Unavailable(message)) => {
                        status.set(SharingStatus::Error(message));
                    }
                }

                Delay::new(LOCATION_SHARE_INTERVAL).await;
            }
        });
    });

    rsx! {
        div { class: "share-location-panel",
            p { "Viaje en curso." }
            match status() {
                SharingStatus::Sharing => rsx! {
                    p { class: "share-location-status", "Compartiendo tu ubicacion..." }
                },
                SharingStatus::PermissionDenied => rsx! {
                    p { class: "share-location-error", role: "alert",
                        "Habilita el permiso de ubicacion para compartirla durante el viaje."
                    }
                },
                SharingStatus::Error(message) => rsx! {
                    p { class: "share-location-error", role: "alert", "{message}" }
                },
            }
        }
    }
}
