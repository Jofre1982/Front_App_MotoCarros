//! Puente de ubicacion para movil nativo — issue #19.
//!
//! TODO(issue de seguimiento): esto deberia usar la API de ubicacion nativa
//! de Android/iOS (con soporte real de segundo plano, a diferencia del
//! target web). Todavia no hay puente nativo (proyecto Xcode/Gradle) en
//! este esqueleto para exponer esas APIs a Rust — mismo criterio que
//! `InMemoryTokenStorage` en `main.rs`. `NativeLocationProvider` siempre
//! devuelve `Unavailable` de forma explicita y documentada, no un descuido:
//! evita que la app entre en panico por falta de contexto, sin fingir que
//! el tracking en movil ya funciona.

use moto_core::location::{LocationError, LocationFuture, LocationProvider};

#[derive(Debug, Default)]
pub struct NativeLocationProvider;

impl NativeLocationProvider {
    pub fn new() -> Self {
        Self
    }
}

impl LocationProvider for NativeLocationProvider {
    fn current_position(&self) -> LocationFuture {
        Box::pin(async move {
            Err(LocationError::Unavailable(
                "Compartir ubicacion todavia no esta disponible en la app movil.".to_string(),
            ))
        })
    }
}
