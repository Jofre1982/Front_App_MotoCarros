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

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::Errand;
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
                        NearbyErrandRow { key: "{errand.id}", errand }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NearbyErrandRowProps {
    errand: Errand,
}

/// Una fila de la lista con su propio estado de "ver foto" (issue #78):
/// componente aparte, igual que `NearbyRideRow` en `nearby_rides.rs`, para
/// que pedir la foto de un mandado no afecte a los demas.
#[component]
fn NearbyErrandRow(props: NearbyErrandRowProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading_photo = use_signal(|| false);
    let mut photo_error = use_signal(|| None::<String>);
    let mut photo_data_uri = use_signal(|| None::<String>);

    let errand_id = props.errand.id;

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
        }
    }
}
