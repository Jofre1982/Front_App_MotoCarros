//! Pantalla de login (pasajero y conductor) — issue #1.
//!
//! El rol no se elige aca: viaja de vuelta en `user.role` dentro de la
//! respuesta de `POST /api/v1/auth/login`, y es quien use este resultado
//! (fuera de alcance de este issue) quien decide que UI mostrar despues.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

#[component]
pub fn LoginScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let email_value = email();
        let password_value = password();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            error_message.set(None);

            match api_client.login(&email_value, &password_value).await {
                Ok(authenticated) => {
                    session.authenticate(authenticated, storage.as_ref());
                }
                Err(err) => {
                    error_message.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    };

    rsx! {
        div { class: "login-screen",
            h1 { "Iniciar sesion" }
            form { onsubmit: on_submit,
                label { r#for: "login-email", "Email" }
                input {
                    id: "login-email",
                    r#type: "email",
                    autocomplete: "username",
                    disabled: is_loading(),
                    value: "{email}",
                    oninput: move |event| email.set(event.value()),
                }
                label { r#for: "login-password", "Contrasena" }
                input {
                    id: "login-password",
                    r#type: "password",
                    autocomplete: "current-password",
                    disabled: is_loading(),
                    value: "{password}",
                    oninput: move |event| password.set(event.value()),
                }
                button { r#type: "submit", disabled: is_loading(),
                    if is_loading() {
                        "Ingresando..."
                    } else {
                        "Ingresar"
                    }
                }
            }
            if let Some(message) = error_message() {
                p { class: "login-error", role: "alert", "{message}" }
            }
        }
    }
}
