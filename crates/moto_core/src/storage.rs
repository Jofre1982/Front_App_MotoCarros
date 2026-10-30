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

/// Storage en un archivo privado de la app (pedido del dueno: "que no
/// estemos ingresando las credenciales cada que se cierre la app, solo
/// cuando yo quiera cerrar sesion").
///
/// En Android el archivo vive en el directorio interno de la app
/// (`/data/data/<paquete>/files`), que el sistema solo deja leer a esta app;
/// es el mismo nivel de proteccion que `SharedPreferences`, lo habitual para
/// sesiones en apps moviles. Se guarda el JWT (que el backend renueva solo
/// durante 14 dias), nunca la contrasena. "Cerrar sesion" borra el archivo.
#[derive(Debug)]
pub struct FileTokenStorage {
    path: std::path::PathBuf,
}

impl FileTokenStorage {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl TokenStorage for FileTokenStorage {
    fn save(&self, token: &AuthToken) {
        let Ok(json) = serde_json::to_string(token) else {
            return;
        };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Se escribe aparte y se renombra: si la app se cierra a mitad de
        // camino no queda un archivo cortado.
        let temp = self.path.with_extension("tmp");
        if std::fs::write(&temp, json).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
            }
            let _ = std::fs::rename(&temp, &self.path);
        }
    }

    fn load(&self) -> Option<AuthToken> {
        let json = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&json).ok()
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.path);
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

    fn temp_session_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("motoya-test-{}-{name}", std::process::id()))
            .join("session.json")
    }

    #[test]
    fn file_storage_survives_closing_the_app() {
        let path = temp_session_path("survives");
        FileTokenStorage::new(&path).save(&sample_token());

        // "Se cierra la app": otra instancia lee el mismo archivo.
        assert_eq!(FileTokenStorage::new(&path).load(), Some(sample_token()));

        FileTokenStorage::new(&path).clear();
        assert_eq!(FileTokenStorage::new(&path).load(), None);
    }

    #[test]
    fn file_storage_without_a_file_has_no_session() {
        let path = temp_session_path("empty");
        assert_eq!(FileTokenStorage::new(path).load(), None);
    }
}
