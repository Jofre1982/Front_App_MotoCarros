use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

pub mod map;
pub mod screens;

pub use map::{MapMarker, MapView};
pub use screens::login::LoginScreen;
pub use screens::register_passenger::RegisterPassengerScreen;

/// Pantallas del flujo de autenticacion, previas a tener sesion iniciada.
#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthScreen {
    Login,
    RegisterPassenger,
}

/// Raiz de la UI, agnostica de plataforma.
///
/// Espera un `moto_core::api::ApiClient` y un
/// `Arc<dyn moto_core::storage::TokenStorage>` ya inyectados en el contexto
/// por el binario de plataforma (`web`/`mobile`), con la URL del backend y
/// el mecanismo de storage que decida cada uno — nunca hardcodeados aca (ver
/// `.claude/STANDARDS.md`).
#[component]
pub fn App() -> Element {
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context_provider(SessionState::new);
    let mut auth_screen = use_signal(|| AuthScreen::Login);

    // Restaura la sesion persistida (si hay una) al arrancar la app —
    // issue #3. Corre una sola vez: el closure no lee ningun signal, solo
    // escribe el token, asi que Dioxus no lo vuelve a disparar despues del
    // primer render.
    use_effect(move || {
        session.hydrate(storage.as_ref());
    });

    rsx! {
        if session.is_authenticated() {
            Home {}
        } else {
            match auth_screen() {
                AuthScreen::Login => rsx! {
                    LoginScreen {
                        on_register_click: move |_| auth_screen.set(AuthScreen::RegisterPassenger),
                    }
                },
                AuthScreen::RegisterPassenger => rsx! {
                    RegisterPassengerScreen {
                        on_login_click: move |_| auth_screen.set(AuthScreen::Login),
                    }
                },
            }
        }
    }
}

#[component]
fn Home() -> Element {
    rsx! {
        div {
            h1 { "MotoYa" }
            p { "Sesion iniciada." }
        }
    }
}
