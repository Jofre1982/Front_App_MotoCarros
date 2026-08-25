//! Abstraccion de almacenamiento persistente del JWT, especifica por
//! plataforma (issue #3).
//!
//! `moto_core` no sabe *como* se guarda el token — solo define el contrato.
//! Cada binario de plataforma (`web`/`mobile`) provee la implementacion real
//! y la inyecta via contexto de Dioxus, igual que ya hace con `ApiClient`
//! (ver `.claude/STANDARDS.md`).

use crate::models::AuthToken;

/// Guarda/recupera el `AuthToken` para que sobreviva a cerrar la app. Cada
/// plataforma decide el mecanismo (web: storage del navegador; movil:
/// keychain/keystore nativo) — nunca texto plano sin justificar la
/// excepcion.
pub trait TokenStorage: std::fmt::Debug {
    fn save(&self, token: &AuthToken);
    fn load(&self) -> Option<AuthToken>;
    fn clear(&self);
}

/// Storage en memoria: no sobrevive a cerrar la app. Sirve de base para
/// tests y como implementacion explicita (no un descuido) donde todavia no
/// existe un mecanismo seguro nativo — ver `crates/mobile/src/storage.rs`
/// para el caso concreto y su justificacion.
#[derive(Debug, Default)]
pub struct InMemoryTokenStorage {
    token: std::sync::Mutex<Option<AuthToken>>,
}

impl InMemoryTokenStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStorage for InMemoryTokenStorage {
    fn save(&self, token: &AuthToken) {
        *self.token.lock().unwrap() = Some(token.clone());
    }

    fn load(&self) -> Option<AuthToken> {
        self.token.lock().unwrap().clone()
    }

    fn clear(&self) {
        *self.token.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_token() -> AuthToken {
        AuthToken {
            access_token: "jwt-token".to_string(),
            token_type: "bearer".to_string(),
            expires_in: Some(900),
        }
    }

    #[test]
    fn starts_without_a_token() {
        let storage = InMemoryTokenStorage::new();
        assert_eq!(storage.load(), None);
    }

    #[test]
    fn saves_and_loads_the_token() {
        let storage = InMemoryTokenStorage::new();
        storage.save(&sample_token());
        assert_eq!(storage.load(), Some(sample_token()));
    }

    #[test]
    fn save_overwrites_the_previous_token() {
        let storage = InMemoryTokenStorage::new();
        storage.save(&sample_token());
        let other = AuthToken {
            access_token: "other-token".to_string(),
            ..sample_token()
        };
        storage.save(&other);

        assert_eq!(storage.load(), Some(other));
    }

    #[test]
    fn clear_removes_the_saved_token() {
        let storage = InMemoryTokenStorage::new();
        storage.save(&sample_token());
        storage.clear();

        assert_eq!(storage.load(), None);
    }
}
