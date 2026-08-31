use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::models::Role;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

pub mod map;
pub mod screens;

pub use map::{MapMarker, MapView};
pub use screens::login::LoginScreen;
pub use screens::profile::ProfileScreen;
pub use screens::register_driver::RegisterDriverScreen;
pub use screens::register_passenger::RegisterPassengerScreen;
pub use screens::register_vehicle::RegisterVehicleScreen;
pub use screens::ride_estimate::RideEstimateScreen;
pub use screens::vehicle::VehicleScreen;

/// Pantallas del flujo de autenticacion, previas a tener sesion iniciada.
#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthScreen {
    Login,
    RegisterPassenger,
    RegisterDriver,
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
                        on_register_driver_click: move |_| auth_screen.set(AuthScreen::RegisterDriver),
                    }
                },
                AuthScreen::RegisterPassenger => rsx! {
                    RegisterPassengerScreen {
                        on_login_click: move |_| auth_screen.set(AuthScreen::Login),
                    }
                },
                AuthScreen::RegisterDriver => rsx! {
                    RegisterDriverScreen {
                        on_login_click: move |_| auth_screen.set(AuthScreen::Login),
                    }
                },
            }
        }
    }
}

/// Secciones de `Home` una vez hay sesion iniciada. No es un router: es el
/// mismo patron de navegacion manual por signal que `AuthScreen`, todavia
/// suficiente para las pocas pantallas que existen (ver
/// `.claude/STANDARDS.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum HomeSection {
    Profile,
    RideEstimate,
    Vehicle,
}

/// Pantalla principal tras iniciar sesion: perfil (issue #9), tarifa
/// estimada (issue #13), mas el logout (issue #8), que no depende de ninguna
/// seccion — cerrar sesion debe estar disponible desde el momento en que hay
/// una sesion iniciada.
///
/// La seccion de vehiculo (issues #11 y #12, `VehicleScreen`) solo se ofrece
/// cuando `session.user()` ya cargo (`GET /api/v1/me`, issue #9) y es una
/// cuenta de conductor: un pasajero nunca ve el boton, y estructuralmente no
/// puede llegar a esa pantalla desde esta navegacion.
#[component]
fn Home() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut is_logging_out = use_signal(|| false);
    let mut section = use_signal(|| HomeSection::Profile);
    let is_driver = session.user().is_some_and(|user| user.role == Role::Driver);

    let on_logout = move |_| {
        let api_client = api_client.clone();
        let storage = storage.clone();
        let Some(token) = session.token() else {
            // Sin token no hay nada que invalidar en el backend, pero la
            // sesion local igual se limpia.
            session.logout(storage.as_ref());
            return;
        };

        spawn(async move {
            is_logging_out.set(true);

            // Cerrar sesion nunca debe dejar al usuario atrapado por un
            // error de red o un token ya vencido: la sesion local se limpia
            // sin importar el resultado de la request (ver issue #8).
            let _ = api_client.logout(&token.access_token).await;
            session.logout(storage.as_ref());

            is_logging_out.set(false);
        });
    };

    rsx! {
        div {
            h1 { "MotoYa" }
            nav { class: "home-nav",
                button {
                    r#type: "button",
                    disabled: section() == HomeSection::Profile,
                    onclick: move |_| section.set(HomeSection::Profile),
                    "Mi perfil"
                }
                button {
                    r#type: "button",
                    disabled: section() == HomeSection::RideEstimate,
                    onclick: move |_| section.set(HomeSection::RideEstimate),
                    "Ver tarifa estimada"
                }
                if is_driver {
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::Vehicle,
                        onclick: move |_| section.set(HomeSection::Vehicle),
                        "Mi vehiculo"
                    }
                }
            }
            match section() {
                HomeSection::Profile => rsx! {
                    ProfileScreen {}
                },
                HomeSection::RideEstimate => rsx! {
                    RideEstimateScreen {}
                },
                HomeSection::Vehicle => rsx! {
                    VehicleScreen {}
                },
            }
            button {
                r#type: "button",
                disabled: is_logging_out(),
                onclick: on_logout,
                if is_logging_out() {
                    "Cerrando sesion..."
                } else {
                    "Cerrar sesion"
                }
            }
        }
    }
}
