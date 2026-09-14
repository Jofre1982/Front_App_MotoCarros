//! Pantalla de mandados disponibles cercanos (conductor) — issue #78.
//!
//! Depende de la disponibilidad para mandados (issue #77,
//! `SessionState::errand_availability()`, en `ProfileScreen`): el backend no
//! expone ningun canal de tiempo real ni aviso push para mandados (a
//! diferencia de `NearbyRidesScreen` con viajes), asi que esta pantalla
//! sondea `GET /api/v1/errands` (`ApiClient::list_errands`) a un ritmo fijo
//! en vez de suscribirse a un canal de Reverb.
//!
//! El backend tampoco distingue "el conductor no esta disponible para
//! mandados" de "esta disponible pero no hay ninguno ahora" — ambos casos
//! devuelven una lista vacia (`ListErrandsController`). Por eso el estado
//! vacio que muestra esta pantalla depende de
//! `SessionState::errand_availability()` en vez de inferirse de la
//! respuesta: si el conductor todavia no toco el control de disponibilidad
//! en esta sesion (`None`), la pantalla se lo dice explicitamente en vez de
//! asumir cualquiera de los otros dos casos.
//!
//! Cada mandado con foto adjunta (`has_photo`) ofrece un boton "Ver foto"
//! que trae los bytes bajo demanda con `ApiClient::errand_photo` y los
//! muestra como un data URI (`base64`): el endpoint exige el mismo Bearer
//! token que el resto de la API, asi que un `<img src>` apuntando
//! directamente a `photo_url` no funcionaria (el navegador no manda esa
//! cabecera en una carga de imagen). Se pide una sola vez por mandado y
//! queda cacheada mientras la fila siga montada, para no volver a pedirla en
//! cada vuelta del sondeo.
//!
//! Cada fila tambien ofrece "Aceptar" con el precio acordado con el pasajero
//! (issue #79, `ApiClient::accept_errand`). Al aceptar con exito, el mandado
//! se saca de la lista de disponibles y pasa a "Mandados que aceptaste en
//! esta sesion" — el backend no tiene ningun `GET` para recuperar los
//! mandados aceptados por este conductor (`GET /errands` solo devuelve los
//! `requested`), asi que esa lista es pura memoria de esta sesion, mismo
//! criterio que `SessionState::errand_availability()` (issue #77): si el
//! conductor recarga la app, la pierde. Si otro conductor lo acepto primero
//! (409, carrera documentada en `openapi.yaml`), la fila se saca igual pero
//! con un aviso explicito en vez de un error generico, en vez de dejarla
//! como si siguiera disponible.
//!
//! Cada mandado de "Mandados que aceptaste en esta sesion" ofrece "Completar"
//! (issue #80, `ApiClient::complete_errand`) mientras siga `accepted`: al
//! completarlo con exito se reemplaza en el lugar por la version que
//! devuelve el backend (ahora `completed`), y el boton deja de ofrecerse —
//! no hay ningun cobro ni recibo que mostrar, a diferencia de completar un
//! viaje: el precio ya quedo fijado en `agreed_price` al aceptar (issue #79)
//! y no hay pasarela de pago para mandados.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    AcceptErrandError, ApiClient, AuthenticatedRequestError, CompleteErrandError,
};
use moto_core::models::{Errand, ErrandStatus};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

/// Intervalo entre sondeos de `GET /api/v1/errands` — mismo valor y mismo
/// motivo que el `POLL_INTERVAL` de `NearbyRidesList` (`nearby_rides.rs`):
/// el transporte no avisa solo cuando hay un mandado nuevo.
const POLL_INTERVAL: Duration = Duration::from_millis(700);

#[component]
pub fn NearbyErrandsScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut errands = use_signal(Vec::<Errand>::new);
    let mut load_error = use_signal(|| None::<String>);
    // Mandados que este conductor acepto en esta sesion (issue #79) — ver el
    // comentario de modulo: el backend no tiene forma de recuperarlos, asi
    // que esta lista es la unica fuente de verdad que tiene la app.
    let mut accepted_errands = use_signal(Vec::<Errand>::new);
    // Aviso transitorio para cuando otro conductor acepta un mandado primero
    // (409): la fila desaparece de `errands` igual que en `NearbyRideRow`,
    // pero a diferencia de esa pantalla el criterio de aceptacion de esta
    // historia pide un mensaje explicito, no solo que la fila se esfume.
    let mut unavailable_notice = use_signal(|| None::<String>);
    // Mismo criterio que los loops de sondeo de `NearbyRidesList`: evita
    // levantar un segundo loop si el efecto se vuelve a disparar. Dioxus
    // cancela la tarea al desmontar el componente (`Home`, al cambiar de
    // pestana).
    let mut started = use_signal(|| false);

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);

        let Some(token) = session.token() else {
            load_error.set(Some(
                "La sesion expiro. Inicia sesion de nuevo.".to_string(),
            ));
            return;
        };

        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            let mut current_token = token;

            loop {
                match api_client.list_errands(&current_token).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed.clone(), storage.as_ref());
                            current_token = refreshed;
                        }
                        load_error.set(None);
                        errands.set(fetch.data);
                    }
                    Err(AuthenticatedRequestError::SessionExpired) => {
                        session.logout(storage.as_ref());
                        break;
                    }
                    Err(err) => {
                        load_error.set(Some(err.to_string()));
                    }
                }

                Delay::new(POLL_INTERVAL).await;
            }
        });
    });

    let availability = session.errand_availability();

    rsx! {
        div { class: "nearby-errands-screen",
            h2 { "Mandados cercanos" }
            if let Some(message) = unavailable_notice() {
                p { class: "nearby-errands-unavailable-notice", role: "alert",
                    "{message}"
                    button {
                        r#type: "button",
                        onclick: move |_| unavailable_notice.set(None),
                        "Cerrar"
                    }
                }
            }
            if let Some(message) = load_error() {
                p { class: "nearby-errands-error", role: "alert", "{message}" }
            } else if errands().is_empty() {
                match availability {
                    None => rsx! {
                        p { class: "nearby-errands-empty",
                            "Todavia no sabemos si estas disponible para mandados en esta sesion. Revisa tu perfil para activarlo."
                        }
                    },
                    Some(false) => rsx! {
                        p { class: "nearby-errands-empty",
                            "No estas disponible para mandados. Activalo en tu perfil para empezar a verlos."
                        }
                    },
                    Some(true) => rsx! {
                        p { class: "nearby-errands-empty", "No hay mandados disponibles por el momento." }
                    },
                }
            } else {
                ul { class: "nearby-errands-list",
                    for errand in errands() {
                        NearbyErrandRow {
                            key: "{errand.id}",
                            errand: errand.clone(),
                            on_accepted: move |accepted: Errand| {
                                let accepted_id = accepted.id;
                                errands.with_mut(|list| list.retain(|e| e.id != accepted_id));
                                accepted_errands.with_mut(|list| list.push(accepted));
                            },
                            on_unavailable: move |errand_id: u64| {
                                errands.with_mut(|list| list.retain(|e| e.id != errand_id));
                                unavailable_notice
                                    .set(Some("Este mandado ya no esta disponible: otro conductor lo acepto primero.".to_string()));
                            },
                        }
                    }
                }
            }
            if !accepted_errands().is_empty() {
                div { class: "accepted-errands-section",
                    h3 { "Mandados que aceptaste en esta sesion" }
                    ul { class: "accepted-errands-list",
                        for errand in accepted_errands() {
                            AcceptedErrandRow {
                                key: "{errand.id}",
                                errand: errand.clone(),
                                on_completed: move |completed: Errand| {
                                    let completed_id = completed.id;
                                    accepted_errands
                                        .with_mut(|list| {
                                            if let Some(existing) = list
                                                .iter_mut()
                                                .find(|e| e.id == completed_id)
                                            {
                                                *existing = completed;
                                            }
                                        });
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NearbyErrandRowProps {
    errand: Errand,
    on_accepted: EventHandler<Errand>,
    on_unavailable: EventHandler<u64>,
}

/// Una fila de la lista con su propio estado de "ver foto" (issue #78) y de
/// "aceptar" (issue #79): componente aparte, igual que `NearbyRideRow` en
/// `nearby_rides.rs`, para que estas acciones en un mandado no afecten a los
/// demas.
#[component]
fn NearbyErrandRow(props: NearbyErrandRowProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading_photo = use_signal(|| false);
    let mut photo_error = use_signal(|| None::<String>);
    let mut photo_data_uri = use_signal(|| None::<String>);

    let mut agreed_price = use_signal(String::new);
    let mut is_accepting = use_signal(|| false);
    let mut accept_error = use_signal(|| None::<String>);

    let errand_id = props.errand.id;
    let on_accepted = props.on_accepted;
    let on_unavailable = props.on_unavailable;

    let api_client_for_accept = api_client.clone();
    let storage_for_accept = storage.clone();

    let on_accept_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Ok(price) = agreed_price().trim().parse::<i64>() else {
            return;
        };
        if price < 1 {
            return;
        }
        let api_client = api_client_for_accept.clone();
        let storage = storage_for_accept.clone();

        spawn(async move {
            is_accepting.set(true);
            accept_error.set(None);

            match api_client.accept_errand(&token, errand_id, price).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_accepted.call(fetch.data);
                }
                Err(AcceptErrandError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(AcceptErrandError::Conflict) => {
                    on_unavailable.call(errand_id);
                }
                Err(err) => {
                    accept_error.set(Some(err.to_string()));
                }
            }

            is_accepting.set(false);
        });
    };

    let on_view_photo_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading_photo.set(true);
            photo_error.set(None);

            match api_client.errand_photo(&token, errand_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    let encoded = BASE64.encode(&fetch.data.bytes);
                    photo_data_uri.set(Some(format!(
                        "data:{};base64,{}",
                        fetch.data.content_type, encoded
                    )));
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    photo_error.set(Some(err.to_string()));
                }
            }

            is_loading_photo.set(false);
        });
    };

    let price_is_valid = agreed_price()
        .trim()
        .parse::<i64>()
        .is_ok_and(|price| price >= 1);

    rsx! {
        li { class: "nearby-errand-row",
            p { "{props.errand.description}" }
            p {
                "Origen: {props.errand.origin.latitude}, {props.errand.origin.longitude}"
            }
            p { "Destino: {props.errand.destination.name}" }
            if props.errand.has_photo {
                if let Some(data_uri) = photo_data_uri() {
                    img { class: "nearby-errand-photo", src: "{data_uri}" }
                } else {
                    button {
                        r#type: "button",
                        class: "nearby-errand-photo-button",
                        disabled: is_loading_photo(),
                        onclick: on_view_photo_click,
                        if is_loading_photo() {
                            "Cargando foto..."
                        } else {
                            "Ver foto"
                        }
                    }
                }
                if let Some(message) = photo_error() {
                    p { class: "nearby-errand-photo-error", role: "alert", "{message}" }
                }
            }
            label { r#for: "errand-{errand_id}-price", "Precio acordado" }
            input {
                id: "errand-{errand_id}-price",
                r#type: "number",
                min: "1",
                disabled: is_accepting(),
                value: "{agreed_price}",
                oninput: move |event| agreed_price.set(event.value()),
            }
            button {
                r#type: "button",
                class: "nearby-errand-accept-button",
                disabled: is_accepting() || !price_is_valid,
                onclick: on_accept_click,
                if is_accepting() {
                    "Aceptando..."
                } else {
                    "Aceptar"
                }
            }
            if let Some(message) = accept_error() {
                p { class: "nearby-errand-accept-error", role: "alert", "{message}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AcceptedErrandRowProps {
    errand: Errand,
    on_completed: EventHandler<Errand>,
}

/// Una fila de "Mandados que aceptaste en esta sesion" con su propio estado
/// de "completar" (issue #80): componente aparte, mismo criterio que
/// `NearbyErrandRow`, para que completar un mandado no afecte a los demas.
#[component]
fn AcceptedErrandRow(props: AcceptedErrandRowProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_completing = use_signal(|| false);
    let mut complete_error = use_signal(|| None::<String>);

    let errand_id = props.errand.id;
    let on_completed = props.on_completed;

    let on_complete_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_completing.set(true);
            complete_error.set(None);

            match api_client.complete_errand(&token, errand_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_completed.call(fetch.data);
                }
                Err(CompleteErrandError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    complete_error.set(Some(err.to_string()));
                }
            }

            is_completing.set(false);
        });
    };

    rsx! {
        li { class: "accepted-errand-row",
            p { "{props.errand.description}" }
            p { "Destino: {props.errand.destination.name}" }
            if let Some(price) = props.errand.agreed_price {
                p { "Precio acordado: {price}" }
            }
            if props.errand.status == ErrandStatus::Completed {
                p { class: "accepted-errand-completed", "Completado." }
            } else {
                button {
                    r#type: "button",
                    class: "accepted-errand-complete-button",
                    disabled: is_completing(),
                    onclick: on_complete_click,
                    if is_completing() {
                        "Completando..."
                    } else {
                        "Completar"
                    }
                }
                if let Some(message) = complete_error() {
                    p { class: "accepted-errand-complete-error", role: "alert", "{message}" }
                }
            }
        }
    }
}
