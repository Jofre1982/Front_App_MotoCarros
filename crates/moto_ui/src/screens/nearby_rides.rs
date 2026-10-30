//! Pantalla de solicitudes de viaje cercanas (conductor) — issues #16 y #17.
//!
//! Desde la historia #87 del backend (issue #74 de este repo), el destino
//! que trae el evento `ride.requested` es un sitio del catalogo (nombre,
//! sin coordenadas), no un punto libre — `NearbyRideRequest` en
//! `moto_core::models` refleja ese cambio. Antes de este fix, el modelo
//! seguia esperando coordenadas: como un `data` que no deserializa al tipo
//! esperado se ignora en silencio (ver `apply_nearby_ride_event`), el
//! conductor dejaba de recibir solicitudes nuevas sin ningun error visible.
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
//! Mientras el viaje aceptado siga en `accepted`, se ofrecen dos botones:
//! "Iniciar viaje", que llama a `POST /api/v1/rides/{ride}/start`
//! (`ApiClient::start_ride`, issue #18) — la condicion de estado es lo que
//! hace que el boton no este disponible si el viaje ya paso a otro estado
//! (criterio de aceptacion del issue). Al iniciar con exito, la pantalla
//! reemplaza el viaje en pantalla por la version devuelta por el backend
//! (ahora `in_progress`, con `started_at`) en vez de solo esconder el boton.
//! Y "Cancelar viaje", que llama a `POST /api/v1/rides/{ride}/cancel`
//! (`ApiClient::cancel_ride`, issue #22) para que el conductor libere un
//! viaje que acepto pero todavia no inicio. A diferencia de cuando cancela
//! el pasajero (issues #15/#21), aqui el viaje no queda `cancelled`: vuelve a
//! `requested` sin conductor asignado, para que otro conductor pueda
//! tomarlo (`RideCancellation::cancellation_fee_applies` llega `None` en
//! este caso, ver `moto_core::models`), asi que esta pantalla simplemente
//! deja de mostrarlo como "su" viaje aceptado y vuelve a la lista de
//! solicitudes cercanas, sin un panel de resultado como el de
//! `RideEstimateScreen`. Un viaje `in_progress` ya no se puede cancelar por
//! este endpoint segun `openapi.yaml` (solo completarse, historia #23), asi
//! que el boton no se ofrece en ese estado.
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
//!
//! Tambien mientras el viaje esta `in_progress`, se ofrece "Completar
//! viaje", que llama a `POST /api/v1/rides/{ride}/complete`
//! (`ApiClient::complete_ride`, issue #23) — la condicion de estado hace
//! que el boton no este disponible si el viaje no esta en curso (criterio
//! de aceptacion del issue). El backend dispara el cobro (historia #25 del
//! backend) en la misma operacion, asi que la respuesta ya trae
//! `completed_at`, `final_fare` y `payment` resueltos; la pantalla
//! reemplaza el viaje en pantalla por esa version y muestra un resumen
//! (tarifa final y estado del pago) en vez de navegar a una pantalla de
//! pago propia — la historia "Pagar el viaje al finalizar" (#24) sigue
//! bloqueada del lado del backend, no hay adonde navegar todavia.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    AcceptRideError, ApiClient, AuthenticatedRequestError, CancelRideError, CompleteRideError,
    GetRideError, GetVehicleError, ServiceActionError, ShareRideLocationError, StartRideError,
};
use moto_core::location::{LocationError, LocationProvider};
use moto_core::models::{
    Coordinates, NearbyRideRequest, Ride, RideStatus, Role, VehicleType, apply_nearby_ride_event,
    distance_km, distance_label, eta_minutes, format_cop, latest_active_ride,
};
use moto_core::realtime::{
    ConnectionState, PollAction, RealtimeClient, RealtimeConfig, SubscribeFailureAction,
    decide_poll_action, decide_subscribe_failure,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::register_vehicle::RegisterVehicleScreen;
use crate::big_notice::{BigNotice, StatusBanner, Tone};
use crate::map::PickupLocation;
use crate::price_offer::ContactCard;
use crate::reoffer::{ReofferBox, ReofferTarget, Reoffered};

/// Intervalo entre sondeos de `RealtimeClient::poll_events` — no hay forma de
/// que el transporte avise por si solo cuando llega un frame nuevo (ver
/// `.claude/STANDARDS.md`, el crate no tiene runtime propio), asi que la
/// pantalla lo consulta activamente a este ritmo.
const POLL_INTERVAL: Duration = Duration::from_millis(700);

/// Cada cuanto se vuelve a pedir `GET /rides/available` (backend #105).
/// Suficiente para que el pasajero no espere de mas, sin gastar datos del
/// celular del conductor.
const AVAILABLE_RIDES_INTERVAL: Duration = Duration::from_secs(8);

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
    let mut drives_motocarga = use_signal(|| false);
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
                    drives_motocarga.set(fetch.data.vehicle_type == VehicleType::Motocarga);
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
            NearbyRidesList { driver_id: id, drives_motocarga: drives_motocarga() }
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
    drives_motocarga: bool,
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
    let mut list_started = use_signal(|| false);
    // `true` cuando la lista ya se cargo al menos una vez por
    // `GET /rides/available`: desde ahi se muestra aunque el tiempo real no
    // conecte.
    let mut list_loaded = use_signal(|| false);
    // Donde esta este conductor (GPS), para ordenar la lista por cercania.
    let mut my_position = use_signal(|| None::<Coordinates>);
    // El viaje cuyo precio rechazo el pasajero: se muestra en una ventana
    // grande con la opcion de ofrecer otro precio.
    let mut rejected_ride = use_signal(|| None::<Ride>);
    // Ventana "¡El pasajero acepto tu precio!".
    let mut price_accepted_popup = use_signal(|| false);
    // Aviso cuando el pasajero cancela un viaje que este conductor ya tenia.
    let mut cancelled_notice = use_signal(|| false);
    let mut offer_watch_started = use_signal(|| false);

    // Mientras el pasajero no responda el precio (backend #107), se vuelve a
    // consultar el viaje: si lo acepta, se habilita "Iniciar viaje"; si lo
    // rechaza, el viaje ya no es de este conductor (403, o vuelve sin
    // conductor) y se regresa a la lista con un aviso.
    {
        let api_client = api_client.clone();
        let storage = storage.clone();
        let driver_id = props.driver_id;
        use_effect(move || {
            if offer_watch_started() {
                return;
            }
            offer_watch_started.set(true);
            let api_client = api_client.clone();
            let storage = storage.clone();
            spawn(async move {
                loop {
                    Delay::new(AVAILABLE_RIDES_INTERVAL).await;
                    // Se sigue el viaje mientras este activo: asi el conductor
                    // se entera si el pasajero acepta o rechaza el precio, o
                    // si cancela el viaje.
                    let waiting = accepted_ride
                        .peek()
                        .as_ref()
                        .filter(|ride| {
                            matches!(ride.status, RideStatus::Accepted | RideStatus::InProgress)
                        })
                        .map(|ride| ride.id);
                    let (Some(ride_id), Some(token)) = (waiting, session.token()) else {
                        continue;
                    };
                    match api_client.get_ride(&token, ride_id).await {
                        Ok(fetch) => {
                            if let Some(refreshed) = fetch.refreshed_token {
                                session.update_token(refreshed, storage.as_ref());
                            }
                            let still_mine = fetch
                                .data
                                .driver
                                .as_ref()
                                .is_some_and(|d| d.id == driver_id);
                            if fetch.data.status == RideStatus::Cancelled {
                                accepted_ride.set(None);
                                cancelled_notice.set(true);
                            } else if still_mine {
                                let was_waiting = accepted_ride
                                    .peek()
                                    .as_ref()
                                    .is_some_and(Ride::awaiting_price_response);
                                if was_waiting
                                    && !fetch.data.awaiting_price_response()
                                    && fetch.data.status == RideStatus::Accepted
                                {
                                    price_accepted_popup.set(true);
                                }
                                if accepted_ride.peek().as_ref() != Some(&fetch.data) {
                                    accepted_ride.set(Some(fetch.data));
                                }
                            } else {
                                let previous = accepted_ride.peek().clone();
                                accepted_ride.set(None);
                                rejected_ride.set(previous);
                            }
                        }
                        Err(GetRideError::Forbidden | GetRideError::NotFound) => {
                            let previous = accepted_ride.peek().clone();
                            accepted_ride.set(None);
                            rejected_ride.set(previous);
                        }
                        Err(GetRideError::SessionExpired) => {
                            session.logout(storage.as_ref());
                            break;
                        }
                        Err(_) => {}
                    }
                }
            });
        });
    }

    // La lista sale del servidor (`GET /rides/available`) y se refresca
    // cada `AVAILABLE_RIDES_INTERVAL`: el aviso en tiempo real solo le llega
    // a quien estaba conectado en el instante del pedido, y un conductor que
    // entraba despues nunca veia el viaje (reportado probando el APK). Al
    // entrar tambien se recupera el viaje que este conductor ya tenia
    // aceptado, para no perderlo si cerro la app.
    {
        let api_client = api_client.clone();
        let storage = storage.clone();
        let location_provider = use_context::<Arc<dyn LocationProvider>>();
        use_effect(move || {
            if list_started() {
                return;
            }
            let Some(token) = session.token() else {
                return;
            };
            list_started.set(true);
            let api_client = api_client.clone();
            let storage = storage.clone();
            let location_provider = location_provider.clone();

            spawn(async move {
                let mut round: u32 = 0;
                let active = match api_client.ride_history(&token, Some(Role::Driver)).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        latest_active_ride(&fetch.data)
                    }
                    Err(_) => None,
                };
                if active.is_some() {
                    accepted_ride.set(active);
                }
                loop {
                    // Cada ~30 s: posicion del conductor para ordenar la lista
                    // y, si esta recibiendo viajes, guardarla en el servidor
                    // (asi le llega el aviso de los viajes cercanos).
                    if accepted_ride().is_none()
                        && round.is_multiple_of(4)
                        && let Ok(point) = location_provider.current_position().await
                    {
                        my_position.set(Some(point));
                        if session.ride_availability() == Some(true)
                            && let Some(current) = session.token()
                            && let Ok(fetch) = api_client
                                .update_ride_availability(&current, true, Some(point))
                                .await
                            && let Some(refreshed) = fetch.refreshed_token
                        {
                            session.update_token(refreshed, storage.as_ref());
                        }
                    }
                    round = round.wrapping_add(1);
                    if accepted_ride().is_none() {
                        let current = session.token().unwrap_or_else(|| token.clone());
                        match api_client.available_rides(&current).await {
                            Ok(fetch) => {
                                if let Some(refreshed) = fetch.refreshed_token {
                                    session.update_token(refreshed, storage.as_ref());
                                }
                                requests
                                    .set(fetch.data.iter().map(NearbyRideRequest::from).collect());
                                list_loaded.set(true);
                            }
                            Err(AuthenticatedRequestError::SessionExpired) => {
                                session.logout(storage.as_ref());
                                break;
                            }
                            // Un fallo de red se reintenta en la siguiente vuelta.
                            Err(_) => {}
                        }
                    }
                    Delay::new(AVAILABLE_RIDES_INTERVAL).await;
                }
            });
        });
    }

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

    // Los mas cercanos primero, con su distancia (si hay GPS).
    let sorted_requests = sort_by_distance(requests(), my_position());

    rsx! {
        div { class: "nearby-rides-screen",
            h2 { "Solicitudes cercanas" }
            if let Some(ride) = accepted_ride() {
                div { class: "nearby-ride-accepted",
                    {
                        let (tone, emoji, title, detail) = driver_ride_banner(&ride);
                        rsx! { StatusBanner { tone, emoji, title, detail } }
                    }
                    if price_accepted_popup() {
                        BigNotice {
                            tone: Tone::Done,
                            emoji: "✅",
                            title: "¡El pasajero aceptó tu precio!",
                            detail: ride.agreed_price.map(|price| format!("Precio acordado: {}. Ve a recogerlo.", format_cop(price))),
                            button {
                                r#type: "button",
                                class: "big-notice-button",
                                onclick: move |_| price_accepted_popup.set(false),
                                "¡Voy en camino!"
                            }
                        }
                    }
                    if ride.status == RideStatus::Accepted {
                        PickupLocation { lat: ride.origin.latitude, lng: ride.origin.longitude }
                    }
                    p { "Destino: {ride.destination_label()}" }
                    if let Some(counterpart) = ride.counterpart.clone() {
                        ContactCard { title: "Tu pasajero", counterpart }
                    }
                    p { "{ride.price_label()}" }
                    if ride.awaiting_price_response() {
                        CancelAcceptedRideButton {
                            ride_id: ride.id,
                            on_cancelled: move |_| {
                                accepted_ride.set(None);
                            },
                        }
                    } else if ride.status == RideStatus::Accepted {
                        // Camino a recoger: comparte la ubicacion para que el
                        // pasajero vea "llega en X min" (backend #111).
                        ShareLocationPanel { ride_id: ride.id }
                        StartRideButton {
                            ride_id: ride.id,
                            on_started: move |started: Ride| {
                                accepted_ride.set(Some(started));
                            },
                        }
                        CancelAcceptedRideButton {
                            ride_id: ride.id,
                            on_cancelled: move |_| {
                                accepted_ride.set(None);
                            },
                        }
                    } else if ride.status == RideStatus::InProgress {
                        ShareLocationPanel { ride_id: ride.id }
                        CompleteRideButton {
                            ride_id: ride.id,
                            on_completed: move |completed: Ride| {
                                accepted_ride.set(Some(completed));
                            },
                        }
                    } else if ride.status == RideStatus::Completed {
                        div { class: "nearby-ride-completed",
                            if let Some(payment) = &ride.payment {
                                p {
                                    "Pago: "
                                    {
                                        match payment.status {
                                            moto_core::models::PaymentStatus::Paid => "pagado",
                                            moto_core::models::PaymentStatus::Pending => "pendiente",
                                            moto_core::models::PaymentStatus::Failed => "fallido",
                                        }
                                    }
                                }
                            }
                            button {
                                r#type: "button",
                                class: "nearby-ride-done-button",
                                onclick: move |_| accepted_ride.set(None),
                                "Listo, volver a la lista"
                            }
                        }
                    }
                }
            } else if list_loaded() || status() == RealtimeStatus::Connected {
                if cancelled_notice() {
                    p { class: "nearby-rides-error", role: "alert",
                        "El pasajero cancelo el viaje."
                        button {
                            r#type: "button",
                            class: "nearby-rides-dismiss-link",
                            onclick: move |_| cancelled_notice.set(false),
                            "Entendido"
                        }
                    }
                }
                if let Some(ride) = rejected_ride() {
                    BigNotice {
                        tone: Tone::Bad,
                        emoji: "😕",
                        title: "El pasajero no aceptó tu precio",
                        detail: match ride.agreed_price {
                            Some(price) => format!("Ofreciste {}. Puedes ofrecer otro precio:", format_cop(price)),
                            None => "Puedes ofrecer otro precio:".to_string(),
                        },
                        ReofferBox {
                            target: ReofferTarget::Ride(ride.id),
                            on_sent: move |sent: Reoffered| {
                                if let Reoffered::Ride(offered) = sent {
                                    let offered_id = offered.id;
                                    requests.with_mut(|list| list.retain(|r| r.ride_id != offered_id));
                                    accepted_ride.set(Some(offered));
                                }
                                rejected_ride.set(None);
                            },
                            on_unavailable: move |_| {
                                let gone = rejected_ride.peek().as_ref().map(|r| r.id);
                                requests.with_mut(|list| list.retain(|r| Some(r.ride_id) != gone));
                                rejected_ride.set(None);
                            },
                        }
                        div { class: "reoffer-details",
                            if ride.is_pickup_only() {
                                p { class: "nearby-ride-mode-badge", "Solo recogida · precio a acordar" }
                            }
                            PickupLocation { lat: ride.origin.latitude, lng: ride.origin.longitude }
                            p { "Destino: {ride.destination_label()}" }
                            p { "Pasajeros: {ride.passenger_count}" }
                        }
                        button {
                            r#type: "button",
                            class: "big-notice-button big-notice-button-secondary",
                            onclick: move |_| rejected_ride.set(None),
                            "No, buscar otro viaje"
                        }
                    }
                }
                if requests().is_empty() {
                    if props.drives_motocarga {
                        p { class: "nearby-rides-empty",
                            "Tu vehiculo esta registrado como MOTOCARGA, y los viajes de pasajeros son para motocarro. Las motocargas te llegan en \"Mandados cercanos\". Si tu vehiculo es un motocarro, cambialo en \"Mi vehiculo\"."
                        }
                    } else if session.ride_availability() == Some(true) {
                        p { class: "nearby-rides-empty",
                            "No hay solicitudes esperando por ahora. La lista se actualiza sola."
                        }
                    } else {
                        TurnOnRidesPrompt {}
                    }
                } else {
                    ul { class: "nearby-rides-list",
                        for (request, km) in sorted_requests.clone() {
                            NearbyRideRow {
                                key: "{request.ride_id}",
                                request: request.clone(),
                                distance_km: km,
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
                                for (request, km) in sorted_requests.clone() {
                                    NearbyRideRow {
                                        key: "{request.ride_id}",
                                        request: request.clone(),
                                        distance_km: km,
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
    /// Distancia desde el conductor (si se conoce su GPS).
    #[props(default)]
    distance_km: Option<f64>,
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
    // Viaje solo recogida (issue #92): el conductor registra el precio que
    // acordo con el pasajero antes de aceptar, mismo criterio que
    // `NearbyErrandRow` (`nearby_errands.rs`).
    let mut agreed_price = use_signal(String::new);

    let ride_id = props.request.ride_id;
    let is_pickup_only = props.request.is_pickup_only();
    let on_accepted = props.on_accepted;
    let on_unavailable = props.on_unavailable;

    let on_accept_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let price = if is_pickup_only {
            match agreed_price().trim().parse::<i64>() {
                Ok(price) if price >= 1 => Some(price),
                _ => return,
            }
        } else {
            None
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_accepting.set(true);
            error.set(None);

            match api_client.accept_ride(&token, ride_id, price).await {
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

    let price_is_valid = !is_pickup_only
        || agreed_price()
            .trim()
            .parse::<i64>()
            .is_ok_and(|price| price >= 1);

    rsx! {
        li {
            class: if is_pickup_only { "nearby-ride-request nearby-ride-request-pickup-only" } else { "nearby-ride-request" },
            if is_pickup_only {
                p { class: "nearby-ride-mode-badge", "Solo recogida · precio a acordar" }
            }
            PickupLocation { lat: props.request.origin.latitude, lng: props.request.origin.longitude }
            p { "Destino: {props.request.destination_label()}" }
            p { "Pasajeros: {props.request.passenger_count}" }
            if let Some(km) = props.distance_km {
                p { class: "nearby-ride-distance",
                    "📍 A {distance_label(km)} de ti · unos {eta_minutes(km)} min"
                }
            }
            if is_pickup_only {
                label { r#for: "ride-{ride_id}-price", "Precio a acordar ({props.request.currency})" }
                input {
                    id: "ride-{ride_id}-price",
                    r#type: "number",
                    min: "1",
                    disabled: is_accepting(),
                    value: "{agreed_price}",
                    oninput: move |event| agreed_price.set(event.value()),
                }
            } else {
                p { "Tarifa estimada: {props.request.fare_label()}" }
            }
            button {
                r#type: "button",
                class: "nearby-ride-accept-button",
                disabled: is_accepting() || !price_is_valid,
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

#[derive(Props, Clone, PartialEq)]
struct CompleteRideButtonProps {
    ride_id: u64,
    on_completed: EventHandler<Ride>,
}

/// Boton "Completar viaje" del viaje en curso (issue #23). Componente
/// aparte, igual que `StartRideButton`, para que su estado de carga/error no
/// dependa del resto de la pantalla. Al tener exito, la pantalla reemplaza
/// el viaje en pantalla por la version devuelta por el backend (ahora
/// `completed`, con `completed_at`, `final_fare` y `payment` ya resueltos —
/// el cobro se dispara en la misma operacion, ver comentario de modulo).
#[component]
fn CompleteRideButton(props: CompleteRideButtonProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_completing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let ride_id = props.ride_id;
    let on_completed = props.on_completed;

    let on_complete_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_completing.set(true);
            error.set(None);

            match api_client.complete_ride(&token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_completed.call(fetch.data);
                }
                Err(CompleteRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }

            is_completing.set(false);
        });
    };

    rsx! {
        div { class: "nearby-ride-complete",
            button {
                r#type: "button",
                class: "nearby-ride-complete-button",
                disabled: is_completing(),
                onclick: on_complete_click,
                if is_completing() {
                    "Completando..."
                } else {
                    "Completar viaje"
                }
            }
            if let Some(message) = error() {
                p { class: "nearby-ride-complete-error", role: "alert", "{message}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CancelAcceptedRideButtonProps {
    ride_id: u64,
    on_cancelled: EventHandler<()>,
}

/// Boton "Cancelar viaje" del viaje aceptado, antes de iniciarlo (issue
/// #22). Llama a `ApiClient::cancel_ride`, el mismo endpoint que usa el
/// pasajero (issues #15/#21) — el backend decide segun quien llama, ver el
/// comentario de modulo mas arriba. Al tener exito no hay nada que mostrar
/// (el viaje ya no es "suyo"), asi que solo avisa al padre para que vuelva a
/// la lista; el estado de carga/error queda aparte, igual que
/// `StartRideButton`, para no afectar al resto de la pantalla mientras la
/// llamada esta en curso.
#[component]
fn CancelAcceptedRideButton(props: CancelAcceptedRideButtonProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_cancelling = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let ride_id = props.ride_id;
    let on_cancelled = props.on_cancelled;

    let on_cancel_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_cancelling.set(true);
            error.set(None);

            match api_client.cancel_ride(&token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_cancelled.call(());
                }
                Err(CancelRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }

            is_cancelling.set(false);
        });
    };

    rsx! {
        div { class: "nearby-ride-cancel",
            button {
                r#type: "button",
                class: "nearby-ride-cancel-button",
                disabled: is_cancelling(),
                onclick: on_cancel_click,
                if is_cancelling() {
                    "Cancelando..."
                } else {
                    "Cancelar viaje"
                }
            }
            if let Some(message) = error() {
                p { class: "nearby-ride-cancel-error", role: "alert", "{message}" }
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

/// Aviso con boton cuando "Recibir viajes" esta apagado: el conductor lo
/// enciende aca mismo, sin tener que ir a "Mi perfil" (reportado: el
/// companero no veia viajes y no sabia por que).
#[component]
fn TurnOnRidesPrompt() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut is_saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let turn_on = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_saving.set(true);
            error.set(None);
            match api_client
                .update_ride_availability(&token, true, None)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_ride_availability(fetch.data.is_available);
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => error.set(Some(err.to_string())),
            }
            is_saving.set(false);
        });
    };

    rsx! {
        div { class: "turn-on-rides",
            p { class: "nearby-rides-empty",
                "Tienes \"Recibir viajes\" APAGADO, por eso no te llegan solicitudes."
            }
            button {
                r#type: "button",
                class: "turn-on-rides-button",
                disabled: is_saving(),
                onclick: turn_on,
                if is_saving() { "Encendiendo..." } else { "🛺 Encender \"Recibir viajes\"" }
            }
            if let Some(message) = error() {
                p { class: "price-offer-error", role: "alert", "{message}" }
            }
        }
    }
}

/// Tarjeta grande con el estado del viaje para el conductor.
fn driver_ride_banner(ride: &Ride) -> (Tone, &'static str, &'static str, Option<String>) {
    match ride.status {
        RideStatus::Accepted if ride.awaiting_price_response() => (
            Tone::Wait,
            "⏳",
            "Esperando que el pasajero acepte tu precio",
            ride.agreed_price
                .map(|price| format!("Ofreciste {}", format_cop(price))),
        ),
        RideStatus::Accepted | RideStatus::Requested => (
            Tone::Go,
            "🛺",
            "¡Ve a recoger al pasajero!",
            Some("Cuando lo recojas, toca \"Iniciar viaje\".".to_string()),
        ),
        RideStatus::InProgress => (
            Tone::Go,
            "🚀",
            "Viaje en curso",
            Some("Al llegar al destino, toca \"Completar viaje\".".to_string()),
        ),
        RideStatus::Completed => (
            Tone::Done,
            "🎉",
            "¡Viaje terminado!",
            ride.final_fare
                .or(ride.fare())
                .map(|amount| format!("Cobrado: {}", format_cop(amount))),
        ),
        RideStatus::Cancelled => (Tone::Bad, "❌", "El viaje fue cancelado", None),
    }
}

/// Ordena las solicitudes por distancia al conductor y le pega la
/// distancia a cada una; sin posicion, deja el orden del servidor.
fn sort_by_distance(
    requests: Vec<NearbyRideRequest>,
    position: Option<Coordinates>,
) -> Vec<(NearbyRideRequest, Option<f64>)> {
    let mut with_distance: Vec<(NearbyRideRequest, Option<f64>)> = requests
        .into_iter()
        .map(|request| {
            let km = position.map(|point| distance_km(point, request.origin));
            (request, km)
        })
        .collect();
    if position.is_some() {
        with_distance.sort_by(|a, b| a.1.unwrap_or(f64::MAX).total_cmp(&b.1.unwrap_or(f64::MAX)));
    }
    with_distance
}

#[cfg(test)]
mod tests {
    use super::*;
    use moto_core::models::RideMode;

    fn request(id: u64, latitude: f64) -> NearbyRideRequest {
        NearbyRideRequest {
            ride_id: id,
            mode: RideMode::default(),
            origin: Coordinates {
                latitude,
                longitude: -67.92,
            },
            destination: None,
            destination_text: None,
            passenger_count: 1,
            currency: "COP".to_string(),
            estimated_fare: None,
        }
    }

    #[test]
    fn the_nearest_ride_goes_first() {
        let me = Coordinates {
            latitude: 3.86,
            longitude: -67.92,
        };
        let sorted = sort_by_distance(vec![request(1, 3.90), request(2, 3.861)], Some(me));
        assert_eq!(sorted[0].0.ride_id, 2);
        assert!(sorted[0].1.unwrap() < sorted[1].1.unwrap());
    }

    #[test]
    fn without_gps_keeps_the_server_order() {
        let sorted = sort_by_distance(vec![request(1, 3.90), request(2, 3.861)], None);
        assert_eq!(sorted[0].0.ride_id, 1);
        assert_eq!(sorted[0].1, None);
    }
}
