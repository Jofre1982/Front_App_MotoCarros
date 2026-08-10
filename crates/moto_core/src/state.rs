//! Signals/stores de Dioxus agnosticos de plataforma, expuestos a `moto_ui`
//! via props o contexto.

use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionState {
    pub is_authenticated: Signal<bool>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            is_authenticated: Signal::new(false),
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}
