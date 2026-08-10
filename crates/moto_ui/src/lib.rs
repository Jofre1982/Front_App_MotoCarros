use dioxus::prelude::*;
use moto_core::state::SessionState;

pub mod screens;

pub use screens::login::LoginScreen;

/// Raiz de la UI, agnostica de plataforma.
///
/// Espera un `moto_core::api::ApiClient` ya inyectado en el contexto por el
/// binario de plataforma (`web`/`mobile`), con la URL del backend que decida
/// cada uno — nunca hardcodeada aca (ver `.claude/STANDARDS.md`).
#[component]
pub fn App() -> Element {
    let session = use_context_provider(SessionState::new);

    rsx! {
        if session.is_authenticated() {
            Home {}
        } else {
            LoginScreen {}
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
