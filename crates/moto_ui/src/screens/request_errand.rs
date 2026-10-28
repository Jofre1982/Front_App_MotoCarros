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
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{
    ApiClient, AuthenticatedRequestError, CreateErrandError, RespondToPriceError,
};
use moto_core::models::{
    Coordinates, CreateErrandPayload, Errand, ErrandStatus, Role, VehicleType, format_cop,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::file_picker::{PhotoPicker, PickedFile};
use crate::map::{MapMarker, MapView};
use crate::price_offer::PriceOfferCard;

/// Centro inicial del mapa: casco urbano de Inirida (ver `ride_estimate.rs`).
const DEFAULT_CENTER_LAT: f64 = 3.8653;
const DEFAULT_CENTER_LNG: f64 = -67.9239;

/// Cada cuanto se vuelve a consultar el pedido mientras sigue abierto.
const ERRAND_REFRESH_INTERVAL: Duration = Duration::from_secs(8);

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
    let mut tracking_started = use_signal(|| false);

    // Seguimiento del pedido (backend #106, `GET /me/errands`): al abrir la
    // pantalla se recupera el pedido que sigue abierto, y mientras no se
    // complete se vuelve a consultar cada `ERRAND_REFRESH_INTERVAL` para
    // mostrar si ya lo aceptaron, quien y a que precio. Antes el pasajero
    // no se enteraba de nada despues de pedir (reportado probando el APK).
    {
        let api_client = api_client.clone();
        let storage = storage.clone();
        use_effect(move || {
            if tracking_started() {
                return;
            }
            tracking_started.set(true);
            let api_client = api_client.clone();
            let storage = storage.clone();
            spawn(async move {
                let mut first = true;
                while let Some(token) = session.token() {
                    // Un pedido completado ya no cambia: se deja de consultar
                    // hasta que el pasajero pida otro.
                    let watching = created_errand
                        .peek()
                        .as_ref()
                        .filter(|errand| errand.status != ErrandStatus::Completed)
                        .map(|errand| errand.id);
                    if first || watching.is_some() {
                        match api_client.my_errands(&token, Role::Passenger).await {
                            Ok(fetch) => {
                                if let Some(refreshed) = fetch.refreshed_token {
                                    session.update_token(refreshed, storage.as_ref());
                                }
                                let found = match watching {
                                    Some(id) => {
                                        fetch.data.into_iter().find(|errand| errand.id == id)
                                    }
                                    None => open_errand_of_kind(fetch.data, vehicle_type),
                                };
                                if let Some(errand) = found
                                    .filter(|errand| created_errand.peek().as_ref() != Some(errand))
                                {
                                    created_errand.set(Some(errand));
                                }
                            }
                            Err(AuthenticatedRequestError::SessionExpired) => {
                                session.logout(storage.as_ref());
                                break;
                            }
                            Err(_) => {}
                        }
                        first = false;
                    }
                    Delay::new(ERRAND_REFRESH_INTERVAL).await;
                }
            });
        });
    }

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
    // Respuesta a la oferta de precio del conductor (backend #107).
    let mut is_responding_price = use_signal(|| false);
    let mut price_error = use_signal(|| None::<String>);
    let mut rejected_notice = use_signal(|| false);
    let respond_api_client = use_context::<ApiClient>();
    let respond_storage = use_context::<Arc<dyn TokenStorage>>();
    let respond_to_price = use_callback(move |accept: bool| {
        let Some(token) = session.token() else {
            return;
        };
        let Some(errand_id) = created_errand.peek().as_ref().map(|errand| errand.id) else {
            return;
        };
        let api_client = respond_api_client.clone();
        let storage = respond_storage.clone();
        spawn(async move {
            is_responding_price.set(true);
            price_error.set(None);
            match api_client
                .respond_to_errand_price(&token, errand_id, accept)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    rejected_notice.set(!accept);
                    created_errand.set(Some(fetch.data));
                }
                Err(RespondToPriceError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => price_error.set(Some(err.to_string())),
            }
            is_responding_price.set(false);
        });
    });

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
                div {
                    class: if errand.status == ErrandStatus::Requested { "request-errand-waiting" } else { "request-errand-done" },
                    role: "status",
                    h2 {
                        if errand.status == ErrandStatus::Requested {
                            "⏳ {copy.created_title}"
                        } else {
                            "✓ {copy.created_title}"
                        }
                    }
                    p { class: "request-errand-status", "{errand_status_label(errand.status)}" }
                    if let Some(driver) = &errand.driver {
                        p { class: "request-errand-done-detail", "Conductor: {driver.name}" }
                    }
                    if errand.awaiting_price_response() {
                        if let (Some(driver), Some(price)) = (&errand.driver, errand.agreed_price) {
                            PriceOfferCard {
                                driver_name: driver.name.clone(),
                                price,
                                is_responding: is_responding_price(),
                                error: price_error(),
                                on_accept: move |_| respond_to_price.call(true),
                                on_reject: move |_| respond_to_price.call(false),
                            }
                        }
                    } else if let Some(price) = errand.agreed_price {
                        p { class: "request-errand-done-detail", "Precio acordado: {format_cop(price)}" }
                    }
                    if rejected_notice() && errand.status == ErrandStatus::Requested {
                        p { class: "request-errand-done-detail",
                            "Rechazaste el precio. Tu pedido quedo libre para otro conductor."
                        }
                    }
                    p { "{errand.description}" }
                    if let Some(destination) = errand.destination_label() {
                        p { "Destino: {destination}" }
                    }
                    if errand.status == ErrandStatus::Requested {
                        p { class: "request-errand-price-note",
                            "Esta pantalla se actualiza sola cuando un conductor lo acepte."
                        }
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

/// El pedido mas reciente que sigue abierto (`requested` o `accepted`) del
/// tipo de esta pantalla (mandado o motocarga). `rides` viene del mas nuevo
/// al mas viejo.
fn open_errand_of_kind(errands: Vec<Errand>, vehicle_type: VehicleType) -> Option<Errand> {
    errands.into_iter().find(|errand| {
        errand.vehicle_type == vehicle_type && errand.status != ErrandStatus::Completed
    })
}

/// Texto del estado de un pedido recien creado (`Errand::status`). Justo
/// despues de `POST /api/v1/errands` siempre nace `requested`; los demas
/// valores se cubren igual porque el tipo los admite.
fn errand_status_label(status: ErrandStatus) -> &'static str {
    match status {
        ErrandStatus::Requested => "Esperando a que un conductor lo acepte.",
        ErrandStatus::Accepted => "Un conductor tomo tu pedido.",
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
