//! La oferta de precio del conductor, del lado del pasajero (backend #107).
//!
//! Cuando un conductor acepta un viaje o mandado propone un precio; el
//! pasajero lo acepta o lo rechaza y el pedido vuelve a quedar libre para
//! otro conductor. Componente compartido por el viaje (`RideEstimateScreen`)
//! y el mandado/motocarga (`RequestErrandScreen`).

use dioxus::prelude::*;
use moto_core::models::{VehicleType, format_cop};

use crate::avatar::{Avatar, VehiclePhoto};

fn vehicle_label(vehicle_type: VehicleType) -> &'static str {
    match vehicle_type {
        VehicleType::Motocarro => "🛺 Motocarro",
        VehicleType::Motocarga => "📦 Motocarga",
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PriceOfferCardProps {
    pub driver_name: String,
    /// Foto aprobada del conductor, para reconocerlo antes de aceptar.
    #[props(default)]
    pub driver_avatar_url: Option<String>,
    /// Placa y tipo del vehiculo.
    #[props(default)]
    pub vehicle: Option<moto_core::models::DriverVehicle>,
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
            div { class: "price-offer-driver",
                Avatar { url: props.driver_avatar_url.clone(), name: props.driver_name.clone(), size: 72 }
                div {
                    p { class: "price-offer-title", "{props.driver_name} te ofrece:" }
                    if let Some(vehicle) = &props.vehicle {
                        p { class: "price-offer-vehicle",
                            "{vehicle_label(vehicle.vehicle_type)} · Placa "
                            strong { "{vehicle.plate}" }
                        }
                    }
                }
            }
            if let Some(vehicle) = &props.vehicle {
                VehiclePhoto { url: vehicle.photo_url.clone() }
            }
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

#[derive(Props, Clone, PartialEq)]
pub struct ContactCardProps {
    /// "Tu conductor" o "Tu pasajero".
    pub title: &'static str,
    pub counterpart: moto_core::models::Counterpart,
    /// Vehiculo del conductor, cuando quien mira es el pasajero.
    #[props(default)]
    pub vehicle: Option<moto_core::models::DriverVehicle>,
}

/// Nombre y celular de la otra persona con el boton para escribirle por
/// WhatsApp (backend #108, `counterpart`). Solo llega con el precio ya
/// confirmado.
#[component]
pub fn ContactCard(props: ContactCardProps) -> Element {
    let url = moto_core::models::whatsapp_url(&props.counterpart.phone);
    rsx! {
        div { class: "contact-card",
            div { class: "contact-card-person",
                Avatar {
                    url: props.counterpart.avatar_url.clone(),
                    name: props.counterpart.name.clone(),
                    size: 56,
                }
                div {
                    p { class: "contact-card-title", "{props.title}: {props.counterpart.name}" }
                    p { class: "contact-card-phone", "📱 {props.counterpart.phone}" }
                    if let Some(vehicle) = &props.vehicle {
                        p { class: "price-offer-vehicle",
                            "{vehicle_label(vehicle.vehicle_type)} · Placa "
                            strong { "{vehicle.plate}" }
                        }
                    }
                }
            }
            if let Some(vehicle) = &props.vehicle {
                VehiclePhoto { url: vehicle.photo_url.clone() }
            }
            a {
                class: "contact-card-whatsapp",
                href: "{url}",
                target: "_blank",
                rel: "noopener",
                "💬 Escribir por WhatsApp"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ConfirmCancelButtonProps {
    /// Texto del boton, p. ej. "Cancelar pedido" o "Soltar mandado".
    pub label: &'static str,
    /// Pregunta de confirmacion.
    pub question: &'static str,
    pub is_busy: bool,
    pub on_confirm: EventHandler<()>,
}

/// Boton de cancelar en dos pasos, para no cancelar por un toque sin querer.
#[component]
pub fn ConfirmCancelButton(props: ConfirmCancelButtonProps) -> Element {
    let mut asking = use_signal(|| false);
    rsx! {
        if asking() {
            div { class: "confirm-cancel",
                p { "{props.question}" }
                button {
                    r#type: "button",
                    class: "confirm-cancel-yes",
                    disabled: props.is_busy,
                    onclick: move |_| props.on_confirm.call(()),
                    if props.is_busy { "Cancelando..." } else { "Si, cancelar" }
                }
                button {
                    r#type: "button",
                    class: "confirm-cancel-no-link",
                    disabled: props.is_busy,
                    onclick: move |_| asking.set(false),
                    "No, volver"
                }
            }
        } else {
            button {
                r#type: "button",
                class: "confirm-cancel-start",
                onclick: move |_| asking.set(true),
                "{props.label}"
            }
        }
    }
}
