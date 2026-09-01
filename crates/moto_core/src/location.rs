//! Abstraccion de obtencion de la posicion actual del dispositivo,
//! especifica por plataforma (issue #19).
//!
//! `moto_core` no sabe *como* se consigue la posicion — solo define el
//! contrato. Cada binario de plataforma (`web`/`mobile`) provee la
//! implementacion real y la inyecta via contexto de Dioxus, igual que ya
//! hace con `TokenStorage` (ver `.claude/STANDARDS.md`).

use std::future::Future;
use std::pin::Pin;

use crate::models::Coordinates;

/// Fallo al pedir la posicion actual. `PermissionDenied` es el unico caso
/// que la UI necesita distinguir explicitamente del resto (criterio de
/// aceptacion del issue: pedir habilitar el permiso en vez de fallar en
/// silencio) — cualquier otro fallo (navegador sin soporte, timeout, GPS
/// apagado, plataforma sin puente todavia) queda bajo `Unavailable` con un
/// mensaje ya listo para mostrar.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationError {
    PermissionDenied,
    Unavailable(String),
}

impl std::fmt::Display for LocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocationError::PermissionDenied => write!(
                f,
                "Habilita el permiso de ubicacion para compartirla durante el viaje."
            ),
            LocationError::Unavailable(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LocationError {}

/// Future sin `Send` que resuelve a la posicion actual: en el target web la
/// obtiene `navigator.geolocation` del navegador via un `JsFuture` (que no
/// es `Send`, ver `crates/web/src/geolocation.rs`) — Dioxus no lo exige,
/// porque su modelo de componentes ya corre en un solo hilo (signals
/// `!Send`), igual que el resto de este crate.
pub type LocationFuture = Pin<Box<dyn Future<Output = Result<Coordinates, LocationError>>>>;

/// Consigue la posicion actual del dispositivo. Cada plataforma decide el
/// mecanismo real: web usa `navigator.geolocation` (ver
/// `crates/web/src/geolocation.rs`); movil todavia no tiene puente a la API
/// nativa de ubicacion (ver `crates/mobile/src/location.rs`, mismo criterio
/// documentado que `InMemoryTokenStorage`).
pub trait LocationProvider: std::fmt::Debug {
    fn current_position(&self) -> LocationFuture;
}
