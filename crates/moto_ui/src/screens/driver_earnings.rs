//! Pantalla de historial de viajes y ganancias (conductor) — historia #29.
//!
//! Dos secciones independientes, cada una con su propio fetch:
//!
//! - Historial: al entrar, consulta `GET /api/v1/me/rides`
//!   (`ApiClient::ride_history`) — el mismo endpoint que usa
//!   `RideHistoryScreen` para el pasajero, pero acá lista los viajes que le
//!   asignaron al conductor (lo decide `ShowRideHistoryController` en el
//!   backend según el rol de la cuenta). No repite la fila "Conductor: ..."
//!   de `RideHistoryScreen`: el conductor ya sabe que el viaje es suyo, y el
//!   backend no expone el nombre del pasajero (`RideResource` no lo publica).
//! - Ganancias: el conductor elige un rango de fechas y consulta
//!   `GET /api/v1/me/earnings` (`ApiClient::driver_earnings`) bajo demanda —
//!   sin rango elegido no hay nada que consultar, así que no hay fetch
//!   automático al entrar (a diferencia del historial).
//!
//! Un conductor sin viajes completados en el rango elegido recibe
//! `total_earned`/`completed_rides` en cero desde el backend, no un error —
//! esta pantalla lo distingue con un estado vacío explícito en vez de
//! mostrar "0 COP" sin contexto (criterio de aceptación de la #29).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError, GetDriverEarningsError};
use moto_core::models::{DriverEarningsSummary, Ride};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use super::ride_history::ride_status_label;

#[component]
pub fn DriverEarningsScreen() -> Element {
    rsx! {
        div { class: "driver-earnings-screen",
            h2 { "Historial de viajes y ganancias" }
            DriverRideHistory {}
            DriverEarningsSummaryForm {}
        }
    }
}

#[component]
fn DriverRideHistory() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut rides = use_signal(Vec::<Ride>::new);
    // Misma guarda que `RideHistoryScreen`: el efecto lee `session.token()`
    // en su primera corrida, lo que lo suscribe a ese signal, y el fetch
    // puede terminar escribiendo en `session` (refresh de token, logout) —
    // sin esta bandera reprogramaria el efecto en un loop.
    let mut has_fetched = use_signal(|| false);

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
        section { class: "driver-ride-history",
            h3 { "Viajes asignados" }
            if is_loading() {
                p { "Cargando..." }
            } else if let Some(message) = load_error() {
                p { class: "driver-ride-history-error", role: "alert", "{message}" }
            } else if rides().is_empty() {
                p { class: "driver-ride-history-empty", "Todavia no tienes viajes asignados." }
            } else {
                ul { class: "driver-ride-history-list",
                    for ride in rides() {
                        DriverRideHistoryRow { key: "{ride.id}", ride }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DriverRideHistoryRowProps {
    ride: Ride,
}

#[component]
fn DriverRideHistoryRow(props: DriverRideHistoryRowProps) -> Element {
    let ride = &props.ride;
    let fare = ride.final_fare.unwrap_or(ride.estimated_fare);

    rsx! {
        li { class: "driver-ride-history-row",
            p { class: "driver-ride-history-status", "{ride_status_label(ride.status)}" }
            p { "Fecha: {ride.requested_at}" }
            p { "Origen: {ride.origin.latitude}, {ride.origin.longitude}" }
            p { "Destino: {ride.destination.latitude}, {ride.destination.longitude}" }
            p { "Tarifa: {ride.currency} {fare}" }
        }
    }
}

#[component]
fn DriverEarningsSummaryForm() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut from = use_signal(String::new);
    let mut to = use_signal(String::new);
    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<GetDriverEarningsError>);
    let mut summary = use_signal(|| None::<DriverEarningsSummary>);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let Some(token) = session.token() else {
            return;
        };
        let from_value = from();
        let to_value = to();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            load_error.set(None);
            summary.set(None);

            match api_client
                .driver_earnings(&token, &from_value, &to_value)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    summary.set(Some(fetch.data));
                }
                Err(GetDriverEarningsError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    load_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    };

    let can_submit = !from().trim().is_empty() && !to().trim().is_empty() && !is_loading();

    rsx! {
        section { class: "driver-earnings-summary",
            h3 { "Resumen de ganancias" }
            form { class: "driver-earnings-form", onsubmit: on_submit,
                label { r#for: "driver-earnings-from", "Desde" }
                input {
                    id: "driver-earnings-from",
                    r#type: "date",
                    disabled: is_loading(),
                    value: "{from}",
                    oninput: move |event| from.set(event.value()),
                }
                label { r#for: "driver-earnings-to", "Hasta" }
                input {
                    id: "driver-earnings-to",
                    r#type: "date",
                    disabled: is_loading(),
                    value: "{to}",
                    oninput: move |event| to.set(event.value()),
                }
                button { r#type: "submit", disabled: !can_submit,
                    if is_loading() {
                        "Consultando..."
                    } else {
                        "Ver ganancias"
                    }
                }
            }
            if let Some(message) = load_error().as_ref().map(|err| err.to_string()) {
                p { class: "driver-earnings-error", role: "alert", "{message}" }
            } else if let Some(current) = summary() {
                if current.completed_rides == 0 {
                    p { class: "driver-earnings-empty",
                        "No tienes viajes completados entre {current.from} y {current.to}."
                    }
                } else {
                    dl { class: "driver-earnings-result",
                        dt { "Total ganado" }
                        dd { "{current.currency} {current.total_earned}" }
                        dt { "Viajes completados" }
                        dd { "{current.completed_rides}" }
                    }
                }
            }
        }
    }
}
