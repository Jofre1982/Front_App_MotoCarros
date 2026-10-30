//! Ubicacion (GPS) desde el WebView de la app movil.
//!
//! El WebView de Android (wry) ya sabe pedirle el permiso de ubicacion al
//! sistema (`onGeolocationPermissionsShowPrompt`) y Dioxus lo sirve con
//! https para que `navigator.geolocation` funcione (contexto seguro). Asi
//! que se usa la misma API que en la web; el manifest declara
//! ACCESS_FINE_LOCATION (ver `.github/workflows/android-apk.yml`).
//!
//! Limitacion conocida: funciona con la app abierta. Con la pantalla
//! apagada Android suspende el WebView; eso queda para un paso aparte.

use dioxus::prelude::*;
use moto_core::location::{LocationError, LocationFuture, LocationProvider};
use moto_core::models::Coordinates;

const GEOLOCATION_SCRIPT: &str = r#"
(function () {
    if (!navigator.geolocation) {
        dioxus.send({ kind: "error", message: "Este celular no permite leer la ubicacion." });
        return;
    }
    navigator.geolocation.getCurrentPosition(
        function (position) {
            dioxus.send({
                kind: "success",
                latitude: position.coords.latitude,
                longitude: position.coords.longitude,
            });
        },
        function (error) {
            if (error.code === error.PERMISSION_DENIED) {
                dioxus.send({ kind: "denied" });
            } else {
                dioxus.send({
                    kind: "error",
                    message: error.message || "No se pudo obtener la ubicacion.",
                });
            }
        },
        { enableHighAccuracy: true, timeout: 15000, maximumAge: 5000 }
    );
})();
"#;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind")]
enum GeolocationEvent {
    #[serde(rename = "success")]
    Success { latitude: f64, longitude: f64 },
    #[serde(rename = "denied")]
    Denied,
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Default)]
pub struct WebViewLocationProvider;

impl WebViewLocationProvider {
    pub fn new() -> Self {
        Self
    }
}

impl LocationProvider for WebViewLocationProvider {
    fn current_position(&self) -> LocationFuture {
        Box::pin(async move {
            let mut eval = document::eval(GEOLOCATION_SCRIPT);
            let event: GeolocationEvent = eval
                .recv()
                .await
                .map_err(|err| LocationError::Unavailable(err.to_string()))?;
            match event {
                GeolocationEvent::Success {
                    latitude,
                    longitude,
                } => Ok(Coordinates {
                    latitude,
                    longitude,
                }),
                GeolocationEvent::Denied => Err(LocationError::PermissionDenied),
                GeolocationEvent::Error { message } => Err(LocationError::Unavailable(message)),
            }
        })
    }
}
