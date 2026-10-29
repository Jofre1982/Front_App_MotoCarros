//! Historial de mandados y motocargas (pedido del dueno: "Mis viajes" y
//! "Historial y ganancias" no mostraban los valores de los mandados).
//! Lo usan el pasajero (`Role::Passenger`, los que pidio) y el conductor
//! (`Role::Driver`, los que hizo), con `GET /me/errands` (backend #106).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::{Errand, ErrandStatus, Role, format_cop};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

/// `2026-10-29T15:04:05+00:00` → `29/10/2026 15:04`. Si no tiene esa forma
/// se devuelve tal cual.
pub(crate) fn short_date(iso: &str) -> String {
    let (date, rest) = iso.split_once('T').unwrap_or((iso, ""));
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(m), Some(d)) if y.len() == 4 => {
            let time: String = rest.chars().take(5).collect();
            if time.len() == 5 {
                format!("{d}/{m}/{y} {time}")
            } else {
                format!("{d}/{m}/{y}")
            }
        }
        _ => iso.to_string(),
    }
}

pub(crate) fn errand_status_text(status: ErrandStatus) -> &'static str {
    match status {
        ErrandStatus::Requested => "Esperando conductor",
        ErrandStatus::Accepted => "En curso",
        ErrandStatus::Completed => "Completado",
        ErrandStatus::Cancelled => "Cancelado",
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ErrandHistoryListProps {
    pub role: Role,
}

#[component]
pub fn ErrandHistoryList(props: ErrandHistoryListProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut errands = use_signal(Vec::<Errand>::new);
    let mut load_error = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| true);
    let mut started = use_signal(|| false);
    let role = props.role;

    use_effect(move || {
        if started() {
            return;
        }
        let Some(token) = session.token() else {
            return;
        };
        started.set(true);
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            match api_client.my_errands(&token, role).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    errands.set(fetch.data);
                }
                Err(AuthenticatedRequestError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => load_error.set(Some(err.to_string())),
            }
            is_loading.set(false);
        });
    });

    rsx! {
        if is_loading() {
            p { "Cargando..." }
        } else if let Some(message) = load_error() {
            p { class: "ride-history-error", role: "alert", "{message}" }
        } else if errands().is_empty() {
            p { class: "ride-history-empty", "Todavia no tienes mandados ni motocargas." }
        } else {
            ul { class: "ride-history-list",
                for errand in errands() {
                    li { key: "{errand.id}", class: "ride-history-row",
                        p { class: "ride-history-status",
                            if errand.is_motocarga() { "📦 Motocarga · " } else { "🛍️ Mandado · " }
                            "{errand_status_text(errand.status)}"
                        }
                        p { "Fecha: {short_date(&errand.requested_at)}" }
                        p { "{errand.description}" }
                        if let Some(destination) = errand.destination_label() {
                            p { "Destino: {destination}" }
                        }
                        if let Some(price) = errand.agreed_price {
                            p { class: "ride-history-price", "Valor: {format_cop(price)}" }
                        }
                        if role == Role::Passenger {
                            if let Some(driver) = &errand.driver {
                                p { "Conductor: {driver.name}" }
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
    fn short_date_reads_day_month_year_and_time() {
        assert_eq!(short_date("2026-10-29T15:04:05+00:00"), "29/10/2026 15:04");
        assert_eq!(short_date("2026-10-29"), "29/10/2026");
        assert_eq!(short_date("ayer"), "ayer");
    }

    #[test]
    fn cancelled_errands_say_so() {
        assert_eq!(errand_status_text(ErrandStatus::Cancelled), "Cancelado");
    }
}
