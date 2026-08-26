//! Pantalla de perfil (pasajero y conductor) — issue #9.
//!
//! Consume `GET /api/v1/me` a traves de `ApiClient::me`, que ya trae el
//! reintento con refresh de token (issue #3). Si el refresh tambien falla,
//! la sesion se cierra en vez de mostrar un error suelto en pantalla — es
//! justo el criterio de aceptacion del issue.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::Role;
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

    rsx! {
        div { class: "profile-screen",
            h2 { "Mi perfil" }
            if is_loading() {
                p { "Cargando perfil..." }
            } else if let Some(message) = error_message() {
                p { class: "profile-error", role: "alert", "{message}" }
            } else if let Some(user) = session.user() {
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
            }
        }
    }
}
