//! Pantalla de vehiculo (conductor): registra el vehiculo si todavia no
//! tiene uno (issue #11, `RegisterVehicleScreen`) o permite corregir sus
//! datos si ya lo tiene (issue #12).
//!
//! Al entrar, consulta `GET /api/v1/me/vehicle` para decidir cual de las dos
//! mostrar: un 404 (`GetVehicleError::NotFound`) significa que el conductor
//! todavia no registro nada — ese es el criterio de aceptacion que evita que
//! la pantalla de edicion sea alcanzable sin vehiculo, sin depender de una
//! bandera local que se perdería al desmontar el componente (`Home`, en
//! `moto_ui/src/lib.rs`, desmonta cada seccion al cambiar de pestaña).
//! Cualquier otro resultado exitoso es el vehiculo ya registrado, precargado
//! en el formulario de edicion.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, GetVehicleError, UpdateVehicleError};
use moto_core::models::Vehicle;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::register_vehicle::RegisterVehicleScreen;

#[component]
pub fn VehicleScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut vehicle = use_signal(|| None::<Vehicle>);
    let mut not_registered = use_signal(|| false);
    // Misma guarda que `ProfileScreen` (issue #9): el efecto lee
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

            match api_client.get_vehicle(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    vehicle.set(Some(fetch.data));
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
            div { class: "vehicle-screen", p { "Cargando vehiculo..." } }
        } else if let Some(message) = load_error() {
            div { class: "vehicle-screen",
                p { class: "vehicle-error", role: "alert", "{message}" }
            }
        } else if let Some(current) = vehicle() {
            EditVehicleForm { vehicle: current }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EditVehicleFormProps {
    vehicle: Vehicle,
}

/// Formulario de edicion de placa/modelo/anio — issue #12. Precargado con
/// `props.vehicle`; al guardar manda los tres campos a
/// `ApiClient::update_vehicle` (PATCH parcial, mismo criterio que
/// `EditProfileForm` en `profile.rs`: el formulario siempre esta
/// completamente precargado, asi que ningun campo queda realmente ausente).
#[component]
fn EditVehicleForm(props: EditVehicleFormProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut plate = use_signal(|| props.vehicle.plate.clone());
    let mut model = use_signal(|| props.vehicle.model.clone());
    let mut year = use_signal(|| props.vehicle.year.to_string());
    let mut update_error = use_signal(|| None::<UpdateVehicleError>);
    let mut is_saving = use_signal(|| false);
    let mut updated_vehicle = use_signal(|| None::<Vehicle>);

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
            is_saving.set(true);
            update_error.set(None);

            match api_client
                .update_vehicle(
                    &token,
                    Some(&plate_value),
                    Some(&model_value),
                    Some(&year_value),
                )
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    updated_vehicle.set(Some(fetch.data));
                }
                Err(UpdateVehicleError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    update_error.set(Some(err));
                }
            }

            is_saving.set(false);
        });
    };

    let current_error = update_error();
    let plate_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("plate"));
    let model_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("model"));
    let year_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("year"));
    // Igual que en `EditProfileForm` (issue #10): el mensaje generico solo
    // se muestra cuando el error no trae desglose campo por campo.
    let has_field_errors = plate_error.is_some() || model_error.is_some() || year_error.is_some();
    let general_message = if has_field_errors {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        div { class: "vehicle-screen",
            h2 { "Mi vehiculo" }
            if let Some(vehicle) = updated_vehicle() {
                dl { class: "update-vehicle-result",
                    dt { "Placa" }
                    dd { "{vehicle.plate}" }
                    dt { "Modelo" }
                    dd { "{vehicle.model}" }
                    dt { "Anio" }
                    dd { "{vehicle.year}" }
                }
            } else {
                form { class: "update-vehicle-form", onsubmit: on_submit,
                    label { r#for: "update-vehicle-plate", "Placa" }
                    input {
                        id: "update-vehicle-plate",
                        r#type: "text",
                        autocomplete: "off",
                        disabled: is_saving(),
                        value: "{plate}",
                        oninput: move |event| plate.set(event.value()),
                    }
                    if let Some(message) = &plate_error {
                        p { class: "update-vehicle-field-error", role: "alert", "{message}" }
                    }
                    label { r#for: "update-vehicle-model", "Modelo" }
                    input {
                        id: "update-vehicle-model",
                        r#type: "text",
                        autocomplete: "off",
                        disabled: is_saving(),
                        value: "{model}",
                        oninput: move |event| model.set(event.value()),
                    }
                    if let Some(message) = &model_error {
                        p { class: "update-vehicle-field-error", role: "alert", "{message}" }
                    }
                    label { r#for: "update-vehicle-year", "Anio" }
                    input {
                        id: "update-vehicle-year",
                        r#type: "number",
                        autocomplete: "off",
                        disabled: is_saving(),
                        value: "{year}",
                        oninput: move |event| year.set(event.value()),
                    }
                    if let Some(message) = &year_error {
                        p { class: "update-vehicle-field-error", role: "alert", "{message}" }
                    }
                    button { r#type: "submit", disabled: is_saving(),
                        if is_saving() {
                            "Guardando..."
                        } else {
                            "Guardar cambios"
                        }
                    }
                }
                if let Some(message) = general_message {
                    p { class: "update-vehicle-error", role: "alert", "{message}" }
                }
            }
        }
    }
}
