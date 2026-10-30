use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::location::LocationProvider;
use moto_core::realtime::RealtimeConfig;
use moto_core::storage::{FileTokenStorage, InMemoryTokenStorage, TokenStorage};
use moto_ui::App;

use moto_ui::geolocation::WebViewLocationProvider;

/// Fallback de desarrollo local. La URL real del backend se inyecta en build
/// time via la variable de entorno `MOTOYA_API_BASE_URL` (nunca hardcodeada
/// en el binario para un despliegue real — ver `.claude/STANDARDS.md`).
const DEFAULT_API_BASE_URL: &str = "http://localhost:8000";

fn main() {
    let base_url = option_env!("MOTOYA_API_BASE_URL")
        .unwrap_or(DEFAULT_API_BASE_URL)
        .to_string();
    // Sin fallback de desarrollo local a proposito: la URL incluye la app key
    // de Reverb, un secreto por entorno sin valor por defecto (ver
    // `RealtimeConfig`). `None` hasta que el entorno configure
    // `MOTOYA_WS_URL`.
    let ws_url = option_env!("MOTOYA_WS_URL").map(str::to_string);

    LaunchBuilder::new()
        .with_context_provider(move || Box::new(ApiClient::new(base_url.clone())))
        .with_context_provider(move || {
            Box::new(RealtimeConfig {
                ws_url: ws_url.clone(),
            })
        })
        // La sesion se guarda en el directorio privado de la app para no
        // pedir el login cada vez que se abre (ver `FileTokenStorage`). Si
        // no se encuentra ese directorio, queda en memoria como antes.
        .with_context_provider(|| {
            let storage: Arc<dyn TokenStorage> = match session_file() {
                Some(path) => Arc::new(FileTokenStorage::new(path)),
                None => Arc::new(InMemoryTokenStorage::new()),
            };
            Box::new(storage)
        })
        // GPS del celular a traves del WebView (ver `moto_ui::geolocation`).
        .with_context_provider(|| {
            Box::new(Arc::new(WebViewLocationProvider::new()) as Arc<dyn LocationProvider>)
        })
        .launch(App);
}

/// Archivo de sesion dentro del almacenamiento interno de la app.
///
/// En Android: `/data/data/<paquete>/files/motoya_session.json`; el nombre
/// del paquete sale de `/proc/self/cmdline` (el proceso de la app se llama
/// igual que su paquete). En iOS/escritorio: bajo `$HOME`.
fn session_file() -> Option<std::path::PathBuf> {
    const FILE_NAME: &str = "motoya_session.json";
    if cfg!(target_os = "android") {
        let cmdline = std::fs::read("/proc/self/cmdline").ok()?;
        let name = String::from_utf8_lossy(&cmdline);
        let package = name.split(['\0', ':']).next()?.trim();
        if package.is_empty() || package.contains('/') {
            return None;
        }
        let dir = std::path::Path::new("/data/data")
            .join(package)
            .join("files");
        std::fs::create_dir_all(&dir).ok()?;
        return Some(dir.join(FILE_NAME));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".motoya").join(FILE_NAME))
}
