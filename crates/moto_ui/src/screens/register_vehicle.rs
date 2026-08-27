//! Pantalla de registro de vehiculo (conductor) — issue #11.
//!
//! Solo alcanzable para conductores: `Home` (`moto_ui/src/lib.rs`) no ofrece
//! esta seccion en la navegacion cuando `session.user()` no es una cuenta de
//! conductor — ese es el criterio de aceptacion que evita que un pasajero
//! llegue aca desde la UI. El backend igual rechaza con 403 a una cuenta no
//! conductora que llegue por otra via (`VehiclePolicy::create`), pero la
//! pantalla no depende de eso para ocultarse.
//!
//! Consume `POST /api/v1/me/vehicle` a traves de `ApiClient::register_vehicle`.
//! Un conductor que ya tiene un vehiculo registrado recibe el 422 del backend
//! (clave sintetica `vehicle`, no un campo del formulario) como mensaje
//! general — actualizar el vehiculo existente es la historia #12, fuera de
//! alcance aca.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, RegisterVehicleError};
use moto_core::models::Vehicle;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

#[component]
pub fn RegisterVehicleScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut plate = use_signal(String::new);
    let mut model = use_signal(String::new);
    let mut year = use_signal(String::new);
    let mut register_error = use_signal(|| None::<RegisterVehicleError>);
    let mut is_loading = use_signal(|| false);
    let mut registered_vehicle = use_signal(|| None::<Vehicle>);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let Some(token) = session.token() else {
            return;
        };
        let plate_value = plate();
        let model_value = model();
        let year_value = year();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            register_error.set(None);

            match api_client
                .register_vehicle(&token, &plate_value, &model_value, &year_value)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    registered_vehicle.set(Some(fetch.data));
                }
                Err(RegisterVehicleError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    register_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    };

    if let Some(vehicle) = registered_vehicle() {
        return rsx! {
            div { class: "register-vehicle-screen",
                h2 { "Vehiculo registrado" }
                dl { class: "register-vehicle-result",
                    dt { "Placa" }
                    dd { "{vehicle.plate}" }
                    dt { "Modelo" }
                    dd { "{vehicle.model}" }
                    dt { "Anio" }
                    dd { "{vehicle.year}" }
                }
            }
        };
    }

    let current_error = register_error();
    let plate_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("plate"));
    let model_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("model"));
    let year_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("year"));
    // Igual que en el registro de conductor (issue #7): el mensaje generico
    // solo se muestra cuando el error no trae desglose campo por campo. Esto
    // tambien cubre el caso de vehiculo duplicado (422 bajo la clave
    // sintetica `vehicle`, que no coincide con ningun campo del formulario).
    let has_field_errors = plate_error.is_some() || model_error.is_some() || year_error.is_some();
    let general_message = if has_field_errors {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        div { class: "register-vehicle-screen",
            h2 { "Registrar mi vehiculo" }
            form { onsubmit: on_submit,
                label { r#for: "register-vehicle-plate", "Placa" }
                input {
                    id: "register-vehicle-plate",
                    r#type: "text",
                    autocomplete: "off",
                    disabled: is_loading(),
                    value: "{plate}",
                    oninput: move |event| plate.set(event.value()),
                }
                if let Some(message) = &plate_error {
                    p { class: "register-vehicle-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-vehicle-model", "Modelo" }
                input {
                    id: "register-vehicle-model",
                    r#type: "text",
                    autocomplete: "off",
                    disabled: is_loading(),
                    value: "{model}",
                    oninput: move |event| model.set(event.value()),
                }
                if let Some(message) = &model_error {
                    p { class: "register-vehicle-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-vehicle-year", "Anio" }
                input {
                    id: "register-vehicle-year",
                    r#type: "number",
                    autocomplete: "off",
                    disabled: is_loading(),
                    value: "{year}",
                    oninput: move |event| year.set(event.value()),
                }
                if let Some(message) = &year_error {
                    p { class: "register-vehicle-field-error", role: "alert", "{message}" }
                }
                button { r#type: "submit", disabled: is_loading(),
                    if is_loading() {
                        "Registrando..."
                    } else {
                        "Registrar vehiculo"
                    }
                }
            }
            if let Some(message) = general_message {
                p { class: "register-vehicle-error", role: "alert", "{message}" }
            }
        }
    }
}
