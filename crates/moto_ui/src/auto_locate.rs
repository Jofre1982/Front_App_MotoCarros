//! Punto de recogida automatico (pedido del dueno: "apenas abra la app me
//! muestre el punto donde me encuentro sin necesidad de tocar la pantalla").
//!
//! Al abrir la pantalla de pedir (motocarro, mandado o motocarga) se lee el
//! GPS y, si el pasajero todavia no marco nada, el punto se pone ahi y el
//! mapa se centra. Si lo recogen en otro lado, toca el mapa como siempre.
//! Sin GPS o sin permiso, queda como antes: marcar con la mano.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::location::LocationProvider;

/// En que va la busqueda de la ubicacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocateState {
    /// Todavia no se busca (o se volvio a empezar tras pedir otro).
    Idle,
    /// Esperando al GPS.
    Searching,
    /// El punto se puso solo con el GPS.
    Located,
    /// El pasajero toco el mapa: manda lo que el marco.
    Manual,
    /// No hay GPS o no dio permiso: se marca con la mano.
    Failed,
}

/// Texto de ayuda sobre el mapa segun el estado.
pub fn pickup_hint(state: LocateState, has_point: bool) -> &'static str {
    match (state, has_point) {
        (LocateState::Searching, false) | (LocateState::Idle, false) => {
            "📍 Buscando tu ubicacion..."
        }
        (LocateState::Located, true) => {
            "📍 Te ubicamos aqui. Si te recogen en otro lugar, toca el mapa ahi."
        }
        (LocateState::Failed, false) => {
            "No pudimos leer tu ubicacion (revisa que el GPS este encendido). Toca el mapa en el lugar donde estas."
        }
        (_, true) => "Listo, ya marcamos el punto. Puedes tocar otro lugar para cambiarlo.",
        (_, false) => "Toca el mapa en el lugar donde estas.",
    }
}

#[derive(Clone, Copy)]
pub struct AutoLocate {
    pub state: Signal<LocateState>,
    /// Centro del mapa: el pueblo, o la ubicacion leida.
    pub center: Signal<(f64, f64)>,
}

impl AutoLocate {
    /// El pasajero toco el mapa.
    pub fn mark_manual(mut self) {
        self.state.set(LocateState::Manual);
    }
}

/// `active`: la pantalla esta mostrando el mapa para pedir (sin un pedido
/// en curso). `default_center`: donde se centra el mapa sin GPS.
pub fn use_auto_locate(
    mut origin: Signal<Option<(f64, f64)>>,
    active: Memo<bool>,
    default_center: (f64, f64),
) -> AutoLocate {
    let provider = try_use_context::<Arc<dyn LocationProvider>>();
    let mut state = use_signal(|| LocateState::Idle);
    let mut center = use_signal(|| default_center);

    use_effect(move || {
        let point = origin();
        if !active() {
            return;
        }
        let current = *state.peek();
        if point.is_none() && matches!(current, LocateState::Located | LocateState::Manual) {
            // Se borro el punto (pidio otro, cancelo): se vuelve a ubicar.
            state.set(LocateState::Idle);
        }
        if point.is_some() || *state.peek() != LocateState::Idle {
            return;
        }
        let Some(provider) = provider.clone() else {
            state.set(LocateState::Failed);
            return;
        };
        state.set(LocateState::Searching);
        spawn(async move {
            let result = provider.current_position().await;
            if *state.peek() != LocateState::Searching {
                return;
            }
            match result {
                Ok(position) if origin.peek().is_none() => {
                    let point = (position.latitude, position.longitude);
                    origin.set(Some(point));
                    center.set(point);
                    state.set(LocateState::Located);
                }
                // Toco el mapa mientras el GPS respondia: manda lo suyo.
                Ok(_) => state.set(LocateState::Manual),
                Err(_) => state.set(LocateState::Failed),
            }
        });
    });

    AutoLocate { state, center }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_follow_the_search() {
        assert_eq!(
            pickup_hint(LocateState::Searching, false),
            "📍 Buscando tu ubicacion..."
        );
        assert!(pickup_hint(LocateState::Located, true).starts_with("📍 Te ubicamos aqui"));
        assert!(pickup_hint(LocateState::Failed, false).contains("GPS"));
        assert!(pickup_hint(LocateState::Manual, true).starts_with("Listo"));
    }
}
