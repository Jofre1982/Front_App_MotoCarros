//! Formulario de mandado y de motocarga (pasajero) — `POST /api/v1/errands`.
//!
//! Lo usan dos lugares, con el mismo formulario:
//! - "Pedir motocarro" → "Mandado" (`vehicle_type: motocarro`).
//! - "Pedir motocarga" del menu (`vehicle_type: motocarga`): solo lo ven y
//!   aceptan conductores con motocarga.
//!
//! Pensado para gente que no esta acostumbrada a estas apps (pedido del
//! dueno): marcar en el mapa donde se recoge, escribir que hay que hacer o
//! llevar, y pedir. A donde llevarlo es opcional y la foto tambien. Sin
//! precio: se acuerda con el conductor, que lo registra al aceptar.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, CreateErrandError};
use moto_core::models::{Coordinates, CreateErrandPayload, Errand, ErrandStatus, VehicleType};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::file_picker::{PhotoPicker, PickedFile};
use crate::map::{MapMarker, MapView};

/// Centro inicial del mapa: casco urbano de Inirida (ver `ride_estimate.rs`).
const DEFAULT_CENTER_LAT: f64 = 3.8653;
const DEFAULT_CENTER_LNG: f64 = -67.9239;

/// Textos que cambian entre mandado y motocarga. Separado para poder
/// probarlo sin Dioxus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrandCopy {
    pub title: &'static str,
    pub pickup_step: &'static str,
    pub description_step: &'static str,
    pub description_placeholder: &'static str,
    pub submit: &'static str,
    pub created_title: &'static str,
}

pub fn errand_copy(vehicle_type: VehicleType) -> ErrandCopy {
    match vehicle_type {
        VehicleType::Motocarro => ErrandCopy {
            title: "Pedir un mandado",
            pickup_step: "1. ¿Donde se hace el mandado?",
            description_step: "2. ¿Que necesitas?",
            description_placeholder: "Ej.: comprar 2 panes y una leche en la tienda de la esquina",
            submit: "Pedir mandado",
            created_title: "Mandado pedido",
        },
        VehicleType::Motocarga => ErrandCopy {
            title: "Pedir motocarga",
            pickup_step: "1. ¿Donde recogemos la carga?",
            description_step: "2. ¿Que hay que llevar?",
            description_placeholder: "Ej.: una nevera y dos cajas medianas",
            submit: "Pedir motocarga",
            created_title: "Motocarga pedida",
        },
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct RequestErrandScreenProps {
    pub vehicle_type: VehicleType,
    /// Dentro de "Pedir motocarro" el titulo ya esta arriba.
    #[props(default = false)]
    pub embedded: bool,
}

#[component]
pub fn RequestErrandScreen(props: RequestErrandScreenProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let copy = errand_copy(props.vehicle_type);
    let vehicle_type = props.vehicle_type;

    let mut origin = use_signal(|| None::<(f64, f64)>);
    let mut description = use_signal(String::new);
    let mut destination_text = use_signal(String::new);
    let selected_photo = use_signal(|| None::<PickedFile>);
    let mut is_submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<CreateErrandError>);
    let mut created_errand = use_signal(|| None::<Errand>);

    let on_map_click = move |(lat, lng): (f64, f64)| {
        origin.set(Some((lat, lng)));
    };

    let on_submit_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some((origin_lat, origin_lng)) = origin() else {
            return;
        };
        let description_value = description().trim().to_string();
        if description_value.is_empty() {
            return;
        }
        let destination_value = destination_text().trim().to_string();
        let photo = selected_photo().map(|file| (file.name, file.content_type, file.bytes));
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_submitting.set(true);
            submit_error.set(None);

            let payload = CreateErrandPayload {
                description: description_value,
                origin: Coordinates {
                    latitude: origin_lat,
                    longitude: origin_lng,
                },
                destination_text: Some(destination_value).filter(|text| !text.is_empty()),
                vehicle_type,
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

    let mut selected_photo_for_reset = selected_photo;
    let on_new_click = move |_| {
        created_errand.set(None);
        origin.set(None);
        description.set(String::new());
        destination_text.set(String::new());
        selected_photo_for_reset.set(None);
        submit_error.set(None);
    };

    if let Some(errand) = created_errand() {
        return rsx! {
            div { class: "request-errand-screen",
                div { class: "request-errand-done", role: "status",
                    h2 { "✓ {copy.created_title}" }
                    p { "{errand_status_label(errand.status)}" }
                    p { class: "request-errand-done-detail", "{errand.description}" }
                    if let Some(destination) = errand.destination_label() {
                        p { class: "request-errand-done-detail", "Destino: {destination}" }
                    }
                    p { class: "request-errand-price-note",
                        "El precio lo acuerdas con el conductor cuando acepte."
                    }
                }
                button {
                    r#type: "button",
                    class: "request-errand-new-link",
                    onclick: on_new_click,
                    "Pedir otro"
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
    let field = |name: &str| {
        current_error
            .as_ref()
            .and_then(|err| err.field_message(name))
    };
    let description_field_error = field("description");
    let origin_field_error = field("origin.latitude").or_else(|| field("origin.longitude"));
    let destination_field_error = field("destination_text");
    let photo_field_error = field("photo");
    // Mismo criterio que el resto de los formularios (issue #6): el mensaje
    // generico solo cuando el error no trae desglose por campo.
    let general_message = if description_field_error.is_some()
        || origin_field_error.is_some()
        || destination_field_error.is_some()
        || photo_field_error.is_some()
    {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    let missing_hint = match (origin().is_some(), !description().trim().is_empty()) {
        (false, _) => Some("Toca el mapa para marcar el punto de recogida."),
        (true, false) => Some("Escribe que necesitas."),
        (true, true) => None,
    };
    let can_submit = missing_hint.is_none() && !is_submitting();

    rsx! {
        div { class: "request-errand-screen",
            if !props.embedded {
                h2 { "{copy.title}" }
            }
            p { class: "ride-step-title", "{copy.pickup_step}" }
            p { class: "ride-estimate-instructions",
                if origin().is_some() {
                    "Listo, ya marcamos el punto. Puedes tocar otro lugar para cambiarlo."
                } else {
                    "Toca el mapa en el lugar."
                }
            }
            div { class: "request-errand-map", style: "height: 300px;",
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
            label { r#for: "request-errand-description", class: "ride-step-title", "{copy.description_step}" }
            textarea {
                id: "request-errand-description",
                rows: "3",
                maxlength: "500",
                placeholder: "{copy.description_placeholder}",
                disabled: is_submitting(),
                value: "{description}",
                oninput: move |event| description.set(event.value()),
            }
            if let Some(message) = &description_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            label { r#for: "request-errand-destination", class: "ride-step-title",
                "3. ¿A donde lo llevamos? "
                span { class: "optional-tag", "(opcional)" }
            }
            input {
                id: "request-errand-destination",
                r#type: "text",
                maxlength: "255",
                placeholder: "Ej.: mi casa, barrio La Esperanza",
                disabled: is_submitting(),
                value: "{destination_text}",
                oninput: move |event| destination_text.set(event.value()),
            }
            if let Some(message) = &destination_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            p { class: "ride-step-title",
                "4. Foto "
                span { class: "optional-tag", "(opcional)" }
            }
            PhotoPicker { selected: selected_photo, disabled: is_submitting() }
            if let Some(message) = &photo_field_error {
                p { class: "request-errand-field-error", role: "alert", "{message}" }
            }
            p { class: "request-errand-price-note",
                "Precio: lo acuerdas con el conductor cuando acepte."
            }
            button {
                r#type: "button",
                class: "request-errand-submit-button",
                disabled: !can_submit,
                onclick: on_submit_click,
                if is_submitting() {
                    if selected_photo().is_some() {
                        "Enviando foto y pedido..."
                    } else {
                        "Pidiendo..."
                    }
                } else {
                    "{copy.submit}"
                }
            }
            if let Some(hint) = missing_hint {
                p { class: "ride-estimate-empty", "{hint}" }
            }
            if let Some(message) = general_message {
                p { class: "request-errand-error", role: "alert", "{message}" }
            }
        }
    }
}

/// Texto del estado de un pedido recien creado (`Errand::status`). Justo
/// despues de `POST /api/v1/errands` siempre nace `requested`; los demas
/// valores se cubren igual porque el tipo los admite.
fn errand_status_label(status: ErrandStatus) -> &'static str {
    match status {
        ErrandStatus::Requested => "Esperando a que un conductor lo acepte.",
        ErrandStatus::Accepted => "Un conductor lo acepto.",
        ErrandStatus::Completed => "Ya se completo.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "Ya se completo."
        );
    }

    #[test]
    fn la_motocarga_habla_de_carga_y_el_mandado_de_lo_que_necesitas() {
        let carga = errand_copy(VehicleType::Motocarga);
        let mandado = errand_copy(VehicleType::Motocarro);
        assert_eq!(carga.submit, "Pedir motocarga");
        assert_eq!(mandado.submit, "Pedir mandado");
        assert!(carga.description_step.contains("llevar"));
        assert!(mandado.description_step.contains("necesitas"));
    }
}
