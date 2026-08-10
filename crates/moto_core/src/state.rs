//! Signals/stores de Dioxus agnosticos de plataforma, expuestos a `moto_ui`
//! via props o contexto.
//!
//! El JWT se guarda unicamente en memoria (este signal vive mientras la app
//! esta abierta). Sobrevivir a un reinicio de la app es una decision de
//! storage seguro por plataforma (web vs. movil) que no forma parte de este
//! issue — ver `.claude/CLAUDE.md` del issue #1.

use dioxus::prelude::*;

use crate::models::{AuthToken, AuthenticatedUser, User};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionState {
    user: Signal<Option<User>>,
    token: Signal<Option<AuthToken>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            user: Signal::new(None),
            token: Signal::new(None),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.read().is_some()
    }

    pub fn user(&self) -> Option<User> {
        self.user.read().clone()
    }

    pub fn token(&self) -> Option<AuthToken> {
        self.token.read().clone()
    }

    /// Guarda la cuenta y el token que devolvio `POST /api/v1/auth/login`.
    pub fn authenticate(&mut self, authenticated: AuthenticatedUser) {
        self.user.set(Some(authenticated.user));
        self.token.set(Some(authenticated.token));
    }

    pub fn logout(&mut self) {
        self.user.set(None);
        self.token.set(None);
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}
