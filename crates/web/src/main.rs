use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::ApiClient;
use moto_core::realtime::RealtimeConfig;
use moto_core::storage::TokenStorage;
use moto_ui::App;

mod storage;

use storage::WebTokenStorage;

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
        .with_context_provider(|| {
            Box::new(Arc::new(WebTokenStorage::new()) as Arc<dyn TokenStorage>)
        })
        .launch(App);
}
