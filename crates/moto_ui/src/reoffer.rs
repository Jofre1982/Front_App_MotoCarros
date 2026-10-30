//! Recuadro "Ofrecer otro precio" dentro de la ventana de precio rechazado
//! (pedido del dueno: antes se mostraba la fila entera con el mapa y la
//! casilla de precio quedaba escondida abajo). Letra grande para quien ve poco.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{AcceptErrandError, AcceptRideError, ApiClient};
use moto_core::models::{Errand, Ride, format_cop};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

/// Que se vuelve a ofertar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReofferTarget {
    Ride(u64),
    Errand(u64),
}

/// Lo que devuelve el servidor al aceptar con el precio nuevo.
#[derive(Debug, Clone, PartialEq)]
pub enum Reoffered {
    Ride(Ride),
    Errand(Errand),
}

#[derive(Props, Clone, PartialEq)]
pub struct ReofferBoxProps {
    pub target: ReofferTarget,
    pub on_sent: EventHandler<Reoffered>,
    /// Otro conductor lo tomo mientras tanto.
    pub on_unavailable: EventHandler<()>,
}

#[component]
pub fn ReofferBox(props: ReofferBoxProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut price = use_signal(String::new);
    let mut is_sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let target = props.target;
    let on_sent = props.on_sent;
    let on_unavailable = props.on_unavailable;

    let parsed = price().trim().parse::<i64>().ok().filter(|p| *p >= 1);

    let send = move |_| {
        let (Some(token), Some(amount)) = (session.token(), parsed) else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_sending.set(true);
            error.set(None);
            let outcome = match target {
                ReofferTarget::Ride(id) => {
                    match api_client.accept_ride(&token, id, Some(amount)).await {
                        Ok(fetch) => Ok((fetch.refreshed_token, Reoffered::Ride(fetch.data))),
                        Err(AcceptRideError::SessionExpired) => Err(None),
                        Err(AcceptRideError::Conflict) => Err(Some(None)),
                        Err(err) => Err(Some(Some(err.to_string()))),
                    }
                }
                ReofferTarget::Errand(id) => {
                    match api_client.accept_errand(&token, id, amount).await {
                        Ok(fetch) => Ok((fetch.refreshed_token, Reoffered::Errand(fetch.data))),
                        Err(AcceptErrandError::SessionExpired) => Err(None),
                        Err(AcceptErrandError::Conflict) => Err(Some(None)),
                        Err(err) => Err(Some(Some(err.to_string()))),
                    }
                }
            };
            match outcome {
                Ok((refreshed, sent)) => {
                    if let Some(refreshed) = refreshed {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_sent.call(sent);
                }
                Err(None) => session.logout(storage.as_ref()),
                Err(Some(None)) => on_unavailable.call(()),
                Err(Some(Some(message))) => error.set(Some(message)),
            }
            is_sending.set(false);
        });
    };

    rsx! {
        div { class: "reoffer-box",
            label { class: "reoffer-label", r#for: "reoffer-price", "👇 Oferta de nuevo aquí" }
            input {
                id: "reoffer-price",
                class: "reoffer-input",
                r#type: "number",
                inputmode: "numeric",
                min: "1",
                placeholder: "Escribe el precio",
                disabled: is_sending(),
                value: "{price}",
                oninput: move |event| price.set(event.value()),
            }
            if let Some(amount) = parsed {
                p { class: "reoffer-preview", "{format_cop(amount)}" }
            }
            button {
                r#type: "button",
                class: "big-notice-button",
                disabled: is_sending() || parsed.is_none(),
                onclick: send,
                if is_sending() { "Enviando..." } else { "Enviar nuevo precio" }
            }
            if let Some(message) = error() {
                p { class: "reoffer-error", role: "alert", "{message}" }
            }
        }
    }
}
