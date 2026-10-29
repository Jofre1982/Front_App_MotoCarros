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
//! Desde la historia #87 del backend (issue #74 de este repo), el destino ya
//! no es un punto libre en el mapa: es un sitio del catalogo con precio fijo
//! (`GET /sites`, `ApiClient::list_sites`), elegido de una lista con su
//! precio de referencia visible. El origen sigue siendo un punto libre
//! (`MapView::on_click`, ver `moto_ui/src/map.rs`): el conductor tiene que
//! encontrar al pasajero donde este, eso no cambio. Se agrega tambien un
//! selector de cantidad de pasajeros (1 a 3, la capacidad de un motocarro).
//! El precio de referencia que se muestra junto a cada sitio es `day_price`:
//! esta pantalla no replica la regla de recargo nocturno del backend (hora
//! exacta, huso horario del servidor), el monto exacto siempre sale de
//! `POST /rides/estimate`.
//!
//! Mientras el viaje solicitado sigue en `requested` o `accepted`, el
//! pasajero puede desistir con `POST /api/v1/rides/{ride}/cancel`
//! (`ApiClient::cancel_ride`). En `requested` es la historia #15 ("nadie lo
//! acepto todavia"); en `accepted` es la historia "Cancelar un viaje
//! aceptado (pasajero)" (#21) — mismo endpoint, el backend decide si aplica
//! una penalizacion segun `openapi.yaml` (`cancellation_fee_applies`: `false`
//! sin conductor asignado todavia, `true` si ya lo habia). Un viaje
//! `in_progress` no es cancelable por este endpoint segun ese mismo contrato
//! (solo se puede completar, historia #23), asi que el boton no se ofrece en
//! ese estado pese a que el criterio de aceptacion original de #21 lo
//! mencionaba — ver la justificacion en la descripcion del PR. Tras un
//! cancelacion exitosa se muestra explicitamente si hubo penalizacion antes
//! de volver la pantalla al estado inicial para poder solicitar otro viaje.
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
//! al montarse (ver `moto_ui/src/map.rs`). Desde la historia #87 del backend
//! (issue #74) el mapa de seguimiento ya no marca el destino: un sitio no
//! tiene coordenadas, solo nombre — el nombre se muestra aparte como texto.
//!
//! Desde la historia #95 del backend (issue #92 de este repo) el pasajero
//! elige ademas la modalidad: "Viaje con destino y precio fijo" (el flujo de
//! arriba, sin cambios) o "Solo recogida", donde marca solo el punto de
//! recogida y escribe a donde va (`ApiClient::request_pickup_only_ride`).
//! Esa modalidad no tiene estimado: el boton pide el viaje directo, y el
//! seguimiento muestra "A acordar con el conductor" hasta que lo aceptan y
//! llega `agreed_price` (en `GET /rides/{id}` y en `status.changed`).
//!
//! Una vez `completed`, el pasajero ve el resultado del cobro leyendo
//! `Ride::payment` (historia #24) -- el cobro mismo ya ocurrio de forma
//! sincronica dentro de `POST /api/v1/rides/{ride}/complete` (issue #23), asi
//! que esta pantalla nunca llama a ningun endpoint de "pagar" (no existe uno
//! en el backend). Un cobro `failed` no ofrece boton de reintentar porque el
//! backend tampoco expone esa accion.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    ApiClient, AuthenticatedRequestError, CancelRideError, GetRideError, RequestRideError,
    ServiceActionError,
};
use moto_core::models::{
    Coordinates, PaymentStatus, Ride, RideCancellation, RideStatus, RideTracking, Role,
    VehicleType, apply_ride_tracking_event, latest_active_ride,
};
use moto_core::realtime::{
    ConnectionState, PollAction, RealtimeClient, RealtimeConfig, SubscribeFailureAction,
    decide_poll_action, decide_subscribe_failure,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::{MapMarker, MapView};
use crate::price_offer::{ContactCard, PriceOfferCard};
use crate::screens::request_errand::RequestErrandScreen;

/// Intervalo entre sondeos de `RealtimeClient::poll_events` para el canal
/// `ride.{id}` (issue #20) — mismo valor y mismo motivo que el
/// `POLL_INTERVAL` de `NearbyRidesList` (`nearby_rides.rs`): el transporte
/// no avisa solo cuando llega un frame nuevo.
const RIDE_TRACKING_POLL_INTERVAL: Duration = Duration::from_millis(700);

/// Cada cuanto se vuelve a pedir el viaje por HTTP, por si el tiempo real
/// no llega.
const RIDE_REFRESH_INTERVAL: Duration = Duration::from_secs(8);

/// Centro inicial del mapa mientras no exista geolocalizacion del
/// dispositivo (issue #90): el casco urbano de Inirida, donde opera el
/// servicio. Antes era Bogota (el punto de los ejemplos de `openapi.yaml`),
/// y el pasajero tenia que desplazar el mapa hasta el pais entero para
/// encontrar su ubicacion.
const DEFAULT_CENTER_LAT: f64 = 3.8653;
const DEFAULT_CENTER_LNG: f64 = -67.9239;

#[component]
pub fn RideEstimateScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut service = use_signal(|| Service::Ride);
    let mut origin = use_signal(|| None::<(f64, f64)>);
    let mut destination_text = use_signal(String::new);
    let mut passenger_count = use_signal(|| 1u8);
    let mut ride_error = use_signal(|| None::<RequestRideError>);
    let mut requested_ride = use_signal(|| None::<Ride>);
    let mut is_requesting = use_signal(|| false);
    let mut cancel_error = use_signal(|| None::<CancelRideError>);
    let mut is_cancelling = use_signal(|| false);
    let mut cancellation_result = use_signal(|| None::<RideCancellation>);
    // `true` mientras se busca si el pasajero ya tenia un viaje activo.
    let mut is_restoring = use_signal(|| true);
    let mut restore_started = use_signal(|| false);

    // Al abrir la pantalla se busca el viaje activo en el historial: el
    // servidor no deja pedir un segundo viaje mientras haya uno activo, y si
    // la app se cerro (o se cambio de pantalla) el pasajero quedaba trabado
    // sin poder verlo ni cancelarlo (reportado probando el APK).
    {
        let api_client = api_client.clone();
        let storage = storage.clone();
        use_effect(move || {
            if restore_started() {
                return;
            }
            let Some(token) = session.token() else {
                is_restoring.set(false);
                return;
            };
            restore_started.set(true);
            let api_client = api_client.clone();
            let storage = storage.clone();
            spawn(async move {
                match api_client.ride_history(&token, Some(Role::Passenger)).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        if let Some(active) = latest_active_ride(&fetch.data) {
                            requested_ride.set(Some(active));
                        }
                    }
                    Err(AuthenticatedRequestError::SessionExpired) => {
                        session.logout(storage.as_ref());
                    }
                    Err(_) => {}
                }
                is_restoring.set(false);
            });
        });
    }

    let on_map_click = move |(lat, lng): (f64, f64)| {
        origin.set(Some((lat, lng)));
    };

    // Siempre "solo recogida": sin precio fijo ni estimado (pedido del dueno,
    // los precios varian segun a donde se va y se acuerdan con el
    // conductor). A donde va es opcional. Un error (p.ej. un 422) se muestra
    // sin borrar lo escrito.
    let on_pickup_only_request_click = {
        let api_client = api_client.clone();
        let storage = storage.clone();
        move |_| {
            let Some(token) = session.token() else {
                return;
            };
            let Some((origin_lat, origin_lng)) = origin() else {
                return;
            };
            let text = destination_text();
            let count = passenger_count();
            let api_client = api_client.clone();
            let storage = storage.clone();

            spawn(async move {
                is_requesting.set(true);
                ride_error.set(None);

                let origin_coords = Coordinates {
                    latitude: origin_lat,
                    longitude: origin_lng,
                };

                match api_client
                    .request_pickup_only_ride(&token, origin_coords, &text, count)
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

    // Respuesta a la oferta de precio del conductor (backend #107).
    let mut is_responding_price = use_signal(|| false);
    let mut price_error = use_signal(|| None::<String>);
    let mut rejected_notice = use_signal(|| false);
    let respond_api_client = api_client.clone();
    let respond_storage = storage.clone();
    let respond_to_price = use_callback(move |accept: bool| {
        let Some(token) = session.token() else {
            return;
        };
        let Some(ride_id) = requested_ride.peek().as_ref().map(|ride| ride.id) else {
            return;
        };
        let api_client = respond_api_client.clone();
        let storage = respond_storage.clone();
        spawn(async move {
            is_responding_price.set(true);
            price_error.set(None);
            match api_client
                .respond_to_ride_price(&token, ride_id, accept)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    rejected_notice.set(!accept);
                    requested_ride.set(Some(fetch.data));
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => price_error.set(Some(err.to_string())),
            }
            is_responding_price.set(false);
        });
    });

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
                    cancellation_result.set(Some(fetch.data));
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

    // Vuelve la pantalla al estado inicial una vez que el pasajero ya vio si
    // se le aplico una penalizacion (issue #21) — mismo reseteo que antes
    // hacia `on_cancel_click` directamente al confirmar la cancelacion.
    let on_cancellation_dismiss = move |_| {
        cancellation_result.set(None);
        requested_ride.set(None);
        ride_error.set(None);
        cancel_error.set(None);
        origin.set(None);
        destination_text.set(String::new());
        passenger_count.set(1);
    };

    // Solo el origen tiene marcador propio: desde la historia #87 del
    // backend (issue #74) el destino es un sitio sin coordenadas, no un
    // punto en el mapa.
    let markers: Vec<MapMarker> = origin()
        .map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Origen".to_string()),
        })
        .into_iter()
        .collect();

    let can_request = origin().is_some();
    let is_errand = service() == Service::Errand;

    if is_restoring() && requested_ride().is_none() {
        return rsx! {
            div { class: "ride-estimate-screen",
                h2 { "Pedir motocarro" }
                p { class: "upload-status", role: "status",
                    span { class: "upload-spinner" }
                    "Cargando..."
                }
            }
        };
    }

    if let Some(cancellation) = cancellation_result() {
        // Confirmacion explicita de la penalizacion (issue #21) antes de
        // volver al estado inicial — el pasajero decide cuando descartarla.
        return rsx! {
            div { class: "ride-estimate-screen",
                h2 { "Viaje cancelado" }
                p { class: "ride-cancellation-status",
                    "{cancellation_fee_message(cancellation.cancellation_fee_applies)}"
                }
                button {
                    r#type: "button",
                    class: "ride-cancellation-dismiss-button",
                    onclick: on_cancellation_dismiss,
                    "Solicitar otro viaje"
                }
            }
        };
    }

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
        // El pasajero puede desistir mientras el viaje sigue `requested`
        // (issue #15) o ya `accepted` (issue #21) — en `in_progress` el
        // endpoint ya no lo permite segun `openapi.yaml`, ver el comentario
        // de modulo mas arriba.
        let is_cancellable = matches!(ride.status, RideStatus::Requested | RideStatus::Accepted);
        let cancel_button_label = match ride.status {
            RideStatus::Requested => "Cancelar solicitud",
            _ => "Cancelar viaje",
        };

        return rsx! {
            div { class: "ride-estimate-screen",
                h2 { "Tu motocarro" }
                p { class: "ride-request-status", "{ride_status_label(ride.status)}" }
                if ride.awaiting_price_response() {
                    if let (Some(driver), Some(price)) = (&ride.driver, ride.agreed_price) {
                        PriceOfferCard {
                            driver_name: driver.name.clone(),
                            driver_avatar_url: driver.avatar_url.clone(),
                            vehicle: driver.vehicle.clone(),
                            price,
                            is_responding: is_responding_price(),
                            error: price_error(),
                            on_accept: move |_| respond_to_price.call(true),
                            on_reject: move |_| respond_to_price.call(false),
                        }
                    }
                }
                if let Some(counterpart) = ride.counterpart.clone() {
                    ContactCard {
                        title: "Tu conductor",
                        counterpart,
                        vehicle: ride.driver.as_ref().and_then(|d| d.vehicle.clone()),
                    }
                }
                if rejected_notice() && ride.status == RideStatus::Requested {
                    p { class: "ride-request-status",
                        "Rechazaste el precio. Tu viaje quedo libre para otro conductor."
                    }
                }
                dl { class: "ride-request-result",
                    dt { "Destino" }
                    dd { "{ride.destination_label()}" }
                    dt { "Personas" }
                    dd { "{ride.passenger_count}" }
                    dt { if ride.is_pickup_only() && ride.agreed_price.is_some() && ride.price_confirmed { "Precio acordado" } else { "Precio" } }
                    dd { class: "ride-request-agreed-price", "{ride.fare_label()}" }
                }
                if is_cancellable {
                    button {
                        r#type: "button",
                        class: "ride-cancel-button",
                        disabled: is_cancelling(),
                        onclick: on_cancel_click,
                        if is_cancelling() {
                            "Cancelando..."
                        } else {
                            "{cancel_button_label}"
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
                } else if ride.status == RideStatus::Completed {
                    // El cobro ya ocurrio de forma sincronica dentro de
                    // `POST /api/v1/rides/{ride}/complete` (issue #23, lo
                    // dispara el conductor) -- esta pantalla no ejecuta
                    // ningun pago, solo lee el resultado que ya viene en el
                    // `Ride` (historia #24).
                    if let Some(payment) = ride.payment {
                        p {
                            class: match payment.status {
                                PaymentStatus::Paid => "payment-result-panel payment-result-paid",
                                PaymentStatus::Failed => "payment-result-panel payment-result-failed",
                                PaymentStatus::Pending => "payment-result-panel payment-result-pending",
                            },
                            role: if payment.status == PaymentStatus::Failed { "alert" } else { "status" },
                            "{payment_result_message(payment.status)}"
                        }
                    }
                }
                // Al terminar (o si lo cancelaron del otro lado) la pantalla
                // no se quedaba sin salida: se vuelve al formulario.
                if !is_active {
                    button {
                        r#type: "button",
                        class: "ride-request-again-button",
                        onclick: move |_| {
                            requested_ride.set(None);
                            ride_error.set(None);
                            cancel_error.set(None);
                            rejected_notice.set(false);
                            origin.set(None);
                            destination_text.set(String::new());
                            passenger_count.set(1);
                        },
                        "Pedir otro viaje"
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "ride-estimate-screen",
            h2 { "Pedir motocarro" }
            fieldset { class: "ride-mode-selector",
                legend { "¿Que necesitas?" }
                label {
                    class: if is_errand { "ride-mode-option" } else { "ride-mode-option ride-mode-option-selected" },
                    input {
                        r#type: "radio",
                        name: "ride-service",
                        checked: !is_errand,
                        onchange: move |_| service.set(Service::Ride),
                    }
                    span { class: "ride-mode-option-text",
                        strong { "🛺 Viaje" }
                        small { "Te recogemos y te llevamos." }
                    }
                }
                label {
                    class: if is_errand { "ride-mode-option ride-mode-option-selected" } else { "ride-mode-option" },
                    input {
                        r#type: "radio",
                        name: "ride-service",
                        checked: is_errand,
                        onchange: move |_| service.set(Service::Errand),
                    }
                    span { class: "ride-mode-option-text",
                        strong { "🛍️ Mandado" }
                        small { "Te compramos o llevamos algo." }
                    }
                }
            }
            if is_errand {
                RequestErrandScreen { vehicle_type: VehicleType::Motocarro, embedded: true }
            } else {
                p { class: "ride-step-title", "1. ¿Donde te recogemos?" }
                p { class: "ride-estimate-instructions",
                    if origin().is_some() {
                        "Listo, marcamos tu punto de recogida. Puedes tocar otro lugar para cambiarlo."
                    } else {
                        "Toca el mapa en el lugar donde estas."
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
                label { r#for: "ride-pickup-only-destination", class: "ride-step-title",
                    "2. ¿A donde vas? "
                    span { class: "optional-tag", "(opcional)" }
                }
                input {
                    id: "ride-pickup-only-destination",
                    r#type: "text",
                    maxlength: "255",
                    placeholder: "Ej.: al hospital, al puerto",
                    value: "{destination_text}",
                    oninput: move |event| destination_text.set(event.value()),
                }
                label { r#for: "ride-estimate-passenger-count", class: "ride-step-title", "3. ¿Cuantas personas van?" }
                select {
                    id: "ride-estimate-passenger-count",
                    value: "{passenger_count}",
                    onchange: move |event| {
                        if let Ok(count) = event.value().parse::<u8>() {
                            passenger_count.set(count);
                        }
                    },
                    option { value: "1", "1 persona" }
                    option { value: "2", "2 personas" }
                    option { value: "3", "3 personas" }
                }
                p { class: "ride-pickup-only-price-note",
                    "Precio: lo acuerdas con el conductor cuando acepte tu viaje."
                }
                button {
                    r#type: "button",
                    class: "ride-request-button",
                    disabled: !can_request || is_requesting(),
                    onclick: on_pickup_only_request_click,
                    if is_requesting() {
                        "Pidiendo..."
                    } else {
                        "Pedir motocarro"
                    }
                }
                if let Some(err) = ride_error() {
                    p { class: "ride-request-error", role: "alert", "{err}" }
                }
                if !can_request {
                    p { class: "ride-estimate-empty", "Toca el mapa para marcar donde te recogemos." }
                }
            }
        }
    }
}

/// Que pide el pasajero desde "Pedir motocarro": un viaje para el, o un
/// mandado (mismo formulario que "Pedir motocarga", con motocarro).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Service {
    Ride,
    Errand,
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
    let mut refresh_started = use_signal(|| false);

    // Ademas del tiempo real, se vuelve a pedir el viaje cada
    // `RIDE_REFRESH_INTERVAL`: si el socket no conecta (o el celular perdio
    // la senal un rato), el pasajero igual se entera de que lo aceptaron o
    // de que el viaje termino.
    {
        let api_client = api_client.clone();
        let storage = storage.clone();
        use_effect(move || {
            if refresh_started() {
                return;
            }
            refresh_started.set(true);
            let api_client = api_client.clone();
            let storage = storage.clone();
            spawn(async move {
                loop {
                    Delay::new(RIDE_REFRESH_INTERVAL).await;
                    let Some(token) = session.token() else {
                        break;
                    };
                    match api_client.get_ride(&token, ride_id).await {
                        Ok(fetch) => {
                            if let Some(refreshed) = fetch.refreshed_token {
                                session.update_token(refreshed, storage.as_ref());
                            }
                            let still_active = fetch.data.status.is_active();
                            if tracking.peek().ride != fetch.data {
                                tracking.with_mut(|t| t.ride = fetch.data.clone());
                                on_ride_updated.call(fetch.data);
                            }
                            if !still_active {
                                break;
                            }
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
    // Solo el origen y (si ya se conoce) el conductor tienen marcador: desde
    // la historia #87 del backend (issue #74) el destino es un sitio sin
    // coordenadas, asi que su nombre se muestra aparte como texto en vez de
    // un marcador en el mapa.
    let mut markers = vec![MapMarker {
        lat: current.ride.origin.latitude,
        lng: current.ride.origin.longitude,
        label: Some("Origen".to_string()),
    }];
    let destination_name = current.ride.destination_label();
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
            p { class: "ride-tracking-destination", "Destino: {destination_name}" }
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

/// Mensaje explicito de penalizacion tras cancelar (issue #21).
/// `cancellation_fee_applies` solo falta cuando quien cancela es el
/// conductor asignado (ver `RideCancellation` en `moto_core::models`), caso
/// que no ocurre en este flujo de pasajero — se trata igual que `Some(false)`
/// por si acaso.
fn cancellation_fee_message(fee_applies: Option<bool>) -> &'static str {
    match fee_applies {
        Some(true) => {
            "Se aplicara una penalizacion porque el conductor ya se habia desplazado hacia el punto de recogida."
        }
        Some(false) | None => "No se aplico ninguna penalizacion.",
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

/// Texto para el pasajero segun el resultado del cobro de un viaje recien
/// completado (`Ride::payment`, historia #24). El backend no expone ningun
/// endpoint para reintentar un cobro fallido (`ChargeRideAction` es
/// idempotente puertas adentro, pero no hay ninguna ruta que la vuelva a
/// disparar -- ver la discusion en el issue), asi que un pago `failed` nunca
/// sugiere reintentar, solo contactar soporte. La historia "Ver recibo"
/// (#25) todavia no existe como pantalla propia, asi que un pago `paid`
/// remite al historial de viajes (issue #28, ya disponible) en vez de
/// ofrecer una navegacion que hoy no tiene destino.
fn payment_result_message(status: PaymentStatus) -> &'static str {
    match status {
        PaymentStatus::Paid => {
            "El cobro de tu viaje se proceso correctamente. Podras ver el detalle en la seccion \"Historial de viajes\"."
        }
        PaymentStatus::Failed => {
            "No pudimos procesar el cobro de tu viaje. Si el problema persiste, contacta a soporte."
        }
        PaymentStatus::Pending => "Estamos procesando el cobro de tu viaje.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_fee_message_warns_about_the_penalty_when_it_applies() {
        assert_eq!(
            cancellation_fee_message(Some(true)),
            "Se aplicara una penalizacion porque el conductor ya se habia desplazado hacia el punto de recogida."
        );
    }

    #[test]
    fn cancellation_fee_message_reassures_when_no_penalty_applies() {
        assert_eq!(
            cancellation_fee_message(Some(false)),
            "No se aplico ninguna penalizacion."
        );
    }

    #[test]
    fn cancellation_fee_message_treats_a_missing_flag_as_no_penalty() {
        assert_eq!(
            cancellation_fee_message(None),
            "No se aplico ninguna penalizacion."
        );
    }

    #[test]
    fn payment_result_message_confirms_a_successful_charge() {
        let message = payment_result_message(PaymentStatus::Paid);
        assert!(message.contains("proceso correctamente"));
        assert!(message.contains("Historial de viajes"));
    }

    #[test]
    fn payment_result_message_does_not_suggest_retrying_a_failed_charge() {
        let message = payment_result_message(PaymentStatus::Failed);
        assert!(message.to_lowercase().contains("soporte"));
        assert!(!message.to_lowercase().contains("reintent"));
        assert!(!message.to_lowercase().contains("pagar"));
    }

    #[test]
    fn payment_result_message_reports_a_pending_charge_as_in_progress() {
        assert!(payment_result_message(PaymentStatus::Pending).contains("procesando"));
    }
}
