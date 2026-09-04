//! Pantalla de recibo del viaje completado (pasajero) — historia #25.
//!
//! Se abre desde `RideHistoryScreen` con el "Ver recibo" de un viaje
//! `completed`: consulta `GET /api/v1/rides/{ride}/receipt`
//! (`ApiClient::ride_receipt`) bajo demanda, mismo criterio que
//! `DriverEarningsSummaryForm` (sin fetch automatico al entrar, porque
//! depende de un `ride_id` que solo se conoce al abrir esta pantalla). Un
//! viaje completado pero sin pago procesado todavia responde 422
//! (`ShowRideReceiptController` en el backend) — ese caso se distingue del
//! recibo con un mensaje explicito, nunca mostrando un recibo vacio o roto
//! (criterio de aceptacion de la #25).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, GetReceiptError};
use moto_core::models::RideReceipt;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

#[derive(Props, Clone, PartialEq)]
pub struct RideReceiptScreenProps {
    pub ride_id: u64,
    /// Se dispara cuando el pasajero pide volver al historial.
    pub on_close: EventHandler<()>,
}

#[component]
pub fn RideReceiptScreen(props: RideReceiptScreenProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<GetReceiptError>);
    let mut receipt = use_signal(|| None::<RideReceipt>);
    // Misma guarda que `RideHistoryScreen`: el efecto lee `session.token()`
    // en su primera corrida, lo que lo suscribe a ese signal, y el fetch
    // puede terminar escribiendo en `session` (refresh de token, logout) —
    // sin esta bandera reprogramaria el efecto en un loop.
    let mut has_fetched = use_signal(|| false);

    let ride_id = props.ride_id;

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

            match api_client.ride_receipt(&token, ride_id).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    receipt.set(Some(fetch.data));
                }
                Err(GetReceiptError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    load_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    });

    rsx! {
        div { class: "ride-receipt-screen",
            h3 { "Recibo del viaje" }
            if is_loading() {
                p { "Cargando..." }
            } else if matches!(load_error(), Some(GetReceiptError::Validation(_))) {
                p { class: "ride-receipt-unavailable",
                    "El recibo todavia no esta disponible para este viaje."
                }
            } else if let Some(message) = load_error().as_ref().map(|err| err.to_string()) {
                p { class: "ride-receipt-error", role: "alert", "{message}" }
            } else if let Some(current) = receipt() {
                dl { class: "ride-receipt-detail",
                    dt { "Tarifa base" }
                    dd { "{current.currency} {current.base_fare}" }
                    dt { "Distancia" }
                    dd { "{current.currency} {current.distance_fare}" }
                    dt { "Tiempo" }
                    dd { "{current.currency} {current.time_fare}" }
                    dt { "Espera" }
                    dd { "{current.currency} {current.waiting_fee}" }
                    dt { "Total" }
                    dd { "{current.currency} {current.total}" }
                    dt { "Fecha" }
                    dd { "{current.completed_at}" }
                }
            }
            button {
                r#type: "button",
                onclick: move |_| props.on_close.call(()),
                "Volver"
            }
        }
    }
}
