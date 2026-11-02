//! Token de notificaciones push del celular (Firebase Cloud Messaging).
//!
//! En Android lo consigue el codigo nativo (`MotoYaPush.kt`, agregado por el
//! workflow del APK) y lo deja en un archivo del almacenamiento privado de la
//! app; `moto_core` solo define como leerlo. La web no tiene push: no provee
//! ninguna fuente y la app sigue igual.

/// De donde sale el token push de este dispositivo.
pub trait PushTokenSource: std::fmt::Debug {
    /// El token actual, o `None` si todavia no hay (Firebase tarda unos
    /// segundos la primera vez) o el celular no tiene Firebase.
    fn token(&self) -> Option<String>;
    /// `"android"` o `"ios"`, como lo espera `POST /me/device-token`.
    fn platform(&self) -> &'static str;
}

/// Lee el token del archivo que escribe el codigo nativo de Android.
#[derive(Debug)]
pub struct FilePushTokenSource {
    path: std::path::PathBuf,
}

impl FilePushTokenSource {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl PushTokenSource for FilePushTokenSource {
    fn token(&self) -> Option<String> {
        let token = std::fs::read_to_string(&self.path).ok()?;
        let token = token.trim();
        (!token.is_empty()).then(|| token.to_string())
    }

    fn platform(&self) -> &'static str {
        "android"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_token_written_by_android() {
        let dir = std::env::temp_dir().join(format!("motoya-push-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("motoya_push_token.txt");
        let source = FilePushTokenSource::new(&path);

        assert_eq!(source.token(), None);
        std::fs::write(&path, "fcm-token-123\n").unwrap();
        assert_eq!(source.token().as_deref(), Some("fcm-token-123"));
        assert_eq!(source.platform(), "android");
    }
}
