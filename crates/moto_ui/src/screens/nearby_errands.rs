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
    ServiceActionError,
};
use moto_core::models::{Errand, ErrandStatus, Role};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::PickupLocation;
use crate::price_offer::{ConfirmCancelButton, ContactCard};

/// Intervalo entre sondeos de `GET /api/v1/errands`. Antes eran 700 ms
/// (copiado del loop del socket de tiempo real), que en un celular con
/// datos era una peticion HTTP por segundo: 8 s alcanzan para un mandado.
const POLL_INTERVAL: Duration = Duration::from_secs(8);

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

            // Recupera los mandados que este conductor ya tenia aceptados
            // (backend #106, `GET /me/errands?as=driver`): antes se perdian
            // al cerrar la app o cambiar de pantalla.
            if let Ok(fetch) = api_client.my_errands(&current_token, Role::Driver).await {
                let pending: Vec<Errand> = fetch
                    .data
                    .into_iter()
                    .filter(|errand| errand.status == ErrandStatus::Accepted)
                    .collect();
                if !pending.is_empty() {
                    accepted_errands.set(pending);
                }
            }

            loop {
                // Los aceptados se refrescan desde el servidor: asi se ve
                // cuando el pasajero acepta el precio (y se habilita
                // "Completar") o lo rechaza (el mandado deja de ser de este
                // conductor y vuelve a la lista) — backend #107.
                // Los aceptados se refrescan desde el servidor: asi se ve si
                // el pasajero acepta el precio (y se habilita "Completar"),
                // lo rechaza (vuelve a la lista) o cancela el mandado.
                let open_ids: Vec<u64> = accepted_errands
                    .peek()
                    .iter()
                    .filter(|errand| errand.status == ErrandStatus::Accepted)
                    .map(|errand| errand.id)
                    .collect();
                if !open_ids.is_empty()
                    && let Ok(fetch) = api_client.my_errands(&current_token, Role::Driver).await
                {
                    let outcome = reconcile_accepted(&accepted_errands.peek(), &fetch.data);
                    if accepted_errands.peek().as_slice() != outcome.kept.as_slice() {
                        accepted_errands.set(outcome.kept);
                    }
                    if let Some(message) = outcome.notice {
                        unavailable_notice.set(Some(message.to_string()));
                    }
                }

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
                    h3 { "Mandados que aceptaste" }
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
                                on_released: move |released_id: u64| {
                                    accepted_errands
                                        .with_mut(|list| list.retain(|e| e.id != released_id));
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
            PickupLocation { lat: props.errand.origin.latitude, lng: props.errand.origin.longitude }
            if props.errand.is_motocarga() {
                p { class: "errand-kind-badge", "📦 Motocarga" }
            }
            if let Some(destination) = props.errand.destination_label() {
                p { "Destino: {destination}" }
            }
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

/// Resultado de cruzar los mandados aceptados que muestra la pantalla con
/// los que el servidor dice que son de este conductor (`GET /me/errands`).
#[derive(Debug, PartialEq)]
struct Reconciled {
    kept: Vec<Errand>,
    notice: Option<&'static str>,
}

/// Se quedan los que siguen siendo del conductor (actualizados); salen los
/// que el pasajero cancelo o los que volvieron a la lista porque rechazo el
/// precio, con el aviso correspondiente. Logica pura, testeada abajo.
fn reconcile_accepted(local: &[Errand], server: &[Errand]) -> Reconciled {
    let mut kept = Vec::new();
    let mut notice = None;
    for errand in local {
        match server.iter().find(|fresh| fresh.id == errand.id) {
            Some(fresh) if fresh.status == ErrandStatus::Cancelled => {
                notice = Some("El pasajero cancelo el mandado.");
            }
            Some(fresh) => kept.push(fresh.clone()),
            None if errand.status == ErrandStatus::Accepted => {
                notice = Some("El pasajero no acepto tu precio. El mandado volvio a la lista.");
            }
            None => kept.push(errand.clone()),
        }
    }
    Reconciled { kept, notice }
}

#[derive(Props, Clone, PartialEq)]
struct AcceptedErrandRowProps {
    errand: Errand,
    on_completed: EventHandler<Errand>,
    on_released: EventHandler<u64>,
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

    // Soltar el mandado (backend #108): vuelve a la lista para otro conductor.
    let mut is_releasing = use_signal(|| false);
    let release_api_client = api_client.clone();
    let release_storage = storage.clone();
    let errand_id = props.errand.id;
    let on_released = props.on_released;
    let release = use_callback(move |_: ()| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = release_api_client.clone();
        let storage = release_storage.clone();
        spawn(async move {
            is_releasing.set(true);
            complete_error.set(None);
            match api_client.cancel_errand(&token, errand_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_released.call(errand_id);
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => complete_error.set(Some(err.to_string())),
            }
            is_releasing.set(false);
        });
    });

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
            if props.errand.is_motocarga() {
                p { class: "errand-kind-badge", "📦 Motocarga" }
            }
            if let Some(destination) = props.errand.destination_label() {
                p { "Destino: {destination}" }
            }
            if let Some(price) = props.errand.agreed_price {
                p { "Precio acordado: {price}" }
            }
            if props.errand.status == ErrandStatus::Completed {
                p { class: "accepted-errand-completed", "Completado." }
            } else {
                PickupLocation { lat: props.errand.origin.latitude, lng: props.errand.origin.longitude }
                if let Some(counterpart) = props.errand.counterpart.clone() {
                    ContactCard { title: "Tu pasajero", counterpart }
                }
                if props.errand.awaiting_price_response() {
                    p { class: "upload-status", role: "status",
                        span { class: "upload-spinner" }
                        "Esperando que el pasajero acepte tu precio..."
                    }
                } else {
                    p { class: "upload-status upload-status-done", role: "status",
                        "✓ El pasajero acepto el precio."
                    }
                }
                button {
                    r#type: "button",
                    class: "accepted-errand-complete-button",
                    disabled: is_completing() || props.errand.awaiting_price_response(),
                    onclick: on_complete_click,
                    if is_completing() {
                        "Completando..."
                    } else {
                        "Completar"
                    }
                }
                ConfirmCancelButton {
                    label: "Soltar mandado",
                    question: "¿Soltar este mandado? Vuelve a la lista para otro conductor.",
                    is_busy: is_releasing(),
                    on_confirm: move |_| release.call(()),
                }
                if let Some(message) = complete_error() {
                    p { class: "accepted-errand-complete-error", role: "alert", "{message}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;

    fn errand(id: u64, status: ErrandStatus) -> Errand {
        serde_json::from_value(serde_json::json!({
            "id": id, "status": status, "description": "Pan",
            "has_photo": false, "photo_url": null,
            "origin": {"latitude": 3.8653, "longitude": -67.9239},
            "vehicle_type": "motocarro", "destination": null, "destination_text": null,
            "driver": {"id": 9, "name": "Luis"}, "agreed_price": 5000,
            "price_confirmed": false,
            "requested_at": "2026-10-28T10:00:00+00:00", "completed_at": null
        }))
        .unwrap()
    }

    #[test]
    fn keeps_and_updates_the_errands_still_assigned() {
        let mut fresh = errand(1, ErrandStatus::Accepted);
        fresh.price_confirmed = true;
        let out = reconcile_accepted(&[errand(1, ErrandStatus::Accepted)], &[fresh.clone()]);
        assert_eq!(out.kept, vec![fresh]);
        assert_eq!(out.notice, None);
    }

    #[test]
    fn drops_a_cancelled_errand_with_its_notice() {
        let out = reconcile_accepted(
            &[errand(1, ErrandStatus::Accepted)],
            &[errand(1, ErrandStatus::Cancelled)],
        );
        assert!(out.kept.is_empty());
        assert_eq!(out.notice, Some("El pasajero cancelo el mandado."));
    }

    #[test]
    fn drops_an_errand_whose_price_was_rejected() {
        let out = reconcile_accepted(&[errand(1, ErrandStatus::Accepted)], &[]);
        assert!(out.kept.is_empty());
        assert!(out.notice.unwrap().contains("no acepto tu precio"));
    }

    #[test]
    fn keeps_a_completed_errand_that_already_left_the_server_list() {
        let done = errand(2, ErrandStatus::Completed);
        let out = reconcile_accepted(std::slice::from_ref(&done), &[]);
        assert_eq!(out.kept, vec![done]);
    }
}
