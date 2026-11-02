//! Registra el token push de este celular en el servidor apenas hay sesion
//! (backend #113), para que lleguen las notificaciones con la app cerrada.
//!
//! Firebase puede tardar unos segundos en dar el token la primera vez, asi
//! que se reintenta un rato. Sin fuente de tokens (web) no hace nada.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{ApiClient, ServiceActionError};
use moto_core::push::PushTokenSource;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

const RETRY_EVERY: Duration = Duration::from_secs(6);
const MAX_TRIES: u32 = 20;

#[component]
pub fn PushRegistration() -> Element {
    let source = try_use_context::<Arc<dyn PushTokenSource>>();
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut started = use_signal(|| false);

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);
        let Some(source) = source.clone() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            for _ in 0..MAX_TRIES {
                if let (Some(device_token), Some(token)) = (source.token(), session.token()) {
                    match api_client
                        .register_device_token(&token, &device_token, source.platform())
                        .await
                    {
                        Ok(fetch) => {
                            if let Some(refreshed) = fetch.refreshed_token {
                                session.update_token(refreshed, storage.as_ref());
                            }
                            return;
                        }
                        Err(ServiceActionError::SessionExpired) => return,
                        // Red caida: se reintenta en la siguiente vuelta.
                        Err(_) => {}
                    }
                }
                Delay::new(RETRY_EVERY).await;
            }
        });
    });

    rsx! {}
}
