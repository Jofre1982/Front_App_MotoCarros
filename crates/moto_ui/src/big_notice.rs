//! Avisos grandes y llamativos (pedido del dueno: "el mensaje de inicio tu
//! viaje y el viaje a terminado esta muy perdido... deberia un mensaje algo
//! mas llamativo").
//!
//! - `StatusBanner`: tarjeta de color con emoji grande que dice en que va el
//!   viaje o el mandado; reemplaza la linea de texto suelta.
//! - `BigNotice`: ventana que tapa la pantalla para los momentos clave
//!   (empezo, termino, rechazaron el precio). Lleva sus botones adentro.

use dioxus::prelude::*;

/// Color del aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Esperando algo (amarillo).
    Wait,
    /// En marcha (azul).
    Go,
    /// Listo / bien (verde).
    Done,
    /// Algo salio mal o se cancelo (rojo).
    Bad,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Wait => "tone-wait",
            Tone::Go => "tone-go",
            Tone::Done => "tone-done",
            Tone::Bad => "tone-bad",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct StatusBannerProps {
    pub tone: Tone,
    #[props(into)]
    pub emoji: String,
    #[props(into)]
    pub title: String,
    #[props(into, default)]
    pub detail: Option<String>,
}

#[component]
pub fn StatusBanner(props: StatusBannerProps) -> Element {
    rsx! {
        div { class: "status-banner {props.tone.class()}", role: "status",
            span { class: "status-banner-emoji", "{props.emoji}" }
            div {
                p { class: "status-banner-title", "{props.title}" }
                if let Some(detail) = &props.detail {
                    p { class: "status-banner-detail", "{detail}" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BigNoticeProps {
    pub tone: Tone,
    #[props(into)]
    pub emoji: String,
    #[props(into)]
    pub title: String,
    #[props(into, default)]
    pub detail: Option<String>,
    /// Botones (y lo que haga falta) dentro de la ventana.
    pub children: Element,
}

#[component]
pub fn BigNotice(props: BigNoticeProps) -> Element {
    rsx! {
        div { class: "big-notice-backdrop",
            div { class: "big-notice {props.tone.class()}", role: "alertdialog",
                div { class: "big-notice-emoji", "{props.emoji}" }
                p { class: "big-notice-title", "{props.title}" }
                if let Some(detail) = &props.detail {
                    p { class: "big-notice-detail", "{detail}" }
                }
                div { class: "big-notice-actions", {props.children} }
            }
        }
    }
}
