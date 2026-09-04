//! Pantalla de historial de viajes (pasajero) — issue #28.
//!
//! Al entrar, consulta `GET /api/v1/me/rides` (`ApiClient::ride_history`) y
//! muestra los viajes que pidio la cuenta, del mas reciente al mas antiguo
//! (el orden ya lo decide el backend, `ShowRideHistoryController`). Una
//! cuenta sin viajes previos recibe una lista vacia, no un error, asi que
//! ese caso se distingue con un estado vacio explicito en vez de mostrar la
//! lista en blanco (criterio de aceptacion del issue).
//!
//! El equivalente para conductor (historial + resumen de ganancias) es
//! `DriverEarningsScreen` (historia #29), que reutiliza `ride_status_label`
//! de aca pero tiene su propio fetch — `Home` (`moto_ui/src/lib.rs`) solo
//! ofrece esta pestana a cuentas de pasajero.
//!
//! Cada viaje `completed` ofrece un "Ver recibo" que abre `RideReceiptScreen`
//! (historia #25) para ese `ride_id`, en vez de una pantalla aparte
//! registrada en `Home`: el recibo solo tiene sentido en el contexto de un
//! viaje de este historial, nunca como seccion propia de la navegacion.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError};
use moto_core::models::{Ride, RideStatus};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::ride_receipt::RideReceiptScreen;

#[component]
pub fn RideHistoryScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut rides = use_signal(Vec::<Ride>::new);
    // Misma guarda que `VehicleScreen`/`ProfileScreen`: el efecto lee
    // `session.token()` en su primera corrida, lo que lo suscribe a ese
    // signal, y el fetch puede terminar escribiendo en `session` (refresh de
    // token, logout) — sin esta bandera reprogramaria el efecto en un loop.
    let mut has_fetched = use_signal(|| false);
    // `Some(ride_id)` mientras se ve el recibo de ese viaje (historia #25):
    // reemplaza la lista en vez de superponerse, mismo patron de navegacion
    // manual por signal que `HomeSection` en `moto_ui/src/lib.rs`.
    let mut viewing_receipt = use_signal(|| None::<u64>);

    use_effect(move || {
        if has_fetched() {
            return;
        }

        let Some(token) = session.token() else {
            return;
        };
        has_fetched.set(true);

        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            load_error.set(None);

            match api_client.ride_history(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    rides.set(fetch.data);
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    load_error.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    });

    rsx! {
        div { class: "ride-history-screen",
            h2 { "Historial de viajes" }
            if let Some(ride_id) = viewing_receipt() {
                RideReceiptScreen {
                    ride_id,
                    on_close: move |_| viewing_receipt.set(None),
                }
            } else if is_loading() {
                p { "Cargando..." }
            } else if let Some(message) = load_error() {
                p { class: "ride-history-error", role: "alert", "{message}" }
            } else if rides().is_empty() {
                p { class: "ride-history-empty", "Todavia no tienes viajes." }
            } else {
                ul { class: "ride-history-list",
                    for ride in rides() {
                        RideHistoryRow {
                            key: "{ride.id}",
                            ride,
                            on_view_receipt: move |ride_id| viewing_receipt.set(Some(ride_id)),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RideHistoryRowProps {
    ride: Ride,
    on_view_receipt: EventHandler<u64>,
}

#[component]
fn RideHistoryRow(props: RideHistoryRowProps) -> Element {
    let ride = &props.ride;
    let fare = ride.final_fare.unwrap_or(ride.estimated_fare);
    let ride_id = ride.id;

    rsx! {
        li { class: "ride-history-row",
            p { class: "ride-history-status", "{ride_status_label(ride.status)}" }
            p { "Fecha: {ride.requested_at}" }
            p { "Origen: {ride.origin.latitude}, {ride.origin.longitude}" }
            p { "Destino: {ride.destination.latitude}, {ride.destination.longitude}" }
            p { "Tarifa: {ride.currency} {fare}" }
            if let Some(driver) = &ride.driver {
                p { "Conductor: {driver.name}" }
            }
            if ride.status == RideStatus::Completed {
                button {
                    r#type: "button",
                    onclick: move |_| props.on_view_receipt.call(ride_id),
                    "Ver recibo"
                }
            }
        }
    }
}

/// Texto del estado de un viaje del historial. Compartido con
/// `DriverEarningsScreen` (historia #29): el estado de un viaje se lee igual
/// sin importar si lo ve el pasajero o el conductor asignado.
pub(crate) fn ride_status_label(status: RideStatus) -> &'static str {
    match status {
        RideStatus::Requested => "Esperando conductor",
        RideStatus::Accepted => "Conductor asignado",
        RideStatus::InProgress => "En curso",
        RideStatus::Completed => "Completado",
        RideStatus::Cancelled => "Cancelado",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ride_status_label_describes_a_completed_ride() {
        assert_eq!(ride_status_label(RideStatus::Completed), "Completado");
    }

    #[test]
    fn ride_status_label_describes_a_cancelled_ride() {
        assert_eq!(ride_status_label(RideStatus::Cancelled), "Cancelado");
    }

    #[test]
    fn ride_status_label_describes_a_ride_still_waiting_for_a_driver() {
        assert_eq!(
            ride_status_label(RideStatus::Requested),
            "Esperando conductor"
        );
    }
}
