//! Storage seguro del JWT para el target web (WASM) — issue #3.
//!
//! Usa `sessionStorage` del navegador en vez de `localStorage`: se limpia al
//! cerrar la pestana/navegador, asi que un token robado (p.ej. via XSS) tiene
//! una ventana de vida mucho mas corta que si quedara en `localStorage`
//! indefinidamente.
//!
//! Limitacion conocida y documentada, no un descuido: ningun storage
//! accesible desde JavaScript (`sessionStorage`, `localStorage`, IndexedDB)
//! es inmune a XSS. La alternativa mas segura seria una cookie `HttpOnly`
//! emitida por el backend, pero `Back_App_MotoCarros` hoy devuelve el token
//! en el body de la respuesta (ver `openapi.yaml`), no en una cookie — migrar
//! a eso es una decision de backend que queda fuera de este issue.
//!
//! No se testea en CI (requiere un `window`/`sessionStorage` real de
//! navegador que no existe en el runner) — ver `.claude/STANDARDS.md`,
//! "el renderizado real en web/mobile no se testea en CI". Se valida
//! manualmente en el navegador.

use moto_core::models::AuthToken;
use moto_core::storage::TokenStorage;

const STORAGE_KEY: &str = "motoya.auth_token";

#[derive(Debug, Default)]
pub struct WebTokenStorage;

impl WebTokenStorage {
    pub fn new() -> Self {
        Self
    }

    fn session_storage(&self) -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok().flatten()
    }
}

impl TokenStorage for WebTokenStorage {
    fn save(&self, token: &AuthToken) {
        let Some(storage) = self.session_storage() else {
            return;
        };
        if let Ok(json) = serde_json::to_string(token) {
            let _ = storage.set_item(STORAGE_KEY, &json);
        }
    }

    fn load(&self) -> Option<AuthToken> {
        let storage = self.session_storage()?;
        let json = storage.get_item(STORAGE_KEY).ok().flatten()?;
        serde_json::from_str(&json).ok()
    }

    fn clear(&self) {
        if let Some(storage) = self.session_storage() {
            let _ = storage.remove_item(STORAGE_KEY);
        }
    }
}
