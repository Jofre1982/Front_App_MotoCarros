//! Pantalla de pedir un mandado (servicio a domicilio, pasajero) — historia
//! #92 del backend, issue #76 de este repo.
//!
//! Consume `POST /api/v1/errands` (`ApiClient::create_errand`) directamente,
//! sin paso de estimacion: a diferencia de un viaje (`RideEstimateScreen`),
//! un mandado no tiene tarifa fija — el precio se negocia por fuera del
//! sistema y el conductor lo registra al aceptar (`AcceptErrandPayload`,
//! issue #79). El destino se elige del mismo catalogo de sitios
//! (`GET /sites`, `ApiClient::list_sites`) que usa `RideEstimateScreen`; el
//! origen (donde recoger el mandado) sigue siendo un punto libre en el mapa,
//! igual que el origen de un viaje.
//!
//! La foto es opcional y se sube junto con el resto de los campos en la
//! misma llamada multipart (`photo`, jpg/jpeg/png hasta 5 MB segun
//! `CreateErrandRequest::rules()` del backend): el campo de archivo no
//! valida formato/tamano en el cliente antes de mandar, mismo criterio que
//! `DocumentsScreen` (`accept` del input mas el desglose por campo de un 422
//! son suficientes, ver `CreateErrandError::field_message`).
//!
//! Una vez creado, esta pantalla solo confirma el mandado (estado, sitio,
//! descripcion): no ofrece seguimiento en tiempo real, cancelar, ni un
//! enlace al historial — el backend no expone nada de eso para mandados
//! todavia (fuera de alcance de esta historia, ver su seccion "Fuera de
//! alcance").

use std::sync::Arc;

use dioxus::html::FileData;
use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError, CreateErrandError};
use moto_core::models::{Coordinates, CreateErrandPayload, Errand, Site};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::{MapMarker, MapView};

/// Centro inicial del mapa mientras no exista geolocalizacion del
/// dispositivo — mismo punto y mismo motivo que `RideEstimateScreen`.
const DEFAULT_CENTER_LAT: f64 = 4.710989;
const DEFAULT_CENTER_LNG: f64 = -74.072092;

#[component]
pub fn RequestErrandScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut sites = use_signal(Vec::<Site>::new);
    let mut sites_error = use_signal(|| None::<String>);
    let mut is_loading_sites = use_signal(|| false);
    // Mismo criterio que `RideEstimateScreen`: evita reprogramar el efecto en
    // un loop si `session.update_token`/`logout` (dentro del propio fetch)
    // volviera a disparar la lectura de `session.token()`.
    let mut has_fetched_sites = use_signal(|| false);

    let mut origin = use_signal(|| None::<(f64, f64)>);
    let mut destination_site_id = use_signal(|| None::<u64>);
    let mut description = use_signal(String::new);
    let mut selected_photo = use_signal(|| None::<FileData>);
    let mut is_submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<CreateErrandError>);
    let mut created_errand = use_signal(|| None::<Errand>);

    let api_client_for_sites = api_client.clone();
    let storage_for_sites = storage.clone();

    use_effect(move || {
        if has_fetched_sites() {
            return;
        }

        let Some(token) = session.token() else {
            return;
        };
        has_fetched_sites.set(true);

        let api_client = api_client_for_sites.clone();
        let storage = storage_for_sites.clone();

        spawn(async move {
            is_loading_sites.set(true);
            sites_error.set(None);

            match api_client.list_sites(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    sites.set(fetch.data);
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    sites_error.set(Some(err.to_string()));
                }
            }

            is_loading_sites.set(false);
        });
    });

    let on_map_click = move |(lat, lng): (f64, f64)| {
        origin.set(Some((lat, lng)));
    };

    let on_photo_selected = move |event: FormEvent| {
        selected_photo.set(event.files().into_iter().next());
    };

    let on_submit_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some((origin_lat, origin_lng)) = origin() else {
            return;
        };
        let Some(site_id) = destination_site_id() else {
            return;
        };
        let description_value = description().trim().to_string();
        if description_value.is_empty() {
            return;
        }
        let photo = selected_photo();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_submitting.set(true);
            submit_error.set(None);

            let photo = match photo {
                Some(file) => {
                    let file_name = file.name();
                    let mime_type = file.content_type();
                    match file.read_bytes().await {
                        Ok(bytes) => Some((file_name, mime_type, bytes.to_vec())),
                        Err(_) => {
                            submit_error.set(Some(CreateErrandError::Network(
                                "No se pudo leer la foto seleccionada.".to_string(),
                            )));
                            is_submitting.set(false);
                            return;
                        }
                    }
                }
                None => None,
            };

            let payload = CreateErrandPayload {
                description: description_value,
                origin: Coordinates {
                    latitude: origin_lat,
                    longitude: origin_lng,
                },
                destination_site_id: site_id,
            };

            match api_client.create_errand(&token, payload, photo).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    created_errand.set(Some(fetch.data));
                }
                Err(CreateErrandError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    submit_error.set(Some(err));
                }
            }

            is_submitting.set(false);
        });
    };

    if let Some(errand) = created_errand() {
        return rsx! {
            div { class: "request-errand-screen",
                h2 { "Mandado solicitado" }
                dl { class: "request-errand-result",
                    dt { "Estado" }
                    dd { "{errand_status_label(errand.status)}" }
                    dt { "Descripcion" }
                    dd { "{errand.description}" }
                    dt { "Destino" }
                    dd { "{errand.destination.name}" }
                }
            }
        };
    }

    let markers: Vec<MapMarker> = origin()
        .map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Recogida".to_string()),
        })
        .into_iter()
        .collect();

    let current_error = submit_error();
    let description_field_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("description"));
    let origin_field_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("origin.latitude"))
        .or_else(|| {
            current_error
                .as_ref()
                .and_then(|err| err.field_message("origin.longitude"))
        });
    let destination_field_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("destination_site_id"));
    let photo_field_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("photo"));
    // Mismo criterio que el resto de los formularios de esta app (issue #6):
    // el mensaje generico solo se muestra cuando el error no trae ningun
    // desglose por campo.
    let general_message = if description_field_error.is_some()
        || origin_field_error.is_some()
        || destination_field_error.is_some()
        || photo_field_error.is_some()
    {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    let can_submit = origin().is_some()
        && destination_site_id().is_some()
        && !description().trim().is_empty()
        && !is_submitting();

    rsx! {
        div { class: "request-errand-screen",
            h2 { "Pedir un mandado" }
            p { class: "request-errand-instructions",
                "Toca el mapa para elegir donde recoger el mandado."
            }
            div { class: "request-errand-map", style: "height: 320px;",
                MapView {
                    center_lat: DEFAULT_CENTER_LAT,
                    center_lng: DEFAULT_CENTER_LNG,
                    markers,
                    on_click: on_map_click,
                }
            }
            if let Some(message) = &origin_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            label { r#for: "request-errand-description", "Descripcion" }
            textarea {
                id: "request-errand-description",
                disabled: is_submitting(),
                value: "{description}",
                oninput: move |event| description.set(event.value()),
            }
            if let Some(message) = &description_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            label { r#for: "request-errand-destination", "Destino" }
            if is_loading_sites() {
                p { "Cargando sitios..." }
            } else if let Some(message) = sites_error() {
                p { class: "request-errand-sites-error", role: "alert", "{message}" }
            } else {
                select {
                    id: "request-errand-destination",
                    disabled: is_submitting(),
                    value: destination_site_id().map(|id| id.to_string()).unwrap_or_default(),
                    onchange: move |event| {
                        destination_site_id.set(event.value().parse::<u64>().ok());
                    },
                    option { value: "", disabled: true, "Elige un sitio" }
                    for site in sites() {
                        option { key: "{site.id}", value: "{site.id}", "{site.name}" }
                    }
                }
            }
            if let Some(message) = &destination_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            label { r#for: "request-errand-photo", "Foto (opcional)" }
            input {
                id: "request-errand-photo",
                r#type: "file",
                accept: "image/jpeg,image/png",
                disabled: is_submitting(),
                onchange: on_photo_selected,
            }
            if let Some(file) = selected_photo() {
                p { class: "request-errand-selected-photo", "Archivo elegido: {file.name()}" }
            }
            if let Some(message) = &photo_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            button {
                r#type: "button",
                class: "request-errand-submit-button",
                disabled: !can_submit,
                onclick: on_submit_click,
                if is_submitting() {
                    "Solicitando..."
                } else {
                    "Pedir mandado"
                }
            }
            if let Some(message) = general_message {
                p { class: "request-errand-error", role: "alert", "{message}" }
            }
        }
    }
}

/// Texto del estado de un mandado recien solicitado (`Errand::status`). Justo
/// despues de `POST /api/v1/errands` siempre nace `requested`; los demas
/// valores no ocurren en esta pantalla (no hay seguimiento aca, ver el
/// comentario de modulo), pero se cubren igual porque el tipo los admite.
fn errand_status_label(status: moto_core::models::ErrandStatus) -> &'static str {
    use moto_core::models::ErrandStatus;

    match status {
        ErrandStatus::Requested => "Esperando a que un conductor lo acepte.",
        ErrandStatus::Accepted => "Un conductor acepto tu mandado.",
        ErrandStatus::Completed => "Tu mandado ya se completo.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moto_core::models::ErrandStatus;

    #[test]
    fn errand_status_label_describes_a_freshly_requested_errand() {
        assert_eq!(
            errand_status_label(ErrandStatus::Requested),
            "Esperando a que un conductor lo acepte."
        );
    }

    #[test]
    fn errand_status_label_describes_a_completed_errand() {
        assert_eq!(
            errand_status_label(ErrandStatus::Completed),
            "Tu mandado ya se completo."
        );
    }
}
