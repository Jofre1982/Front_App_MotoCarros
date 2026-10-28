//! Aviso para el conductor de que hay servicios esperando, en cualquier
//! pantalla del modo conductor (pedido probando el APK: si el conductor
//! estaba en su perfil no se enteraba de nada).
//!
//! Consulta `GET /rides/available` y `GET /errands` cada
//! `ALERT_INTERVAL` y muestra un aviso con boton para ir a la lista. Solo
//! funciona con la app abierta: avisar con la app cerrada necesita
//! notificaciones push (Firebase), que todavia no estan conectadas.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::ApiClient;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

const ALERT_INTERVAL: Duration = Duration::from_secs(12);

/// Texto del aviso segun cuantos viajes y mandados esperan, o `None` si no
/// hay nada. Separado para poder probarlo sin Dioxus.
pub fn alert_text(rides: usize, errands: usize) -> Option<String> {
    let plural = |count: usize, one: &str, many: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {many}")
        }
    };
    match (rides, errands) {
        (0, 0) => None,
        (r, 0) => Some(format!(
            "🔔 Hay {} esperando conductor",
            plural(r, "viaje", "viajes")
        )),
        (0, e) => Some(format!(
            "🔔 Hay {} esperando",
            plural(e, "mandado", "mandados")
        )),
        (r, e) => Some(format!(
            "🔔 Hay {} y {} esperando",
            plural(r, "viaje", "viajes"),
            plural(e, "mandado", "mandados")
        )),
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct DriverServiceAlertProps {
    pub on_open_rides: EventHandler<()>,
    pub on_open_errands: EventHandler<()>,
}

#[component]
pub fn DriverServiceAlert(props: DriverServiceAlertProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut counts = use_signal(|| (0usize, 0usize));
    let mut started = use_signal(|| false);

    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            while let Some(token) = session.token() {
                let rides = match api_client.available_rides(&token).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        fetch.data.len()
                    }
                    Err(_) => counts.peek().0,
                };
                let token = session.token().unwrap_or(token);
                // `GET /errands` ya devuelve vacio si el conductor no esta
                // disponible para mandados, asi que no hay que filtrar aca.
                let errands = match api_client.list_errands(&token).await {
                    Ok(fetch) => fetch.data.len(),
                    Err(_) => counts.peek().1,
                };
                if *counts.peek() != (rides, errands) {
                    counts.set((rides, errands));
                }
                Delay::new(ALERT_INTERVAL).await;
            }
        });
    });

    let (rides, errands) = counts();
    let Some(text) = alert_text(rides, errands) else {
        return rsx! {};
    };

    rsx! {
        div { class: "driver-service-alert", role: "alert",
            p { "{text}" }
            div { class: "driver-service-alert-actions",
                if rides > 0 {
                    button {
                        r#type: "button",
                        onclick: move |_| props.on_open_rides.call(()),
                        "Ver viajes"
                    }
                }
                if errands > 0 {
                    button {
                        r#type: "button",
                        onclick: move |_| props.on_open_errands.call(()),
                        "Ver mandados"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::alert_text;

    #[test]
    fn sin_servicios_no_hay_aviso() {
        assert_eq!(alert_text(0, 0), None);
    }

    #[test]
    fn el_texto_cuenta_viajes_y_mandados() {
        assert_eq!(
            alert_text(1, 0).as_deref(),
            Some("🔔 Hay 1 viaje esperando conductor")
        );
        assert_eq!(
            alert_text(0, 3).as_deref(),
            Some("🔔 Hay 3 mandados esperando")
        );
        assert_eq!(
            alert_text(2, 1).as_deref(),
            Some("🔔 Hay 2 viajes y 1 mandado esperando")
        );
    }
}
