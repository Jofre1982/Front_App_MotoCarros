//! Pantalla de tarifa estimada (pasajero) — issue #13.
//!
//! Consume `POST /api/v1/rides/estimate` a traves de
//! `ApiClient::estimate_ride`. No solicita el viaje ni persiste nada: es solo
//! una consulta para que el pasajero decida si continua. Para pedir el viaje
//! de verdad esta `POST /rides` (historia #15), que vuelve a calcular estos
//! mismos numeros.
//!
//! Origen y destino se eligen tocando el mapa (`MapView::on_click`, ver
//! `moto_ui/src/map.rs`), no con campos de texto: la historia depende
//! explicitamente del componente de mapa (issue #4).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, EstimateRideError};
use moto_core::models::{Coordinates, RideEstimate};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::map::{MapMarker, MapView};

/// Centro inicial del mapa mientras no exista geolocalizacion del
/// dispositivo (fuera de alcance de este issue): Bogota, el mismo punto que
/// ya usan los ejemplos de `openapi.yaml`.
const DEFAULT_CENTER_LAT: f64 = 4.710989;
const DEFAULT_CENTER_LNG: f64 = -74.072092;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PickTarget {
    Origin,
    Destination,
}

#[component]
pub fn RideEstimateScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut origin = use_signal(|| None::<(f64, f64)>);
    let mut destination = use_signal(|| None::<(f64, f64)>);
    let mut pick_target = use_signal(|| PickTarget::Origin);
    let mut estimate_error = use_signal(|| None::<EstimateRideError>);
    let mut estimate = use_signal(|| None::<RideEstimate>);
    let mut is_loading = use_signal(|| false);

    let on_map_click = move |(lat, lng): (f64, f64)| {
        estimate.set(None);
        estimate_error.set(None);

        match pick_target() {
            PickTarget::Origin => {
                origin.set(Some((lat, lng)));
                pick_target.set(PickTarget::Destination);
            }
            PickTarget::Destination => {
                destination.set(Some((lat, lng)));
            }
        }
    };

    let on_estimate_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some((origin_lat, origin_lng)) = origin() else {
            return;
        };
        let Some((destination_lat, destination_lng)) = destination() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            estimate_error.set(None);

            let origin_coords = Coordinates {
                latitude: origin_lat,
                longitude: origin_lng,
            };
            let destination_coords = Coordinates {
                latitude: destination_lat,
                longitude: destination_lng,
            };

            match api_client
                .estimate_ride(&token, origin_coords, destination_coords)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    estimate.set(Some(fetch.data));
                }
                Err(EstimateRideError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    estimate_error.set(Some(err));
                }
            }

            is_loading.set(false);
        });
    };

    let markers: Vec<MapMarker> = [
        origin().map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Origen".to_string()),
        }),
        destination().map(|(lat, lng)| MapMarker {
            lat,
            lng,
            label: Some("Destino".to_string()),
        }),
    ]
    .into_iter()
    .flatten()
    .collect();

    let can_estimate = origin().is_some() && destination().is_some();

    let instructions = match pick_target() {
        PickTarget::Origin => "Toca el mapa para elegir el origen.",
        PickTarget::Destination => "Toca el mapa para elegir el destino.",
    };

    rsx! {
        div { class: "ride-estimate-screen",
            h2 { "Ver tarifa estimada" }
            p { class: "ride-estimate-instructions", "{instructions}" }
            div { class: "ride-estimate-target-buttons",
                button {
                    r#type: "button",
                    disabled: pick_target() == PickTarget::Origin,
                    onclick: move |_| pick_target.set(PickTarget::Origin),
                    "Elegir origen"
                }
                button {
                    r#type: "button",
                    disabled: pick_target() == PickTarget::Destination,
                    onclick: move |_| pick_target.set(PickTarget::Destination),
                    "Elegir destino"
                }
            }
            div { class: "ride-estimate-map", style: "height: 320px;",
                MapView {
                    center_lat: DEFAULT_CENTER_LAT,
                    center_lng: DEFAULT_CENTER_LNG,
                    markers,
                    on_click: on_map_click,
                }
            }
            button {
                r#type: "button",
                class: "ride-estimate-button",
                disabled: !can_estimate || is_loading(),
                onclick: on_estimate_click,
                if is_loading() {
                    "Calculando..."
                } else {
                    "Ver tarifa estimada"
                }
            }
            if let Some(err) = estimate_error() {
                p { class: "ride-estimate-error", role: "alert", "{err}" }
            } else if let Some(value) = estimate() {
                dl { class: "ride-estimate-result",
                    dt { "Distancia" }
                    dd { "{value.distance_meters} m" }
                    dt { "Duracion" }
                    dd { "{value.duration_seconds / 60} min" }
                    dt { "Tarifa estimada" }
                    dd { "{value.currency} {value.estimated_fare}" }
                }
            } else if !can_estimate {
                p { class: "ride-estimate-empty",
                    "Elige un origen y un destino en el mapa para ver la tarifa."
                }
            }
        }
    }
}
