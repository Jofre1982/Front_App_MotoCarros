//! Foto de perfil redonda (backend #109). La foto esta protegida por token,
//! asi que no sirve un `<img src>` directo: se descarga con
//! `ApiClient::protected_image` y se muestra como `data:` URL, mismo
//! criterio que la foto de los mandados. Sin foto (o si no carga) muestra
//! las iniciales.

use std::sync::Arc;

use base64::Engine;
use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

/// "Luis Perez" → "LP"; un solo nombre → su inicial.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

#[derive(Props, Clone, PartialEq)]
pub struct AvatarProps {
    pub url: Option<String>,
    pub name: String,
    /// Lado en pixeles.
    #[props(default = 64)]
    pub size: u32,
}

/// Descarga una imagen protegida por token y la devuelve como `data:` URL
/// (`None` mientras carga, si no hay o si falla). La usan la foto de perfil
/// y la del vehiculo.
pub fn use_protected_image(url: Option<String>) -> Signal<Option<String>> {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let mut image = use_signal(|| None::<String>);
    let mut loaded_url = use_signal(|| None::<String>);

    use_effect(use_reactive!(|url| {
        if *loaded_url.peek() == url {
            return;
        }
        loaded_url.set(url.clone());
        image.set(None);
        let (Some(url), Some(token)) = (url, session.token()) else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            if let Ok(fetch) = api_client.protected_image(&token, &url).await {
                if let Some(refreshed) = fetch.refreshed_token {
                    session.update_token(refreshed, storage.as_ref());
                }
                let encoded = base64::engine::general_purpose::STANDARD.encode(&fetch.data.bytes);
                image.set(Some(format!(
                    "data:{};base64,{encoded}",
                    fetch.data.content_type
                )));
            }
        });
    }));
    image
}

#[component]
pub fn Avatar(props: AvatarProps) -> Element {
    let image = use_protected_image(props.url.clone());

    let style = format!(
        "width: {0}px; height: {0}px; font-size: {1}px;",
        props.size,
        props.size * 2 / 5
    );
    rsx! {
        if let Some(src) = image() {
            img { class: "avatar", style: "{style}", src: "{src}", alt: "Foto de {props.name}" }
        } else {
            div { class: "avatar avatar-initials", style: "{style}", "{initials(&props.name)}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct VehiclePhotoProps {
    pub url: Option<String>,
}

/// Foto del vehiculo del conductor (backend #112), ancha, para reconocer el
/// motocarro por su color. Sin foto aprobada no muestra nada.
#[component]
pub fn VehiclePhoto(props: VehiclePhotoProps) -> Element {
    let image = use_protected_image(props.url.clone());
    rsx! {
        if let Some(src) = image() {
            figure { class: "vehicle-photo",
                img { src: "{src}", alt: "Foto del vehiculo" }
                figcaption { "🛺 Así es su vehículo" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_takes_the_first_two_words() {
        assert_eq!(initials("luis pérez gómez"), "LP");
        assert_eq!(initials("Ana"), "A");
        assert_eq!(initials("  "), "");
    }
}
