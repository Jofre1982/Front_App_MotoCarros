//! Ventana "Te ofrecen un precio" para el pasajero, en cualquier pantalla
//! (reportado probando: el conductor quedaba "Esperando que el pasajero
//! acepte tu precio" y el pasajero no sabia donde aceptarlo, porque la
//! oferta solo salia en la pantalla del pedido).
//!
//! Consulta cada pocos segundos si algun viaje o mandado suyo espera su
//! respuesta y la muestra grande, con Aceptar / Rechazar ahi mismo.

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;
use moto_core::api::{ApiClient, ServiceActionError};
use moto_core::models::{DriverVehicle, Errand, Ride, Role};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::big_notice::{BigNotice, Tone};
use crate::price_offer::PriceOfferCard;

const CHECK_EVERY: Duration = Duration::from_secs(8);

/// Una oferta pendiente, de viaje o de mandado.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingOffer {
    pub kind: OfferKind,
    pub driver_name: String,
    pub driver_avatar_url: Option<String>,
    pub vehicle: Option<DriverVehicle>,
    pub price: i64,
    pub service: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OfferKind {
    Ride(u64),
    Errand(u64),
}

/// La primera oferta que espera respuesta: viajes primero.
pub fn pending_offer(rides: &[Ride], errands: &[Errand]) -> Option<PendingOffer> {
    let ride = rides
        .iter()
        .find(|ride| ride.awaiting_price_response())
        .and_then(|ride| {
            let driver = ride.driver.as_ref()?;
            Some(PendingOffer {
                kind: OfferKind::Ride(ride.id),
                driver_name: driver.name.clone(),
                driver_avatar_url: driver.avatar_url.clone(),
                vehicle: driver.vehicle.clone(),
                price: ride.agreed_price?,
                service: "viaje",
            })
        });
    ride.or_else(|| {
        errands
            .iter()
            .find(|errand| errand.awaiting_price_response())
            .and_then(|errand| {
                let driver = errand.driver.as_ref()?;
                Some(PendingOffer {
                    kind: OfferKind::Errand(errand.id),
                    driver_name: driver.name.clone(),
                    driver_avatar_url: driver.avatar_url.clone(),
                    vehicle: driver.vehicle.clone(),
                    price: errand.agreed_price?,
                    service: if errand.is_motocarga() {
                        "motocarga"
                    } else {
                        "mandado"
                    },
                })
            })
    })
}

#[component]
pub fn PassengerOfferAlert() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut offer = use_signal(|| None::<PendingOffer>);
    let mut is_responding = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut started = use_signal(|| false);

    let poll_api = api_client.clone();
    let poll_storage = storage.clone();
    use_effect(move || {
        if started() {
            return;
        }
        started.set(true);
        let api_client = poll_api.clone();
        let storage = poll_storage.clone();
        spawn(async move {
            while let Some(token) = session.token() {
                let rides = match api_client.ride_history(&token, Some(Role::Passenger)).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        Some(fetch.data)
                    }
                    Err(_) => None,
                };
                let token = session.token().unwrap_or(token);
                let errands = match api_client.my_errands(&token, Role::Passenger).await {
                    Ok(fetch) => {
                        if let Some(refreshed) = fetch.refreshed_token {
                            session.update_token(refreshed, storage.as_ref());
                        }
                        Some(fetch.data)
                    }
                    Err(_) => None,
                };
                // Si una consulta fallo (sin senal), se deja lo que habia.
                if let (Some(rides), Some(errands)) = (rides, errands) {
                    let found = pending_offer(&rides, &errands);
                    if *offer.peek() != found && !*is_responding.peek() {
                        offer.set(found);
                    }
                }
                Delay::new(CHECK_EVERY).await;
            }
        });
    });

    let respond = use_callback(move |accept: bool| {
        let (Some(current), Some(token)) = (offer(), session.token()) else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_responding.set(true);
            error.set(None);
            let result = match current.kind {
                OfferKind::Ride(id) => api_client
                    .respond_to_ride_price(&token, id, accept)
                    .await
                    .map(|fetch| fetch.refreshed_token),
                OfferKind::Errand(id) => api_client
                    .respond_to_errand_price(&token, id, accept)
                    .await
                    .map(|fetch| fetch.refreshed_token),
            };
            match result {
                Ok(refreshed) => {
                    if let Some(refreshed) = refreshed {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    offer.set(None);
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => error.set(Some(err.to_string())),
            }
            is_responding.set(false);
        });
    });

    let Some(current) = offer() else {
        return rsx! {};
    };

    rsx! {
        BigNotice {
            tone: Tone::Wait,
            emoji: "💬",
            title: "¡Te ofrecen un precio!",
            detail: format!("Para tu {}. Acéptalo o recházalo aquí:", current.service),
            PriceOfferCard {
                driver_name: current.driver_name.clone(),
                driver_avatar_url: current.driver_avatar_url.clone(),
                vehicle: current.vehicle.clone(),
                price: current.price,
                is_responding: is_responding(),
                error: error(),
                on_accept: move |_| respond.call(true),
                on_reject: move |_| respond.call(false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errand(id: u64, price_confirmed: bool) -> Errand {
        serde_json::from_value(serde_json::json!({
            "id": id, "status": "accepted", "description": "Unos cordones",
            "has_photo": false, "photo_url": null, "completed_at": null,
            "origin": {"latitude": 3.86, "longitude": -67.92},
            "destination": null, "destination_text": "Calle 26",
            "agreed_price": 5000, "price_confirmed": price_confirmed,
            "driver": {"id": 9, "name": "Jofre Sanchez"},
            "requested_at": "2026-11-02T15:34:00+00:00"
        }))
        .unwrap()
    }

    #[test]
    fn an_errand_waiting_for_the_passenger_is_the_pending_offer() {
        let offer = pending_offer(&[], &[errand(4, false)]).unwrap();
        assert_eq!(offer.kind, OfferKind::Errand(4));
        assert_eq!(offer.price, 5000);
        assert_eq!(offer.driver_name, "Jofre Sanchez");
        assert_eq!(offer.service, "mandado");
    }

    #[test]
    fn an_answered_offer_is_not_pending() {
        assert_eq!(pending_offer(&[], &[errand(4, true)]), None);
    }
}
