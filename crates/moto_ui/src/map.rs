//! Componente de mapa reutilizable — issue #4.
//!
//! No conoce nada de viajes ni de la API de `Back_App_MotoCarros`: solo
//! dibuja centro, zoom y marcadores a partir de props. Historias posteriores
//! (tarifa estimada, tracking en tiempo real, etc.) construyen sobre esto.
//!
//! Proveedor elegido: Leaflet + tiles de OpenStreetMap, cargados desde CDN
//! por JS embebido via `dioxus::document::eval`. Ver la justificacion
//! completa en `.claude/STANDARDS.md`.

use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

const LEAFLET_CSS_URL: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.css";
const LEAFLET_JS_URL: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.js";

// Hashes de Subresource Integrity (SRI) para leaflet@1.9.4, calculados
// directamente sobre los archivos servidos por unpkg (sha384). Fijar la
// version Y el hash juntos: si se sube la version de Leaflet, hay que
// recalcular estos hashes contra los nuevos archivos.
const LEAFLET_CSS_INTEGRITY: &str =
    "sha384-sHL9NAb7lN7rfvG5lfHpm643Xkcjzp4jFvuavGOndn6pjVqS6ny56CAt3nsEVT4H";
const LEAFLET_JS_INTEGRITY: &str =
    "sha384-cxOPjt7s7Iz04uaHJceBmS+qpjv2JkIHNVcuOrM+YHwZOmJGBXI00mdUXEq65HTH";

const MAP_INIT_TEMPLATE: &str = r#"
(function () {
    function ready(cb) {
        if (window.L) {
            cb();
            return;
        }
        if (window.__motoyaLeafletLoading) {
            window.__motoyaLeafletLoading.then(cb);
            return;
        }

        var cssLink = document.createElement("link");
        cssLink.rel = "stylesheet";
        cssLink.href = "__MOTOYA_LEAFLET_CSS__";
        cssLink.integrity = "__MOTOYA_LEAFLET_CSS_INTEGRITY__";
        cssLink.crossOrigin = "anonymous";
        document.head.appendChild(cssLink);

        window.__motoyaLeafletLoading = new Promise(function (resolve) {
            var script = document.createElement("script");
            script.src = "__MOTOYA_LEAFLET_JS__";
            script.integrity = "__MOTOYA_LEAFLET_JS_INTEGRITY__";
            script.crossOrigin = "anonymous";
            script.onload = resolve;
            document.head.appendChild(script);
        });
        window.__motoyaLeafletLoading.then(cb);
    }

    ready(function () {
        var el = document.getElementById("__MOTOYA_MAP_ID__");
        if (!el) {
            return;
        }
        var map = L.map(el).setView([__MOTOYA_LAT__, __MOTOYA_LNG__], __MOTOYA_ZOOM__);
        L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
            maxZoom: 19,
            attribution: "&copy; OpenStreetMap contributors",
        }).addTo(map);
        __MOTOYA_MARKERS__
    });
})();
"#;

/// Un marcador a dibujar en el mapa. Agnostico de dominio: quien use
/// `MapView` decide que representa cada marcador (origen, destino,
/// ubicacion de un conductor, etc.).
#[derive(Clone, Debug, PartialEq)]
pub struct MapMarker {
    pub lat: f64,
    pub lng: f64,
    pub label: Option<String>,
}

impl MapMarker {
    fn to_js_statement(&self) -> String {
        let popup = match &self.label {
            // `serde_json::to_string` escapa comillas y caracteres
            // especiales del label para que no pueda romper el script
            // generado (evita inyeccion de JS via el texto del marcador).
            Some(label) => {
                let escaped = serde_json::to_string(label).unwrap_or_else(|_| "null".to_string());
                format!(".bindPopup({escaped})")
            }
            None => String::new(),
        };

        format!(
            "L.marker([{lat}, {lng}]){popup}.addTo(map);",
            lat = self.lat,
            lng = self.lng,
        )
    }
}

fn build_init_script(
    id: &str,
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    markers: &[MapMarker],
) -> String {
    let markers_js: String = markers
        .iter()
        .map(MapMarker::to_js_statement)
        .collect::<Vec<_>>()
        .join("\n        ");

    MAP_INIT_TEMPLATE
        .replace("__MOTOYA_LEAFLET_CSS__", LEAFLET_CSS_URL)
        .replace("__MOTOYA_LEAFLET_JS__", LEAFLET_JS_URL)
        .replace("__MOTOYA_LEAFLET_CSS_INTEGRITY__", LEAFLET_CSS_INTEGRITY)
        .replace("__MOTOYA_LEAFLET_JS_INTEGRITY__", LEAFLET_JS_INTEGRITY)
        .replace("__MOTOYA_MAP_ID__", id)
        .replace("__MOTOYA_LAT__", &center_lat.to_string())
        .replace("__MOTOYA_LNG__", &center_lng.to_string())
        .replace("__MOTOYA_ZOOM__", &zoom.to_string())
        .replace("__MOTOYA_MARKERS__", &markers_js)
}

static MAP_INSTANCE_COUNTER: AtomicU32 = AtomicU32::new(0);

fn next_map_id() -> String {
    let n = MAP_INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("motoya-map-{n}")
}

/// Mapa reutilizable: centro, zoom y marcadores por props, sin logica de
/// negocio. Renderiza un `div` y delega la inicializacion real a Leaflet
/// via `dioxus::document::eval`, que funciona tanto en el build web (WASM)
/// como en renderers basados en webview.
///
/// Limitacion conocida (documentada, no un descuido): la inicializacion
/// corre una sola vez al montar el componente, igual que el patron ya usado
/// en `App::hydrate` (ver `moto_ui/src/lib.rs`). Si `center`/`zoom`/
/// `markers` cambian en un remount posterior, el mapa no se actualiza
/// reactivamente todavia — eso queda para la historia que consuma esto con
/// tracking en tiempo real (fuera de alcance de este issue).
#[component]
pub fn MapView(
    center_lat: f64,
    center_lng: f64,
    #[props(default = 13)] zoom: u8,
    #[props(default = Vec::new())] markers: Vec<MapMarker>,
) -> Element {
    let map_id = use_hook(next_map_id);

    {
        let map_id = map_id.clone();
        use_effect(move || {
            let script = build_init_script(&map_id, center_lat, center_lng, zoom, &markers);
            document::eval(&script);
        });
    }

    rsx! {
        div {
            id: "{map_id}",
            class: "motoya-map-view",
            style: "width: 100%; height: 100%; min-height: 240px;",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_init_script_embeds_id_center_and_zoom() {
        let script = build_init_script("motoya-map-0", 4.710989, -74.072092, 15, &[]);

        assert!(script.contains(r#"document.getElementById("motoya-map-0")"#));
        assert!(script.contains("setView([4.710989, -74.072092], 15)"));
        assert!(script.contains(LEAFLET_CSS_URL));
        assert!(script.contains(LEAFLET_JS_URL));
    }

    #[test]
    fn build_init_script_sets_sri_integrity_and_crossorigin_on_css_and_js() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[]);

        assert!(script.contains(&format!(r#"cssLink.integrity = "{LEAFLET_CSS_INTEGRITY}""#)));
        assert!(script.contains(&format!(r#"script.integrity = "{LEAFLET_JS_INTEGRITY}""#)));
        assert!(script.contains(r#"cssLink.crossOrigin = "anonymous""#));
        assert!(script.contains(r#"script.crossOrigin = "anonymous""#));
    }

    #[test]
    fn build_init_script_with_no_markers_has_no_marker_statements() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[]);

        assert!(!script.contains("L.marker"));
    }

    #[test]
    fn build_init_script_adds_one_statement_per_marker() {
        let markers = vec![
            MapMarker {
                lat: 4.71,
                lng: -74.07,
                label: Some("Origen".to_string()),
            },
            MapMarker {
                lat: 4.72,
                lng: -74.08,
                label: None,
            },
        ];

        let script = build_init_script("motoya-map-1", 4.71, -74.07, 13, &markers);

        assert_eq!(script.matches("L.marker(").count(), 2);
        assert!(script.contains("L.marker([4.71, -74.07]).bindPopup(\"Origen\").addTo(map);"));
        assert!(script.contains("L.marker([4.72, -74.08]).addTo(map);"));
    }

    #[test]
    fn marker_label_with_quotes_is_escaped_and_cannot_break_out_of_the_script() {
        let markers = vec![MapMarker {
            lat: 1.0,
            lng: 2.0,
            label: Some(r#""); alert("xss"); ("#.to_string()),
        }];

        let script = build_init_script("motoya-map-2", 1.0, 2.0, 13, &markers);

        // El label queda como un literal JSON escapado (comillas escapadas
        // con `\"`), no como codigo JS suelto que rompa fuera del string.
        assert!(script.contains(r#".bindPopup("\"); alert(\"xss\"); (")"#));
    }

    #[test]
    fn next_map_id_returns_unique_ids() {
        let first = next_map_id();
        let second = next_map_id();

        assert_ne!(first, second);
        assert!(first.starts_with("motoya-map-"));
    }
}
