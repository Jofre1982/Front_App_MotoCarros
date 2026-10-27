//! Pantalla unica de registro — reemplaza a las de pasajero (issue #6) y
//! conductor (issue #7).
//!
//! Desde la historia #99 del backend una misma cuenta puede ser pasajero y
//! conductor, y al entrar se elige el modo; tener dos botones de registro
//! ("Crear cuenta de pasajero" / "de conductor") ya no tenia sentido y
//! confundia. Toda cuenta nueva es pasajero; quien ademas quiera manejar
//! marca la casilla y deja su numero de documento (`license_number` en el
//! backend), y la cuenta se crea con `POST /auth/register/driver`, que le da
//! los dos modos. Si no la marca, puede activarlo despues desde "Mi perfil"
//! (`POST /me/driver-profile`).
//!
//! La contrasena se escribe dos veces y se compara aca, antes de enviar
//! nada: el backend no tiene campo de confirmacion.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, RegisterError};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::fields::{PasswordInput, password_confirmation_error};

#[derive(Props, Clone, PartialEq)]
pub struct RegisterScreenProps {
    /// Se dispara cuando el usuario pide volver a la pantalla de login.
    pub on_login_click: EventHandler<()>,
}

#[component]
pub fn RegisterScreen(props: RegisterScreenProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut phone = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut password_confirmation = use_signal(String::new);
    let mut wants_to_drive = use_signal(|| false);
    let mut license_number = use_signal(String::new);
    let mut register_error = use_signal(|| None::<RegisterError>);
    let mut confirmation_error = use_signal(|| None::<&'static str>);
    let mut is_loading = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let password_value = password();
        let confirmation_value = password_confirmation();
        if confirmation_value.is_empty() && !password_value.is_empty() {
            confirmation_error.set(Some("Repite la contrasena para confirmarla."));
            return;
        }
        if let Some(message) = password_confirmation_error(&password_value, &confirmation_value) {
            confirmation_error.set(Some(message));
            return;
        }
        confirmation_error.set(None);

        let name_value = name();
        let email_value = email();
        let phone_value = phone();
        let license_value = wants_to_drive().then(|| license_number.cloned());
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            register_error.set(None);

            let result = match license_value {
                Some(license) => {
                    api_client
                        .register_driver(
                            &name_value,
                            &email_value,
                            &phone_value,
                            &password_value,
                            &license,
                        )
                        .await
                }
                None => {
                    api_client
                        .register_passenger(
                            &name_value,
                            &email_value,
                            &phone_value,
                            &password_value,
                        )
                        .await
                }
            };

            match result {
                Ok(authenticated) => session.authenticate(authenticated, storage.as_ref()),
                Err(err) => register_error.set(Some(err)),
            }

            is_loading.set(false);
        });
    };

    let current_error = register_error();
    let field = |name: &str| {
        current_error
            .as_ref()
            .and_then(|err| err.field_message(name))
    };
    let name_error = field("name");
    let email_error = field("email");
    let phone_error = field("phone");
    let password_error = field("password");
    let license_number_error = field("license_number");
    // El mensaje generico solo se muestra cuando el error no trae desglose
    // campo por campo (p. ej. red, 5xx, o EmptyFields) para no repetir el
    // mismo texto dos veces cuando ya se pinta junto al input afectado.
    let has_field_errors = name_error.is_some()
        || email_error.is_some()
        || phone_error.is_some()
        || password_error.is_some()
        || license_number_error.is_some();
    let general_message = if has_field_errors {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };
    // Aviso en vivo mientras escribe la confirmacion; el de "repitela" solo
    // aparece al intentar enviar.
    let live_mismatch = password_confirmation_error(&password(), &password_confirmation());
    let confirmation_message = live_mismatch.or(confirmation_error());

    rsx! {
        div { class: "register-screen",
            h1 { "Crear cuenta" }
            p { class: "register-intro",
                "Con una sola cuenta puedes pedir motocarros y, si quieres, tambien trabajar como conductor."
            }
            form { onsubmit: on_submit,
                label { r#for: "register-name", "Nombre" }
                input {
                    id: "register-name",
                    r#type: "text",
                    autocomplete: "name",
                    disabled: is_loading(),
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
                if let Some(message) = &name_error {
                    p { class: "register-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-email", "Email" }
                input {
                    id: "register-email",
                    r#type: "email",
                    autocomplete: "username",
                    autocapitalize: "off",
                    disabled: is_loading(),
                    value: "{email}",
                    oninput: move |event| email.set(event.value()),
                }
                if let Some(message) = &email_error {
                    p { class: "register-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-phone", "Celular" }
                input {
                    id: "register-phone",
                    r#type: "tel",
                    autocomplete: "tel",
                    disabled: is_loading(),
                    value: "{phone}",
                    oninput: move |event| phone.set(event.value()),
                }
                if let Some(message) = &phone_error {
                    p { class: "register-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-password", "Contrasena" }
                PasswordInput {
                    id: "register-password",
                    value: password(),
                    autocomplete: "new-password",
                    disabled: is_loading(),
                    oninput: move |value| password.set(value),
                }
                if let Some(message) = &password_error {
                    p { class: "register-field-error", role: "alert", "{message}" }
                }
                label { r#for: "register-password-confirmation", "Repite la contrasena" }
                PasswordInput {
                    id: "register-password-confirmation",
                    value: password_confirmation(),
                    autocomplete: "new-password",
                    disabled: is_loading(),
                    oninput: move |value| {
                        password_confirmation.set(value);
                        confirmation_error.set(None);
                    },
                }
                if let Some(message) = confirmation_message {
                    p { class: "register-field-error", role: "alert", "{message}" }
                }
                label { class: "register-drive-option",
                    input {
                        r#type: "checkbox",
                        checked: wants_to_drive(),
                        disabled: is_loading(),
                        onchange: move |event| wants_to_drive.set(event.checked()),
                    }
                    span { "Tambien quiero trabajar como conductor" }
                }
                if wants_to_drive() {
                    label { r#for: "register-license-number", "Numero de documento" }
                    input {
                        id: "register-license-number",
                        r#type: "text",
                        autocomplete: "off",
                        disabled: is_loading(),
                        value: "{license_number}",
                        oninput: move |event| license_number.set(event.value()),
                    }
                    p { class: "register-hint",
                        "Despues de crear la cuenta te pediremos tu vehiculo y tus documentos."
                    }
                    if let Some(message) = &license_number_error {
                        p { class: "register-field-error", role: "alert", "{message}" }
                    }
                }
                button { r#type: "submit", disabled: is_loading(),
                    if is_loading() {
                        "Creando cuenta..."
                    } else {
                        "Crear cuenta"
                    }
                }
            }
            if let Some(message) = general_message {
                p { class: "register-error", role: "alert", "{message}" }
            }
            button {
                r#type: "button",
                class: "register-login-link",
                onclick: move |_| props.on_login_click.call(()),
                "Ya tengo cuenta, iniciar sesion"
            }
        }
    }
}
