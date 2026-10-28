//! Pantalla de recuperación de contraseña. El código llega al correo de la
//! cuenta (backend #104: no hay proveedor de SMS contratado).
//!
//! Dos pasos en la misma pantalla, con navegación manual por un enum de
//! paso, mismo patrón que el resto de la app (ver `.claude/STANDARDS.md`):
//! pedir el código con el correo (`POST /auth/password/forgot`), después
//! confirmarlo junto con la contraseña nueva (`POST /auth/password/reset`).
//! Si el código es correcto, la respuesta ya deja la sesión iniciada —mismo
//! criterio que login y los registros—, así que no hace falta volver a la
//! pantalla de login para entrar con la contraseña recién elegida.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, ConfirmPasswordResetError, RequestPasswordResetError};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::fields::{PasswordInput, password_confirmation_error};

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
    let mut email = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut password_confirmation = use_signal(String::new);
    let mut confirmation_error = use_signal(|| None::<&'static str>);
    let mut is_loading = use_signal(|| false);
    let mut request_error = use_signal(|| None::<RequestPasswordResetError>);
    let mut confirm_error = use_signal(|| None::<ConfirmPasswordResetError>);

    let request_api_client = api_client.clone();

    let on_request_submit = move |event: FormEvent| {
        event.prevent_default();

        let email_value = email();
        let api_client = request_api_client.clone();

        spawn(async move {
            is_loading.set(true);
            request_error.set(None);

            match api_client.request_password_reset(&email_value).await {
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

        let email_value = email();
        let code_value = code();
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
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            confirm_error.set(None);

            match api_client
                .confirm_password_reset(&email_value, &code_value, &password_value)
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
            let email_error = current_error
                .as_ref()
                .and_then(|err| err.field_message("email"));
            // Igual que en el resto de los formularios (issue #6): el
            // mensaje generico solo se muestra cuando el error no trae
            // desglose por campo.
            let general_message = if email_error.is_some() {
                None
            } else {
                current_error.as_ref().map(|err| err.to_string())
            };

            rsx! {
                div { class: "forgot-password-screen",
                    h1 { "Recuperar mi contrasena" }
                    p {
                        "Escribe el correo con el que te registraste. Si tiene una cuenta, te llega un código a ese correo."
                    }
                    form { onsubmit: on_request_submit,
                        label { r#for: "forgot-password-email", "Correo" }
                        input {
                            id: "forgot-password-email",
                            r#type: "email",
                            autocomplete: "username",
                            autocapitalize: "off",
                            disabled: is_loading(),
                            value: "{email}",
                            oninput: move |event| email.set(event.value()),
                        }
                        if let Some(message) = &email_error {
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
            let confirmation_message =
                password_confirmation_error(&password(), &password_confirmation())
                    .or(confirmation_error());
            let general_message = if code_error.is_some() || password_error.is_some() {
                None
            } else {
                current_error.as_ref().map(|err| err.to_string())
            };

            rsx! {
                div { class: "forgot-password-screen",
                    h1 { "Ingresa el código" }
                    p { "Revisa tu correo (mira tambien en Spam o Correo no deseado)." }
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
                        PasswordInput {
                            id: "forgot-password-new-password",
                            value: password(),
                            autocomplete: "new-password",
                            disabled: is_loading(),
                            oninput: move |value| password.set(value),
                        }
                        if let Some(message) = &password_error {
                            p { class: "forgot-password-field-error", role: "alert", "{message}" }
                        }
                        label { r#for: "forgot-password-new-password-confirmation", "Repite la contrasena nueva" }
                        PasswordInput {
                            id: "forgot-password-new-password-confirmation",
                            value: password_confirmation(),
                            autocomplete: "new-password",
                            disabled: is_loading(),
                            oninput: move |value| {
                                password_confirmation.set(value);
                                confirmation_error.set(None);
                            },
                        }
                        if let Some(message) = confirmation_message {
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
