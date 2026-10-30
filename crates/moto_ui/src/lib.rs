use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::{Role, VehicleType};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

pub mod avatar;
pub mod big_notice;
pub mod driver_alert;
pub mod fields;
pub mod file_picker;
pub mod map;
pub mod price_offer;
pub mod reoffer;
pub mod screens;
mod theme;

pub use map::{MapMarker, MapView};
pub use screens::documents::DocumentsScreen;
pub use screens::driver_earnings::DriverEarningsScreen;
pub use screens::forgot_password::ForgotPasswordScreen;
pub use screens::login::LoginScreen;
pub use screens::nearby_errands::NearbyErrandsScreen;
pub use screens::nearby_rides::NearbyRidesScreen;
pub use screens::profile::ProfileScreen;
pub use screens::register::RegisterScreen;
pub use screens::register_vehicle::RegisterVehicleScreen;
pub use screens::request_errand::RequestErrandScreen;
pub use screens::ride_estimate::RideEstimateScreen;
pub use screens::ride_history::RideHistoryScreen;
pub use screens::vehicle::VehicleScreen;

/// Pantallas del flujo de autenticacion, previas a tener sesion iniciada.
#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthScreen {
    Login,
    Register,
    ForgotPassword,
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
        document::Style { "{theme::STYLES}" }
        if session.is_authenticated() {
            Home {}
        } else {
            match auth_screen() {
                AuthScreen::Login => rsx! {
                    LoginScreen {
                        on_register_click: move |_| auth_screen.set(AuthScreen::Register),
                        on_forgot_password_click: move |_| auth_screen.set(AuthScreen::ForgotPassword),
                    }
                },
                AuthScreen::Register => rsx! {
                    RegisterScreen {
                        on_login_click: move |_| auth_screen.set(AuthScreen::Login),
                    }
                },
                AuthScreen::ForgotPassword => rsx! {
                    ForgotPasswordScreen {
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
    RequestErrand,
    Vehicle,
    Documents,
    NearbyRides,
    NearbyErrands,
    RideHistory,
    DriverEarnings,
}

/// Pantalla principal tras iniciar sesion: perfil (issue #9), tarifa
/// estimada (issue #13), mas el logout (issue #8), que no depende de ninguna
/// seccion — cerrar sesion debe estar disponible desde el momento en que hay
/// una sesion iniciada.
///
/// Las secciones de vehiculo (issues #11 y #12, `VehicleScreen`), de
/// documentos de verificacion (issue #60, `DocumentsScreen`), de
/// solicitudes cercanas (issue #16, `NearbyRidesScreen`), de mandados
/// cercanos (issue #78, `NearbyErrandsScreen`) y de historial y
/// ganancias (historia #29, `DriverEarningsScreen`) solo se ofrecen cuando
/// `session.user()` ya cargo (`GET /api/v1/me`, issue #9) y es una cuenta de
/// conductor: un pasajero nunca ve esos botones, y estructuralmente no puede
/// llegar a esas pantallas desde esta navegacion. El historial de viajes
/// simple (issue #28, `RideHistoryScreen`) y pedir un mandado (issue #76,
/// `RequestErrandScreen`) son al reves: solo se ofrecen a pasajero — el
/// equivalente de historial para conductor no reutiliza `RideHistoryScreen`
/// porque tambien necesita el resumen de ganancias, asi que vive en
/// `DriverEarningsScreen`; el equivalente de mandados para conductor es
/// `NearbyErrandsScreen` (issue #78), un flujo completamente distinto
/// (encontrar y aceptar, no pedir).
#[component]
fn Home() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut is_logging_out = use_signal(|| false);
    // Con que rol esta usando la app (issue #95): una cuenta con ambos roles
    // lo elige al entrar y lo cambia desde el menu (`SessionState::set_mode`).
    let mode = session.active_mode();
    let is_driver = mode == Some(Role::Driver);
    let has_both_modes = session.user().is_some_and(|user| user.has_both_modes());
    // `None` = la seccion de inicio del modo actual. Se vuelve a `None` al
    // cambiar de modo, para no quedar en una pantalla del otro rol. El
    // pasajero arranca en "Pedir motocarro"; el conductor en su perfil (ahi
    // ve si le falta vehiculo o documentos).
    let mut chosen_section = use_signal(|| None::<HomeSection>);
    let section = move || {
        chosen_section().unwrap_or(if is_driver {
            HomeSection::Profile
        } else {
            HomeSection::RideEstimate
        })
    };
    let mut go_to = move |target: HomeSection| chosen_section.set(Some(target));

    // Al reabrir la app solo se recupera el token (`SessionState::hydrate`);
    // sin la cuenta no se sabe que menu mostrar, asi que se pide aca en vez
    // de depender de que la persona abra su perfil.
    let mut has_fetched_user = use_signal(|| false);
    let api_client_for_me = api_client.clone();
    let storage_for_me = storage.clone();
    use_effect(move || {
        if has_fetched_user() || session.user().is_some() {
            return;
        }
        let Some(token) = session.token() else {
            return;
        };
        has_fetched_user.set(true);
        let api_client = api_client_for_me.clone();
        let storage = storage_for_me.clone();
        spawn(async move {
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
                // Cualquier otro error: el perfil lo vuelve a intentar y lo
                // muestra; aca no hay donde mostrarlo.
                Err(_) => {}
            }
        });
    });

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

    let mut switch_mode = move |target: Role| {
        session.set_mode(target);
        chosen_section.set(None);
    };

    if session.user().is_none() {
        return rsx! {
            div {
                h1 { "MotoYa" }
                p { class: "home-loading", "Cargando tu cuenta..." }
                button {
                    r#type: "button",
                    disabled: is_logging_out(),
                    onclick: on_logout,
                    "Cerrar sesion"
                }
            }
        };
    }

    if session.needs_mode_choice() {
        return rsx! {
            div { class: "mode-chooser-screen",
                h1 { "MotoYa" }
                h2 { "¿Como quieres entrar?" }
                p { class: "mode-chooser-hint",
                    "Tu cuenta es de pasajero y de conductor. Puedes cambiar de modo cuando quieras desde el menu."
                }
                button {
                    r#type: "button",
                    class: "mode-chooser-option",
                    onclick: move |_| switch_mode(Role::Passenger),
                    strong { "Pasajero" }
                    small { "Pedir un motocarro o un mandado." }
                }
                button {
                    r#type: "button",
                    class: "mode-chooser-option",
                    onclick: move |_| switch_mode(Role::Driver),
                    strong { "Conductor" }
                    small { "Recibir solicitudes de viaje y mandados." }
                }
                button {
                    r#type: "button",
                    class: "mode-chooser-logout-link",
                    disabled: is_logging_out(),
                    onclick: on_logout,
                    "Cerrar sesion"
                }
            }
        };
    }

    rsx! {
        div {
            h1 { "MotoYa" }
            if has_both_modes {
                p { class: "home-mode-banner",
                    if is_driver { "Estas en modo conductor" } else { "Estas en modo pasajero" }
                    button {
                        r#type: "button",
                        class: "home-mode-switch-link",
                        onclick: move |_| {
                            switch_mode(if is_driver { Role::Passenger } else { Role::Driver })
                        },
                        if is_driver { "Cambiar a modo pasajero" } else { "Cambiar a modo conductor" }
                    }
                }
            }
            nav { class: "home-nav",
                if is_driver {
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::Profile,
                        onclick: move |_| go_to(HomeSection::Profile),
                        "Mi perfil"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::Vehicle,
                        onclick: move |_| go_to(HomeSection::Vehicle),
                        "Mi vehiculo"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::Documents,
                        onclick: move |_| go_to(HomeSection::Documents),
                        "Mis documentos"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::NearbyRides,
                        onclick: move |_| go_to(HomeSection::NearbyRides),
                        "Solicitudes cercanas"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::NearbyErrands,
                        onclick: move |_| go_to(HomeSection::NearbyErrands),
                        "Mandados cercanos"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::DriverEarnings,
                        onclick: move |_| go_to(HomeSection::DriverEarnings),
                        "Historial y ganancias"
                    }
                } else {
                    // "Pedir motocarro" (estimar + solicitar un viaje) es un
                    // flujo del pasajero: el conductor no pide viajes, los
                    // recibe en "Solicitudes cercanas". El backend ya
                    // rechaza la creacion del viaje si la intenta una cuenta
                    // de conductor (`RidePolicy::create()`), pero mostrarle
                    // la pestana igual invitaba a un 403 confuso en vez de
                    // que la opcion ni aparezca.
                    //
                    // Va primero y se llama por lo que el pasajero quiere
                    // hacer ("pedir un motocarro"), no por el primer paso
                    // del flujo ("ver tarifa estimada"): el estimado sigue
                    // estando adentro de la pantalla.
                    button {
                        r#type: "button",
                        class: "home-nav-primary",
                        disabled: section() == HomeSection::RideEstimate,
                        onclick: move |_| go_to(HomeSection::RideEstimate),
                        "Pedir motocarro"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::RequestErrand,
                        onclick: move |_| go_to(HomeSection::RequestErrand),
                        // El mandado quedo adentro de "Pedir motocarro"
                        // (Viaje / Mandado); aca va la motocarga, para que el
                        // sistema sepa que vehiculo hace falta.
                        "Pedir motocarga"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::RideHistory,
                        onclick: move |_| go_to(HomeSection::RideHistory),
                        "Mis viajes"
                    }
                    button {
                        r#type: "button",
                        disabled: section() == HomeSection::Profile,
                        onclick: move |_| go_to(HomeSection::Profile),
                        "Mi perfil"
                    }
                }
            }
            if is_driver && section() != HomeSection::NearbyRides
                && section() != HomeSection::NearbyErrands
            {
                driver_alert::DriverServiceAlert {
                    on_open_rides: move |_| go_to(HomeSection::NearbyRides),
                    on_open_errands: move |_| go_to(HomeSection::NearbyErrands),
                }
            }
            match section() {
                HomeSection::Profile => rsx! {
                    ProfileScreen {}
                },
                HomeSection::RideEstimate => rsx! {
                    RideEstimateScreen {}
                },
                HomeSection::RequestErrand => rsx! {
                    RequestErrandScreen { vehicle_type: VehicleType::Motocarga }
                },
                HomeSection::Vehicle => rsx! {
                    VehicleScreen {}
                },
                HomeSection::Documents => rsx! {
                    DocumentsScreen {}
                },
                HomeSection::NearbyRides => rsx! {
                    NearbyRidesScreen {}
                },
                HomeSection::NearbyErrands => rsx! {
                    NearbyErrandsScreen {}
                },
                HomeSection::RideHistory => rsx! {
                    RideHistoryScreen {}
                },
                HomeSection::DriverEarnings => rsx! {
                    DriverEarningsScreen {}
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
