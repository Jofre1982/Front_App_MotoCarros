//! Iconos SVG propios de la identidad visual de MotoYa — issue #54.
//!
//! Vectores propios (sin fotografias), inspirados en tres referencias del
//! Guainia: los Cerros de Mavicure, la flor de Inirida y el motocarro
//! ("torito", el vehiculo de tres ruedas que da nombre a la app). Cada uno
//! es puramente presentacional — recibe un tamaño en pixeles y pinta con
//! `currentColor`, para heredar el color de texto de donde se use (ver
//! `.motoya-brand` en `theme.rs`).

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct IconProps {
    /// Ancho y alto del icono, en pixeles.
    #[props(default = 24)]
    pub size: u32,
}

/// Silueta de los Cerros de Mavicure: tres picos rocosos.
#[component]
pub fn MavicureHillsIcon(props: IconProps) -> Element {
    let size = props.size;
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            xmlns: "http://www.w3.org/2000/svg",
            role: "img",
            "aria-label": "Cerros de Mavicure",
            path {
                d: "M2 19 L7 8 L10 13 L13 6 L17 14 L19 10 L22 19 Z",
                fill: "currentColor",
            }
        }
    }
}

/// Flor de Inirida estilizada: cinco petalos alrededor de un centro.
#[component]
pub fn IniridaFlowerIcon(props: IconProps) -> Element {
    let size = props.size;
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            xmlns: "http://www.w3.org/2000/svg",
            role: "img",
            "aria-label": "Flor de Inirida",
            g {
                fill: "currentColor",
                ellipse { cx: "12", cy: "5.5", rx: "3", ry: "4.5" }
                ellipse {
                    cx: "18.5",
                    cy: "12",
                    rx: "3",
                    ry: "4.5",
                    transform: "rotate(90 18.5 12)",
                }
                ellipse { cx: "12", cy: "18.5", rx: "3", ry: "4.5" }
                ellipse {
                    cx: "5.5",
                    cy: "12",
                    rx: "3",
                    ry: "4.5",
                    transform: "rotate(90 5.5 12)",
                }
                circle { cx: "12", cy: "12", r: "2.6" }
            }
        }
    }
}

/// Motocarro ("torito"): vehiculo de tres ruedas visto de lado.
#[component]
pub fn MotocarroIcon(props: IconProps) -> Element {
    let size = props.size;
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            xmlns: "http://www.w3.org/2000/svg",
            role: "img",
            "aria-label": "Motocarro",
            g {
                fill: "currentColor",
                path { d: "M2 16 L4 9 L11 9 L13 12 L16 12 L16 16 Z" }
                path { d: "M16 11 L20 11 L21 14 L16 14 Z" }
            }
            g {
                fill: "none",
                stroke: "currentColor",
                "stroke-width": "1.6",
                circle { cx: "5.5", cy: "17.5", r: "2.2" }
                circle { cx: "18.5", cy: "17.5", r: "2.2" }
            }
        }
    }
}
