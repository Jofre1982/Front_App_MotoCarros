//! Conductores favoritos del pasajero (backend #118; pedido del dueno: "hay
//! gente que se casa con ciertos conductores").
//!
//! La lista se guarda en un contexto compartido por todas las pantallas del
//! pasajero: el historial la usa para la estrella de cada conductor, "Pedir
//! motocarro" para ofrecer "pedirle primero a mi favorito" y "Mi perfil"
//! para quitarlos.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError, ServiceActionError};
use moto_core::models::FavoriteDriver;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::avatar::Avatar;

/// `None` mientras no se ha cargado.
#[derive(Clone, Copy)]
pub struct Favorites {
    pub list: Signal<Option<Vec<FavoriteDriver>>>,
}

/// En `Home`: crea la lista compartida.
pub fn provide_favorites() {
    use_context_provider(|| Favorites {
        list: Signal::new(None),
    });
}

/// La lista compartida; la pide al servidor la primera vez.
pub fn use_favorites() -> Favorites {
    let favorites = try_use_context::<Favorites>().unwrap_or_else(|| Favorites {
        list: use_signal(|| None),
    });
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut list = favorites.list;

    use_effect(move || {
        if list.peek().is_some() {
            return;
        }
        let Some(token) = session.token() else {
            return;
        };
        // Marca "cargando" con una lista vacia para no pedirla dos veces.
        list.set(Some(Vec::new()));
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            match api_client.favorite_drivers(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    list.set(Some(fetch.data));
                }
                Err(AuthenticatedRequestError::SessionExpired) => session.logout(storage.as_ref()),
                Err(_) => list.set(None),
            }
        });
    });

    favorites
}

pub fn is_favorite(list: &Option<Vec<FavoriteDriver>>, driver_id: u64) -> bool {
    list.as_ref()
        .is_some_and(|drivers| drivers.iter().any(|d| d.id == driver_id))
}

#[derive(Props, Clone, PartialEq)]
pub struct FavoriteToggleProps {
    pub driver_id: u64,
}

/// "⭐ Guardar como favorito" / "★ Es tu favorito · Quitar".
#[component]
pub fn FavoriteToggle(props: FavoriteToggleProps) -> Element {
    let favorites = use_favorites();
    let mut list = favorites.list;
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut is_saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let driver_id = props.driver_id;
    let saved = is_favorite(&list(), driver_id);

    let toggle = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_saving.set(true);
            error.set(None);
            let result = if saved {
                api_client
                    .remove_favorite_driver(&token, driver_id)
                    .await
                    .map(|()| None)
            } else {
                api_client
                    .add_favorite_driver(&token, driver_id)
                    .await
                    .map(|fetch| {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        Some(fetch.data)
                    })
            };
            match result {
                Ok(added) => list.with_mut(|current| {
                    let drivers = current.get_or_insert_with(Vec::new);
                    drivers.retain(|d| d.id != driver_id);
                    drivers.extend(added);
                }),
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => error.set(Some(err.to_string())),
            }
            is_saving.set(false);
        });
    };

    rsx! {
        button {
            r#type: "button",
            class: if saved { "favorite-toggle favorite-toggle-on" } else { "favorite-toggle" },
            disabled: is_saving(),
            onclick: toggle,
            if saved { "★ Es tu favorito · Quitar" } else { "⭐ Guardar como favorito" }
        }
        if let Some(message) = error() {
            p { class: "profile-error", role: "alert", "{message}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct FavoritePickerProps {
    /// El favorito elegido, o `None` = pedir a todos.
    pub selected: Signal<Option<u64>>,
}

/// En "Pedir motocarro": pedirle primero a un favorito (opcional).
#[component]
pub fn FavoritePicker(props: FavoritePickerProps) -> Element {
    let favorites = use_favorites();
    let mut selected = props.selected;
    let drivers = (favorites.list)().unwrap_or_default();
    if drivers.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "favorite-picker",
            p { class: "ride-step-title", "⭐ ¿Se lo pides primero a tu favorito? " span { class: "optional-tag", "(opcional)" } }
            label { class: "favorite-option",
                input {
                    r#type: "radio",
                    name: "favorite-driver",
                    checked: selected().is_none(),
                    onchange: move |_| selected.set(None),
                }
                span { "Pedir a todos los conductores" }
            }
            for driver in drivers {
                label {
                    key: "{driver.id}",
                    class: if driver.receiving_rides { "favorite-option" } else { "favorite-option favorite-option-off" },
                    input {
                        r#type: "radio",
                        name: "favorite-driver",
                        disabled: !driver.receiving_rides,
                        checked: selected() == Some(driver.id),
                        onchange: move |_| selected.set(Some(driver.id)),
                    }
                    Avatar { url: driver.avatar_url.clone(), name: driver.name.clone(), size: 36 }
                    span {
                        strong { "{driver.name}" }
                        if driver.receiving_rides {
                            small { " · 🟢 Recibiendo viajes" }
                        } else {
                            small { " · ⚪ No disponible ahora" }
                        }
                    }
                }
            }
            if selected().is_some() {
                p { class: "favorite-picker-note",
                    "Solo a él le llegará tu viaje durante 2 minutos. Si no lo toma, le avisamos a todos."
                }
            }
        }
    }
}

/// En "Mi perfil" del pasajero: la lista con opcion de quitar.
#[component]
pub fn FavoriteDriversSection() -> Element {
    let favorites = use_favorites();
    let drivers = (favorites.list)().unwrap_or_default();

    rsx! {
        div { class: "favorite-drivers-section",
            h3 { "⭐ Mis conductores favoritos" }
            if drivers.is_empty() {
                p { class: "ride-history-empty",
                    "Todavia no tienes. Guarda a un conductor desde \"Mis viajes\" con la estrella."
                }
            } else {
                ul { class: "favorite-drivers-list",
                    for driver in drivers {
                        li { key: "{driver.id}", class: "favorite-driver-row",
                            Avatar { url: driver.avatar_url.clone(), name: driver.name.clone(), size: 40 }
                            div {
                                p { strong { "{driver.name}" } }
                                if let Some(vehicle) = &driver.vehicle {
                                    p { "🛺 {vehicle.vehicle_type.label()} · Placa {vehicle.plate}" }
                                }
                                FavoriteToggle { driver_id: driver.id }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_who_is_a_favorite() {
        let list = Some(vec![FavoriteDriver {
            id: 7,
            name: "Jofre".into(),
            avatar_url: None,
            vehicle: None,
            receiving_rides: true,
        }]);
        assert!(is_favorite(&list, 7));
        assert!(!is_favorite(&list, 8));
        assert!(!is_favorite(&None, 7));
    }
}
