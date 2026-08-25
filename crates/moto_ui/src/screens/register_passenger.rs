//! Pantalla de registro de pasajero — issue #6.
//!
//! Al registrarse exitosamente la sesion queda autenticada igual que tras un
//! login (mismo shape `AuthenticatedUser`), asi que no hace falta encadenar
//! un login aparte.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

#[derive(Props, Clone, PartialEq)]
pub struct RegisterPassengerScreenProps {
    /// Se dispara cuando el usuario pide volver a la pantalla de login.
    pub on_login_click: EventHandler<()>,
}

#[component]
pub fn RegisterPassengerScreen(props: RegisterPassengerScreenProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut phone = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let name_value = name();
        let email_value = email();
        let phone_value = phone();
        let password_value = password();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            error_message.set(None);

            match api_client
                .register_passenger(&name_value, &email_value, &phone_value, &password_value)
                .await
            {
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
        div { class: "register-passenger-screen",
            h1 { "Crear cuenta de pasajero" }
            form { onsubmit: on_submit,
                label { r#for: "register-passenger-name", "Nombre" }
                input {
                    id: "register-passenger-name",
                    r#type: "text",
                    autocomplete: "name",
                    disabled: is_loading(),
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
                label { r#for: "register-passenger-email", "Email" }
                input {
                    id: "register-passenger-email",
                    r#type: "email",
                    autocomplete: "username",
                    disabled: is_loading(),
                    value: "{email}",
                    oninput: move |event| email.set(event.value()),
                }
                label { r#for: "register-passenger-phone", "Telefono" }
                input {
                    id: "register-passenger-phone",
                    r#type: "tel",
                    autocomplete: "tel",
                    disabled: is_loading(),
                    value: "{phone}",
                    oninput: move |event| phone.set(event.value()),
                }
                label { r#for: "register-passenger-password", "Contrasena" }
                input {
                    id: "register-passenger-password",
                    r#type: "password",
                    autocomplete: "new-password",
                    disabled: is_loading(),
                    value: "{password}",
                    oninput: move |event| password.set(event.value()),
                }
                button { r#type: "submit", disabled: is_loading(),
                    if is_loading() {
                        "Creando cuenta..."
                    } else {
                        "Crear cuenta"
                    }
                }
            }
            if let Some(message) = error_message() {
                p { class: "register-passenger-error", role: "alert", "{message}" }
            }
            button {
                r#type: "button",
                class: "register-passenger-login-link",
                onclick: move |_| props.on_login_click.call(()),
                "Ya tengo cuenta, iniciar sesion"
            }
        }
    }
}
