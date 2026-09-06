//! Pantalla de recuperación de contraseña (recuperar contraseña por SMS,
//! historia técnica del backend).
//!
//! Dos pasos en la misma pantalla, con navegación manual por un enum de
//! paso, mismo patrón que el resto de la app (ver `.claude/STANDARDS.md`):
//! pedir el código con el celular (`POST /auth/password/forgot`), después
//! confirmarlo junto con la contraseña nueva (`POST /auth/password/reset`).
//! Si el código es correcto, la respuesta ya deja la sesión iniciada —mismo
//! criterio que login y los registros—, así que no hace falta volver a la
//! pantalla de login para entrar con la contraseña recién elegida.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, ConfirmPasswordResetError, RequestPasswordResetError};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    RequestCode,
    ConfirmCode,
}

#[derive(Props, Clone, PartialEq)]
pub struct ForgotPasswordScreenProps {
    /// Se dispara cuando el usuario pide volver al login (ya sea porque
    /// recordó su contraseña, o para no seguir esperando un código).
    pub on_login_click: EventHandler<()>,
}

#[component]
pub fn ForgotPasswordScreen(props: ForgotPasswordScreenProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut step = use_signal(|| Step::RequestCode);
    let mut phone = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut is_loading = use_signal(|| false);
    let mut request_error = use_signal(|| None::<RequestPasswordResetError>);
    let mut confirm_error = use_signal(|| None::<ConfirmPasswordResetError>);

    let request_api_client = api_client.clone();

    let on_request_submit = move |event: FormEvent| {
        event.prevent_default();

        let phone_value = phone();
        let api_client = request_api_client.clone();

        spawn(async move {
            is_loading.set(true);
            request_error.set(None);

            match api_client.request_password_reset(&phone_value).await {
                Ok(()) => {
                    step.set(Step::ConfirmCode);
                }
                Err(err) => {
                    request_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    };

    let on_confirm_submit = move |event: FormEvent| {
        event.prevent_default();

        let phone_value = phone();
        let code_value = code();
        let password_value = password();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            confirm_error.set(None);

            match api_client
                .confirm_password_reset(&phone_value, &code_value, &password_value)
                .await
            {
                Ok(authenticated) => {
                    session.authenticate(authenticated, storage.as_ref());
                }
                Err(err) => {
                    confirm_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    };

    match step() {
        Step::RequestCode => {
            let current_error = request_error();
            let phone_error = current_error
                .as_ref()
                .and_then(|err| err.field_message("phone"));
            // Igual que en el resto de los formularios (issue #6): el
            // mensaje generico solo se muestra cuando el error no trae
            // desglose por campo.
            let general_message = if phone_error.is_some() {
                None
            } else {
                current_error.as_ref().map(|err| err.to_string())
            };

            rsx! {
                div { class: "forgot-password-screen",
                    h1 { "Recuperar mi contrasena" }
                    p {
                        "Ingresa el celular con el que te registraste. Si tiene una cuenta, te llega un código por SMS."
                    }
                    form { onsubmit: on_request_submit,
                        label { r#for: "forgot-password-phone", "Celular" }
                        input {
                            id: "forgot-password-phone",
                            r#type: "tel",
                            autocomplete: "tel",
                            disabled: is_loading(),
                            value: "{phone}",
                            oninput: move |event| phone.set(event.value()),
                        }
                        if let Some(message) = &phone_error {
                            p { class: "forgot-password-field-error", role: "alert", "{message}" }
                        }
                        button { r#type: "submit", disabled: is_loading(),
                            if is_loading() {
                                "Enviando..."
                            } else {
                                "Enviar código"
                            }
                        }
                    }
                    if let Some(message) = general_message {
                        p { class: "forgot-password-error", role: "alert", "{message}" }
                    }
                    button {
                        r#type: "button",
                        class: "forgot-password-login-link",
                        onclick: move |_| props.on_login_click.call(()),
                        "Ya tengo mi contrasena, iniciar sesion"
                    }
                }
            }
        }
        Step::ConfirmCode => {
            let current_error = confirm_error();
            let code_error = current_error
                .as_ref()
                .and_then(|err| err.field_message("code"));
            let password_error = current_error
                .as_ref()
                .and_then(|err| err.field_message("password"));
            let general_message = if code_error.is_some() || password_error.is_some() {
                None
            } else {
                current_error.as_ref().map(|err| err.to_string())
            };

            rsx! {
                div { class: "forgot-password-screen",
                    h1 { "Ingresa el código" }
                    p { "Revisa los mensajes de texto de tu celular." }
                    form { onsubmit: on_confirm_submit,
                        label { r#for: "forgot-password-code", "Código" }
                        input {
                            id: "forgot-password-code",
                            r#type: "text",
                            inputmode: "numeric",
                            autocomplete: "one-time-code",
                            disabled: is_loading(),
                            value: "{code}",
                            oninput: move |event| code.set(event.value()),
                        }
                        if let Some(message) = &code_error {
                            p { class: "forgot-password-field-error", role: "alert", "{message}" }
                        }
                        label { r#for: "forgot-password-new-password", "Contrasena nueva" }
                        input {
                            id: "forgot-password-new-password",
                            r#type: "password",
                            autocomplete: "new-password",
                            disabled: is_loading(),
                            value: "{password}",
                            oninput: move |event| password.set(event.value()),
                        }
                        if let Some(message) = &password_error {
                            p { class: "forgot-password-field-error", role: "alert", "{message}" }
                        }
                        button { r#type: "submit", disabled: is_loading(),
                            if is_loading() {
                                "Confirmando..."
                            } else {
                                "Cambiar contrasena"
                            }
                        }
                    }
                    if let Some(message) = general_message {
                        p { class: "forgot-password-error", role: "alert", "{message}" }
                    }
                    button {
                        r#type: "button",
                        class: "forgot-password-retry-link",
                        onclick: move |_| step.set(Step::RequestCode),
                        "No me llegó el código, volver a pedirlo"
                    }
                }
            }
        }
    }
}
