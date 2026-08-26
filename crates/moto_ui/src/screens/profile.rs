//! Pantalla de perfil (pasajero y conductor) — issue #9 — mas la edicion de
//! datos de contacto — issue #10.
//!
//! Consume `GET /api/v1/me` a traves de `ApiClient::me`, que ya trae el
//! reintento con refresh de token (issue #3). Si el refresh tambien falla,
//! la sesion se cierra en vez de mostrar un error suelto en pantalla — es
//! justo el criterio de aceptacion del issue.
//!
//! La edicion (issue #10) reusa esta misma pantalla en vez de ser una
//! pantalla aparte: el formulario se precarga con `session.user()` y, al
//! guardar, llama a `ApiClient::update_profile` (`PATCH /api/v1/me`).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError, UpdateProfileError};
use moto_core::models::{Role, UpdateProfilePayload, User};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Passenger => "Pasajero",
        Role::Driver => "Conductor",
    }
}

#[component]
pub fn ProfileScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    // Guarda contra volver a disparar el fetch: el efecto lee `session.token()`
    // en su primera corrida (para mandarlo en la request), lo que lo suscribe
    // a ese signal. Sin esta bandera, el `update_token`/`logout` que dispara
    // el propio fetch reprogramaria el efecto en un loop. Una vez que
    // `has_fetched` es `true`, las corridas siguientes retornan antes de leer
    // `session.token()` de nuevo, asi que dejan de depender de el.
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
            error_message.set(None);

            match api_client.me(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error_message.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    });

    let mut is_editing = use_signal(|| false);

    rsx! {
        div { class: "profile-screen",
            h2 { "Mi perfil" }
            if is_loading() {
                p { "Cargando perfil..." }
            } else if let Some(message) = error_message() {
                p { class: "profile-error", role: "alert", "{message}" }
            } else if let Some(user) = session.user() {
                if is_editing() {
                    EditProfileForm {
                        user,
                        on_saved: move |()| is_editing.set(false),
                        on_cancel: move |()| is_editing.set(false),
                    }
                } else {
                    dl {
                        dt { "Nombre" }
                        dd { "{user.name}" }
                        dt { "Email" }
                        dd { "{user.email}" }
                        dt { "Telefono" }
                        dd { "{user.phone}" }
                        dt { "Rol" }
                        dd { "{role_label(user.role)}" }
                    }
                    button {
                        r#type: "button",
                        class: "profile-edit-button",
                        onclick: move |_| is_editing.set(true),
                        "Editar perfil"
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EditProfileFormProps {
    user: User,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
}

/// Formulario de edicion de nombre/email/telefono — issue #10. Precargado
/// con `props.user`; al guardar manda solo los campos presentes en el
/// formulario a `ApiClient::update_profile` (PATCH parcial, no PUT — ver
/// `openapi.yaml` de `Back_App_MotoCarros`).
#[component]
fn EditProfileForm(props: EditProfileFormProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut name = use_signal(|| props.user.name.clone());
    let mut email = use_signal(|| props.user.email.clone());
    let mut phone = use_signal(|| props.user.phone.clone());
    let mut update_error = use_signal(|| None::<UpdateProfileError>);
    let mut is_saving = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let Some(token) = session.token() else {
            return;
        };
        let payload = UpdateProfilePayload {
            name: Some(name()),
            email: Some(email()),
            phone: Some(phone()),
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_saving.set(true);
            update_error.set(None);

            match api_client.update_profile(&token, payload).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                    is_saving.set(false);
                    props.on_saved.call(());
                    return;
                }
                Err(UpdateProfileError::SessionExpired) => {
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
    let name_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("name"));
    let email_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("email"));
    let phone_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("phone"));
    // Igual que en el registro (issue #6): el mensaje generico solo se
    // muestra cuando el error no trae desglose campo por campo, para no
    // repetir el mismo texto dos veces.
    let has_field_errors = name_error.is_some() || email_error.is_some() || phone_error.is_some();
    let general_message = if has_field_errors {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        form { class: "profile-edit-form", onsubmit: on_submit,
            label { r#for: "profile-edit-name", "Nombre" }
            input {
                id: "profile-edit-name",
                r#type: "text",
                autocomplete: "name",
                disabled: is_saving(),
                value: "{name}",
                oninput: move |event| name.set(event.value()),
            }
            if let Some(message) = &name_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            label { r#for: "profile-edit-email", "Email" }
            input {
                id: "profile-edit-email",
                r#type: "email",
                autocomplete: "email",
                disabled: is_saving(),
                value: "{email}",
                oninput: move |event| email.set(event.value()),
            }
            if let Some(message) = &email_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            label { r#for: "profile-edit-phone", "Telefono" }
            input {
                id: "profile-edit-phone",
                r#type: "tel",
                autocomplete: "tel",
                disabled: is_saving(),
                value: "{phone}",
                oninput: move |event| phone.set(event.value()),
            }
            if let Some(message) = &phone_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            button { r#type: "submit", disabled: is_saving(),
                if is_saving() {
                    "Guardando..."
                } else {
                    "Guardar cambios"
                }
            }
            button {
                r#type: "button",
                disabled: is_saving(),
                onclick: move |_| props.on_cancel.call(()),
                "Cancelar"
            }
        }
        if let Some(message) = general_message {
            p { class: "profile-edit-error", role: "alert", "{message}" }
        }
    }
}
