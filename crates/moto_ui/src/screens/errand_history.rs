//! Historial de mandados y motocargas (pedido del dueno: "Mis viajes" y
//! "Historial y ganancias" no mostraban los valores de los mandados).
//! Lo usan el pasajero (`Role::Passenger`, los que pidio) y el conductor
//! (`Role::Driver`, los que hizo), con `GET /me/errands` (backend #106).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::{DriverVehicle, Errand, ErrandStatus, RideDriver, Role, format_cop};

use crate::avatar::Avatar;
use crate::favorites::FavoriteToggle;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

/// Colombia no cambia la hora en el ano: siempre UTC-5.
const COLOMBIA_OFFSET_MINUTES: i64 = -5 * 60;

/// `2026-10-29T15:04:05+00:00` → `29/10/2026 10:04` (hora de Colombia: el
/// servidor manda la hora en UTC). Sin hora: solo la fecha. Si no tiene esa
/// forma se devuelve tal cual.
pub(crate) fn short_date(iso: &str) -> String {
    let (date, rest) = iso.split_once('T').unwrap_or((iso, ""));
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next())
    else {
        return iso.to_string();
    };
    if rest.len() < 5 {
        return format!("{d:02}/{m:02}/{y}");
    }
    let (Ok(hour), Ok(minute)) = (rest[0..2].parse::<i64>(), rest[3..5].parse::<i64>()) else {
        return format!("{d:02}/{m:02}/{y}");
    };
    let utc_minutes = (days_from_civil(y, m, d) * 24 + hour) * 60 + minute - offset_minutes(rest);
    let local = utc_minutes + COLOMBIA_OFFSET_MINUTES;
    let (ly, lm, ld) = civil_from_days(local.div_euclid(24 * 60));
    let minute_of_day = local.rem_euclid(24 * 60);
    format!(
        "{ld:02}/{lm:02}/{ly} {:02}:{:02}",
        minute_of_day / 60,
        minute_of_day % 60
    )
}

/// Desfase del texto (`+00:00`, `-05:00`, `Z`), en minutos.
fn offset_minutes(rest: &str) -> i64 {
    let Some(pos) = rest.rfind(['+', '-']) else {
        return 0;
    };
    let sign = if rest[pos..].starts_with('-') { -1 } else { 1 };
    let digits: String = rest[pos + 1..]
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    if digits.len() < 4 {
        return 0;
    }
    let hours = digits[0..2].parse::<i64>().unwrap_or(0);
    let minutes = digits[2..4].parse::<i64>().unwrap_or(0);
    sign * (hours * 60 + minutes)
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
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
                            if errand.awaiting_price_response() {
                                "💬 Te ofrecen un precio: respóndelo"
                            } else {
                                "{errand_status_text(errand.status)}"
                            }
                        }
                        p { "Fecha: {short_date(&errand.requested_at)}" }
                        p { "{errand.description}" }
                        if let Some(destination) = errand.destination_label() {
                            p { "Destino: {destination}" }
                        }
                        if let Some(price) = errand.agreed_price {
                            p { class: "ride-history-price", "Valor: {format_cop(price)}" }
                        }
                        ServiceParties {
                            role,
                            driver: errand.driver.clone(),
                            vehicle: errand.driver_vehicle.clone(),
                            passenger_name: errand.passenger_name.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub(crate) struct ServicePartiesProps {
    pub role: Role,
    pub driver: Option<RideDriver>,
    /// La placa con la que se hizo; si no vino, la actual del conductor.
    pub vehicle: Option<DriverVehicle>,
    pub passenger_name: Option<String>,
}

/// Quien hizo el servicio y con que vehiculo (pedido del dueno: "en caso de
/// alguna eventualidad"). Al pasajero: foto, nombre, tipo y placa. Al
/// conductor: el nombre del pasajero y la placa que uso.
#[component]
pub(crate) fn ServiceParties(props: ServicePartiesProps) -> Element {
    let vehicle = props
        .vehicle
        .clone()
        .or_else(|| props.driver.as_ref().and_then(|d| d.vehicle.clone()));
    rsx! {
        div { class: "service-parties",
            if props.role == Role::Passenger {
                if let Some(driver) = &props.driver {
                    div { class: "service-parties-driver",
                        Avatar { url: driver.avatar_url.clone(), name: driver.name.clone(), size: 40 }
                        p { strong { "Conductor: " } "{driver.name}" }
                    }
                    FavoriteToggle { driver_id: driver.id }
                }
            } else if let Some(name) = &props.passenger_name {
                p { strong { "Pasajero: " } "{name}" }
            }
            if let Some(vehicle) = &vehicle {
                p { class: "service-parties-vehicle",
                    "🛺 {vehicle.vehicle_type.label()} · Placa "
                    strong { "{vehicle.plate}" }
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
        assert_eq!(short_date("2026-10-29T15:04:05+00:00"), "29/10/2026 10:04");
        assert_eq!(short_date("2026-11-02T15:34:00Z"), "02/11/2026 10:34");
        // Pasada la medianoche UTC todavia es el dia anterior en Colombia.
        assert_eq!(short_date("2026-01-01T02:30:00+00:00"), "31/12/2025 21:30");
        assert_eq!(short_date("2026-10-29T10:04:00-05:00"), "29/10/2026 10:04");
        assert_eq!(short_date("2026-10-29"), "29/10/2026");
        assert_eq!(short_date("ayer"), "ayer");
    }

    #[test]
    fn cancelled_errands_say_so() {
        assert_eq!(errand_status_text(ErrandStatus::Cancelled), "Cancelado");
    }
}
