//! La oferta de precio del conductor, del lado del pasajero (backend #107).
//!
//! Cuando un conductor acepta un viaje o mandado propone un precio; el
//! pasajero lo acepta o lo rechaza y el pedido vuelve a quedar libre para
//! otro conductor. Componente compartido por el viaje (`RideEstimateScreen`)
//! y el mandado/motocarga (`RequestErrandScreen`).

use dioxus::prelude::*;
use moto_core::models::format_cop;

#[derive(Props, Clone, PartialEq)]
pub struct PriceOfferCardProps {
    pub driver_name: String,
    pub price: i64,
    pub is_responding: bool,
    pub error: Option<String>,
    pub on_accept: EventHandler<()>,
    pub on_reject: EventHandler<()>,
}

#[component]
pub fn PriceOfferCard(props: PriceOfferCardProps) -> Element {
    rsx! {
        div { class: "price-offer", role: "alert",
            p { class: "price-offer-title", "💬 {props.driver_name} te ofrece:" }
            p { class: "price-offer-amount", "{format_cop(props.price)}" }
            p { class: "price-offer-hint",
                "Si te parece bien, acéptalo. Si no, recházalo y le llega a otro conductor."
            }
            button {
                r#type: "button",
                class: "price-offer-accept",
                disabled: props.is_responding,
                onclick: move |_| props.on_accept.call(()),
                if props.is_responding { "Enviando..." } else { "✓ Aceptar precio" }
            }
            button {
                r#type: "button",
                class: "price-offer-reject",
                disabled: props.is_responding,
                onclick: move |_| props.on_reject.call(()),
                "✗ Rechazar y buscar otro"
            }
            if let Some(message) = &props.error {
                p { class: "price-offer-error", role: "alert", "{message}" }
            }
        }
    }
}
