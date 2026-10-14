//! Signals/stores de Dioxus agnosticos de plataforma, expuestos a `moto_ui`
//! via props o contexto.
//!
//! El signal vive mientras la app esta abierta; sobrevivir a un reinicio
//! depende de un `TokenStorage` (issue #3) que el caller (pantalla o binario
//! de plataforma) debe pasar explicitamente en cada mutacion — `SessionState`
//! no lo guarda para seguir siendo `Copy`, igual que el resto de los signals
//! de este modulo.

use dioxus::prelude::*;

use crate::models::{AuthToken, AuthenticatedUser, Role, User};
use crate::storage::TokenStorage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionState {
    user: Signal<Option<User>>,
    token: Signal<Option<AuthToken>>,
    /// Disponibilidad del conductor para mandados (historia #92 del backend,
    /// issues #77/#78 de este repo), tal como la dejo el ultimo
    /// `PATCH /me/availability/errands` exitoso en esta sesion. Vive aca y
    /// no como estado local de `ProfileScreen` porque tanto esa pantalla
    /// como la de mandados cercanos (#78) lo necesitan: el backend no
    /// expone ningun `GET` para leerlo, asi que este signal es la unica
    /// fuente de verdad que tiene la app — `None` significa "todavia no lo
    /// sabemos en esta sesion", nunca se asume `false`.
    errand_availability: Signal<Option<bool>>,
    /// El modo que eligio la persona en esta sesion (historia #99 del
    /// backend, issue #95): pasajero o conductor. Solo importa en una cuenta
    /// con ambos roles; `None` mientras no haya elegido (ver
    /// `needs_mode_choice`).
    mode: Signal<Option<Role>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            user: Signal::new(None),
            token: Signal::new(None),
            errand_availability: Signal::new(None),
            mode: Signal::new(None),
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
        // Cada inicio de sesion vuelve a preguntar el modo si la cuenta tiene
        // los dos: puede ser otra persona en el mismo celular.
        self.mode.set(None);
    }

    /// Limpia la sesion, tanto en memoria como en `storage`. Se usa tanto
    /// para un logout explicito del usuario como para el logout forzado
    /// cuando un refresh de token falla (ver `moto_core::api::ApiClient`).
    pub fn logout(&mut self, storage: &dyn TokenStorage) {
        storage.clear();
        self.user.set(None);
        self.token.set(None);
        self.errand_availability.set(None);
        self.mode.set(None);
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

    /// Lo que esta sesion sabe sobre la disponibilidad del conductor para
    /// mandados. `None` mientras no se haya llamado a
    /// `set_errand_availability` todavia (ver el campo homonimo).
    pub fn errand_availability(&self) -> Option<bool> {
        *self.errand_availability.read()
    }

    /// Guarda el valor que devolvio el ultimo
    /// `PATCH /me/availability/errands` exitoso (`ApiClient::update_errand_availability`).
    pub fn set_errand_availability(&mut self, is_available_for_errands: bool) {
        self.errand_availability.set(Some(is_available_for_errands));
    }
}

impl SessionState {
    /// Con que rol esta usando la app la persona (issue #95). Una cuenta de
    /// un solo rol esta siempre en ese rol; una con ambos, en el que eligio
    /// (`None` hasta que elija). `None` tambien mientras no se conozca la
    /// cuenta (p.ej. recien hidratado el token, antes de `GET /me`).
    pub fn active_mode(&self) -> Option<Role> {
        let user = self.user.read();
        let user = user.as_ref()?;

        if user.has_both_modes() {
            return *self.mode.read();
        }

        Some(if user.can_drive() {
            Role::Driver
        } else {
            Role::Passenger
        })
    }

    /// Si la app tiene que preguntar "¿Como quieres entrar?": la cuenta tiene
    /// ambos roles y todavia no eligio en esta sesion.
    pub fn needs_mode_choice(&self) -> bool {
        self.user.read().as_ref().is_some_and(User::has_both_modes) && self.mode.read().is_none()
    }

    /// Elige el modo (al entrar, o con "Cambiar a modo ..." del menu).
    pub fn set_mode(&mut self, mode: Role) {
        self.mode.set(Some(mode));
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
                phone_verified: false,
                role: Role::Passenger,
                roles: vec!["passenger".to_string()],
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

    #[test]
    fn errand_availability_starts_unknown_and_reflects_the_last_value_set() {
        let (before, after_true, after_false) = run_in_runtime(|| {
            let mut session = SessionState::new();
            let before = session.errand_availability();

            session.set_errand_availability(true);
            let after_true = session.errand_availability();

            session.set_errand_availability(false);
            let after_false = session.errand_availability();

            (before, after_true, after_false)
        });

        assert_eq!(before, None);
        assert_eq!(after_true, Some(true));
        assert_eq!(after_false, Some(false));
    }

    #[test]
    fn logout_resets_errand_availability_to_unknown() {
        let (before_logout, after_logout) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(sample_authenticated(), &storage);
            session.set_errand_availability(true);
            let before_logout = session.errand_availability();

            session.logout(&storage);

            (before_logout, session.errand_availability())
        });

        assert_eq!(before_logout, Some(true));
        assert_eq!(after_logout, None);
    }

    fn authenticated_with_roles(role: Role, roles: &[&str]) -> AuthenticatedUser {
        let mut authenticated = sample_authenticated();
        authenticated.user.role = role;
        authenticated.user.roles = roles.iter().map(|r| r.to_string()).collect();
        authenticated
    }

    #[test]
    fn a_single_role_account_is_always_in_that_mode() {
        let (passenger_mode, driver_mode, any_choice) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut passenger = SessionState::new();
            passenger.authenticate(
                authenticated_with_roles(Role::Passenger, &["passenger"]),
                &storage,
            );
            let mut driver = SessionState::new();
            // Backend anterior a la historia #99: sin `roles`.
            driver.authenticate(authenticated_with_roles(Role::Driver, &[]), &storage);

            (
                passenger.active_mode(),
                driver.active_mode(),
                passenger.needs_mode_choice() || driver.needs_mode_choice(),
            )
        });

        assert_eq!(passenger_mode, Some(Role::Passenger));
        assert_eq!(driver_mode, Some(Role::Driver));
        assert!(!any_choice);
    }

    #[test]
    fn a_dual_role_account_must_choose_and_can_switch() {
        let (before, needs_before, chosen, switched, needs_after) = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            session.authenticate(
                authenticated_with_roles(Role::Passenger, &["passenger", "driver"]),
                &storage,
            );
            let before = session.active_mode();
            let needs_before = session.needs_mode_choice();
            session.set_mode(Role::Driver);
            let chosen = session.active_mode();
            session.set_mode(Role::Passenger);
            (
                before,
                needs_before,
                chosen,
                session.active_mode(),
                session.needs_mode_choice(),
            )
        });

        assert_eq!(before, None);
        assert!(needs_before);
        assert_eq!(chosen, Some(Role::Driver));
        assert_eq!(switched, Some(Role::Passenger));
        assert!(!needs_after);
    }

    #[test]
    fn logging_in_again_asks_for_the_mode_again() {
        let needs = run_in_runtime(|| {
            let storage = InMemoryTokenStorage::new();
            let mut session = SessionState::new();
            let dual = authenticated_with_roles(Role::Driver, &["passenger", "driver"]);
            session.authenticate(dual.clone(), &storage);
            session.set_mode(Role::Driver);
            session.logout(&storage);
            session.authenticate(dual, &storage);
            session.needs_mode_choice()
        });

        assert!(needs);
    }
}
