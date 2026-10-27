//! Campos de formulario reutilizados por varias pantallas.
//!
//! `PasswordInput` agrega el "ojito" para ver lo que se escribio: en el
//! celular es facil equivocarse de tecla sin notarlo (pedido del dueno del
//! repo probando el APK). Las pantallas que crean una contrasena nueva la
//! piden dos veces y comparan con [`password_confirmation_error`] antes de
//! enviar nada al backend.

use dioxus::prelude::*;

/// Mensaje a mostrar junto al campo de confirmacion, o `None` si no hay
/// nada que reclamar. Un campo de confirmacion vacio no es error aca: de eso
/// ya se encarga la validacion de campos vacios de cada pantalla.
pub fn password_confirmation_error(password: &str, confirmation: &str) -> Option<&'static str> {
    if confirmation.is_empty() || password == confirmation {
        None
    } else {
        Some("Las contrasenas no coinciden. Escribelas igual en los dos campos.")
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PasswordInputProps {
    pub id: String,
    pub value: String,
    /// `"current-password"` en el login, `"new-password"` al crear una.
    pub autocomplete: &'static str,
    pub disabled: bool,
    pub oninput: EventHandler<String>,
}

#[component]
pub fn PasswordInput(props: PasswordInputProps) -> Element {
    let mut visible = use_signal(|| false);
    let input_type = if visible() { "text" } else { "password" };
    let toggle_label = if visible() {
        "Ocultar contrasena"
    } else {
        "Ver contrasena"
    };

    rsx! {
        div { class: "password-field",
            input {
                id: "{props.id}",
                r#type: "{input_type}",
                autocomplete: props.autocomplete,
                autocapitalize: "off",
                spellcheck: "false",
                disabled: props.disabled,
                value: "{props.value}",
                oninput: move |event| props.oninput.call(event.value()),
            }
            button {
                r#type: "button",
                class: "password-toggle",
                aria_label: toggle_label,
                title: toggle_label,
                aria_pressed: if visible() { "true" } else { "false" },
                onclick: move |_| visible.toggle(),
                // Ojo abierto / ojo tachado, sin depender de fuentes de iconos.
                svg {
                    width: "22",
                    height: "22",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" }
                    circle { cx: "12", cy: "12", r: "3" }
                    if visible() {
                        line { x1: "3", y1: "3", x2: "21", y2: "21" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::password_confirmation_error;

    #[test]
    fn iguales_no_reclama() {
        assert_eq!(
            password_confirmation_error("motoya2026", "motoya2026"),
            None
        );
    }

    #[test]
    fn confirmacion_vacia_no_reclama_todavia() {
        assert_eq!(password_confirmation_error("motoya2026", ""), None);
    }

    #[test]
    fn distintas_reclama() {
        assert!(password_confirmation_error("motoya2026", "motoya2025").is_some());
    }

    #[test]
    fn distingue_mayusculas() {
        assert!(password_confirmation_error("MotoYa2026", "motoya2026").is_some());
    }
}
