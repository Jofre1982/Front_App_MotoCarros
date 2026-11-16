//! Anuncios de negocios locales (backend: `GET /ads`). El pasajero los ve
//! mientras espera su motocarro, uno a la vez, cambiando cada 8 segundos.
//! Si no hay anuncios no se muestra nada.
//!
//! Cada anuncio cuenta una vista la primera vez que aparece en esta espera y
//! un toque cuando abren su WhatsApp: son los numeros que el dueño le
//! muestra al negocio.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::ApiClient;
use moto_core::models::{Ad, AdEvent, whatsapp_url};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::avatar::use_protected_image;

/// Cada cuanto cambia el anuncio.
const ROTATE_EVERY: Duration = Duration::from_secs(8);

/// El anuncio que toca mostrar en el turno `tick`.
pub fn ad_at(count: usize, tick: usize) -> Option<usize> {
    (count > 0).then(|| tick % count)
}

#[component]
pub fn AdCarousel() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut ads = use_signal(Vec::<Ad>::new);
    let mut tick = use_signal(|| 0usize);
    let mut seen = use_signal(HashSet::<u64>::new);

    let load_api = api_client.clone();
    use_hook(move || {
        let api_client = load_api.clone();
        spawn(async move {
            let Some(token) = session.token() else {
                return;
            };
            if let Ok(fetch) = api_client.active_ads(&token).await {
                if let Some(refreshed) = fetch.refreshed_token {
                    session.update_token(refreshed, storage.as_ref());
                }
                ads.set(fetch.data);
            }
            loop {
                Delay::new(ROTATE_EVERY).await;
                if ads.peek().len() > 1 {
                    tick += 1;
                }
            }
        });
    });

    let list = ads();
    let Some(index) = ad_at(list.len(), tick()) else {
        return rsx! {};
    };
    let ad = list[index].clone();

    // Una vista por anuncio en esta espera, no una cada 8 segundos.
    if !seen.peek().contains(&ad.id) {
        seen.write().insert(ad.id);
        if let Some(token) = session.token() {
            let api_client = api_client.clone();
            let id = ad.id;
            spawn(async move {
                let _ = api_client.record_ad(&token, id, AdEvent::View).await;
            });
        }
    }

    let click_api = api_client.clone();
    let ad_id = ad.id;
    let on_whatsapp = move |_| {
        if let Some(token) = session.token() {
            let api_client = click_api.clone();
            spawn(async move {
                let _ = api_client.record_ad(&token, ad_id, AdEvent::Click).await;
            });
        }
    };

    rsx! {
        div { class: "ad-card", key: "{ad.id}",
            span { class: "ad-label", "Publicidad" }
            AdImage { url: ad.image_url.clone(), name: ad.business_name.clone() }
            div { class: "ad-body",
                p { class: "ad-business", "{ad.business_name}" }
                p { class: "ad-message", "{ad.message}" }
                if let Some(phone) = ad.whatsapp.as_ref() {
                    a {
                        class: "ad-whatsapp",
                        href: "{whatsapp_url(phone)}",
                        target: "_blank",
                        rel: "noopener",
                        onclick: on_whatsapp,
                        "💬 Escribir por WhatsApp"
                    }
                }
            }
            if list.len() > 1 {
                div { class: "ad-dots",
                    for i in 0..list.len() {
                        span { key: "{i}", class: if i == index { "ad-dot ad-dot-on" } else { "ad-dot" } }
                    }
                }
            }
        }
    }
}

#[component]
fn AdImage(url: Option<String>, name: String) -> Element {
    let image = use_protected_image(url);
    rsx! {
        if let Some(src) = image() {
            img { class: "ad-image", src: "{src}", alt: "{name}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rota_por_todos_los_anuncios_y_vuelve_al_primero() {
        assert_eq!(ad_at(0, 5), None);
        assert_eq!(ad_at(3, 0), Some(0));
        assert_eq!(ad_at(3, 2), Some(2));
        assert_eq!(ad_at(3, 3), Some(0));
    }
}
