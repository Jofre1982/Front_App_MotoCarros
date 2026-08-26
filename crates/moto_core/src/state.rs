//! Signals/stores de Dioxus agnosticos de plataforma, expuestos a `moto_ui`
//! via props o contexto.
//!
//! El signal vive mientras la app esta abierta; sobrevivir a un reinicio
//! depende de un `TokenStorage` (issue #3) que el caller (pantalla o binario
//! de plataforma) debe pasar explicitamente en cada mutacion — `SessionState`
//! no lo guarda para seguir siendo `Copy`, igual que el resto de los signals
//! de este modulo.

use dioxus::prelude::*;

use crate::models::{AuthToken, AuthenticatedUser, User};
use crate::storage::TokenStorage;

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

    /// Guarda la cuenta y el token que devolvio `POST /api/v1/auth/login`, y
    /// lo persiste en `storage` para que sobreviva a un reinicio de la app.
    pub fn authenticate(&mut self, authenticated: AuthenticatedUser, storage: &dyn TokenStorage) {
        storage.save(&authenticated.token);
        self.user.set(Some(authenticated.user));
        self.token.set(Some(authenticated.token));
    }

    /// Limpia la sesion, tanto en memoria como en `storage`. Se usa tanto
    /// para un logout explicito del usuario como para el logout forzado
    /// cuando un refresh de token falla (ver `moto_core::api::ApiClient`).
    pub fn logout(&mut self, storage: &dyn TokenStorage) {
        storage.clear();
        self.user.set(None);
        self.token.set(None);
    }

    /// Restaura el token persistido (si hay uno) al arrancar la app.
    ///
    /// No repuebla `user()`: eso requiere `GET /api/v1/me` (issue #9, fuera
    /// de alcance aca), asi que tras hidratar `is_authenticated()` puede ser
    /// `true` con `user()` todavia en `None` hasta que la pantalla que lo
    /// necesite lo pida.
    pub fn hydrate(&mut self, storage: &dyn TokenStorage) {
        if let Some(token) = storage.load() {
            self.token.set(Some(token));
        }
    }

    /// Reemplaza el token vigente (p.ej. tras el refresh automatico de una
    /// request autenticada) sin tocar `user()`.
    pub fn update_token(&mut self, token: AuthToken, storage: &dyn TokenStorage) {
        storage.save(&token);
        self.token.set(Some(token));
    }

    /// Guarda los datos de cuenta que devolvio `GET /api/v1/me` (issue #9).
    /// No toca el token: el caller que haya recibido un `refreshed_token`
    /// junto a la respuesta debe persistirlo aparte con `update_token`.
    pub fn set_user(&mut self, user: User) {
        self.user.set(Some(user));
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Role;
    use crate::storage::InMemoryTokenStorage;
    use std::any::Any;
    use std::cell::RefCell;

    // `Signal::new` (usado por `SessionState::new`) exige un runtime de
    // Dioxus activo — no alcanza con estar en un `#[test]` normal, ver el
    // panic de `dioxus_core::Runtime::current` si se prueba sin esto. Un
    // `VirtualDom` minimo provee ese runtime sin necesidad de renderizar
    // nada de verdad.
    //
    // Dioxus atrapa panics durante el render de un componente para sus
    // error boundaries (`any_props.rs::render`), asi que un `assert!` que
    // fallara *dentro* de `body` quedaria silenciado en vez de tumbar el
    // test. Por eso `body` solo hace las mutaciones y devuelve lo que haga
    // falta verificar; los asserts van despues, fuera del render.
    type PendingBody = Box<dyn FnOnce() -> Box<dyn Any>>;

    thread_local! {
        static PENDING: RefCell<Option<PendingBody>> = const { RefCell::new(None) };
        static RESULT: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
    }

    fn test_root() -> Element {
        if let Some(body) = PENDING.with(|cell| cell.borrow_mut().take()) {
            let value = body();
            RESULT.with(|cell| *cell.borrow_mut() = Some(value));
        }
        rsx! {}
    }

    fn run_in_runtime<T: 'static>(body: impl FnOnce() -> T + 'static) -> T {
        PENDING.with(|cell| {
            *cell.borrow_mut() = Some(Box::new(move || Box::new(body()) as Box<dyn Any>));
        });

        let mut vdom = dioxus::prelude::VirtualDom::new(test_root);
        vdom.rebuild_in_place();

        let result = RESULT
            .with(|cell| cell.borrow_mut().take())
            .expect("el body no corrio dentro del runtime de dioxus");

        *result
            .downcast::<T>()
            .expect("el tipo devuelto por run_in_runtime no coincide")
    }

    fn sample_authenticated() -> AuthenticatedUser {
        AuthenticatedUser {
            user: User {
                id: 1,
                name: "Ana Garcia".to_string(),
                email: "ana@example.com".to_string(),
                phone: "+573001234567".to_string(),
                role: Role::Passenger,
            },
            token: AuthToken {
                access_token: "jwt-token".to_string(),
                token_type: "bearer".to_string(),
                expires_in: Some(900),
            },
        }
    }

    #[test]
    fn authenticate_persists_the_token_in_storage() {
        let (is_authenticated, saved_access_token) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(sample_authenticated(), &storage);
            (
                session.is_authenticated(),
                storage.load().map(|t| t.access_token),
            )
        });

        assert!(is_authenticated);
        assert_eq!(saved_access_token.as_deref(), Some("jwt-token"));
    }

    #[test]
    fn logout_clears_both_the_session_and_storage() {
        let (is_authenticated, user, saved_token) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(sample_authenticated(), &storage);

            session.logout(&storage);

            (session.is_authenticated(), session.user(), storage.load())
        });

        assert!(!is_authenticated);
        assert_eq!(user, None);
        assert_eq!(saved_token, None);
    }

    #[test]
    fn hydrate_restores_a_previously_saved_token() {
        let (was_authenticated_before, is_authenticated_after, user) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            storage.save(&sample_authenticated().token);

            let mut session = SessionState::new();
            let before = session.is_authenticated();

            session.hydrate(&storage);

            (before, session.is_authenticated(), session.user())
        });

        assert!(!was_authenticated_before);
        assert!(is_authenticated_after);
        assert_eq!(user, None);
    }

    #[test]
    fn hydrate_is_a_no_op_when_nothing_was_saved() {
        let is_authenticated = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();

            session.hydrate(&storage);

            session.is_authenticated()
        });

        assert!(!is_authenticated);
    }

    #[test]
    fn update_token_replaces_the_token_without_touching_the_user() {
        let (session_access_token, saved_access_token, has_user) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(sample_authenticated(), &storage);

            let renewed = AuthToken {
                access_token: "renewed-token".to_string(),
                token_type: "bearer".to_string(),
                expires_in: Some(900),
            };
            session.update_token(renewed, &storage);

            (
                session.token().map(|t| t.access_token),
                storage.load().map(|t| t.access_token),
                session.user().is_some(),
            )
        });

        assert_eq!(session_access_token.as_deref(), Some("renewed-token"));
        assert_eq!(saved_access_token.as_deref(), Some("renewed-token"));
        assert!(has_user);
    }

    #[test]
    fn set_user_populates_the_profile_without_touching_the_token() {
        let (user, token) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(sample_authenticated(), &storage);

            let updated = User {
                name: "Ana Garcia Actualizada".to_string(),
                ..sample_authenticated().user
            };
            session.set_user(updated.clone());

            (session.user(), session.token())
        });

        assert_eq!(user.unwrap().name, "Ana Garcia Actualizada");
        assert_eq!(token.unwrap().access_token, "jwt-token");
    }
}
