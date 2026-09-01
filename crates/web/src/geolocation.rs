//! Obtencion de la posicion actual via `navigator.geolocation` del
//! navegador — issue #19.
//!
//! Usa `dioxus::document::eval` para invocar la API del navegador, mismo
//! mecanismo que `MapView` (`moto_ui::map`) para Leaflet: es parte del core
//! de Dioxus, no requiere `#[cfg(target_arch = "wasm32")]` disperso. La
//! diferencia con Leaflet es que aca el resultado es un solo valor (o un
//! fallo), no un stream de eventos — el script manda exactamente un mensaje
//! por invocacion via `dioxus.send`.
//!
//! Riesgo de arquitectura conocido (ver `.claude/CLAUDE.md`): esto solo
//! funciona mientras la pestana esta en primer plano. Los navegadores
//! suspenden o limitan fuertemente `navigator.geolocation` con la pantalla
//! apagada o la app minimizada/en segundo plano — no hay forma de superar
//! esa limitacion desde JS de pagina. Si el piloto necesita tracking en
//! segundo plano confiable para el conductor, la salida es el shell nativo
//! delgado que ya anticipa `.claude/CLAUDE.md`, con una implementacion de
//! `LocationProvider` propia en `crates/mobile` que use la API de ubicacion
//! del sistema operativo en vez de esto.

use dioxus::prelude::*;
use moto_core::location::{LocationError, LocationFuture, LocationProvider};
use moto_core::models::Coordinates;

const GEOLOCATION_SCRIPT: &str = r#"
(function () {
    if (!navigator.geolocation) {
        dioxus.send({ kind: "error", message: "Este navegador no soporta geolocalizacion." });
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
        { enableHighAccuracy: true, timeout: 10000, maximumAge: 0 }
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
pub struct WebLocationProvider;

impl WebLocationProvider {
    pub fn new() -> Self {
        Self
    }
}

impl LocationProvider for WebLocationProvider {
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
