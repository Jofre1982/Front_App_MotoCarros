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
        window.__motoyaMaps = window.__motoyaMaps || {};
        var map = L.map(el).setView([__MOTOYA_LAT__, __MOTOYA_LNG__], __MOTOYA_ZOOM__);
        window.__motoyaMaps["__MOTOYA_MAP_ID__"] = map;
        map.__motoyaMarkers = [];
        L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
            maxZoom: 19,
            attribution: "&copy; OpenStreetMap contributors",
        }).addTo(map);
        __MOTOYA_MARKERS__
        __MOTOYA_CLICK_HANDLER__
    });
})();
"#;

/// Actualiza un mapa ya inicializado (centro/zoom y marcadores) sin volver a
/// llamar `L.map(el)` — Leaflet no permite reinicializar el mismo elemento
/// del DOM dos veces ("Map container is already initialized"). Se usa
/// cuando `MapView` vuelve a renderizar con props distintas despues del
/// primer montaje (issue #20: tracking en tiempo real de la ubicacion del
/// conductor), la reactividad que `.claude/STANDARDS.md` deja pendiente
/// desde el issue #4. Busca la instancia guardada en
/// `window.__motoyaMaps` por `MAP_INIT_TEMPLATE`; si todavia no existe
/// (efecto de actualizacion disparado antes de que termine el `ready()`
/// inicial) no hace nada, sin lanzar un error.
const MAP_UPDATE_TEMPLATE: &str = r#"
(function () {
    var map = window.__motoyaMaps && window.__motoyaMaps["__MOTOYA_MAP_ID__"];
    if (!map) {
        return;
    }
    map.setView([__MOTOYA_LAT__, __MOTOYA_LNG__], __MOTOYA_ZOOM__);
    (map.__motoyaMarkers || []).forEach(function (marker) {
        map.removeLayer(marker);
    });
    map.__motoyaMarkers = [];
    __MOTOYA_MARKERS__
})();
"#;

/// Se agrega al script solo cuando `MapView` recibe `on_click` (issue #13):
/// sin el, el mapa no se suscribe a clicks y `dioxus.send` nunca se llama, asi
/// que el `Eval::recv` del lado de Rust simplemente no tiene nada que leer.
const CLICK_HANDLER_TEMPLATE: &str = r#"map.on("click", function (e) {
            dioxus.send({ lat: e.latlng.lat, lng: e.latlng.lng });
        });"#;

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

        // Se registra en `map.__motoyaMarkers` (no solo `.addTo(map)`) para
        // que `MAP_UPDATE_TEMPLATE` pueda encontrar y quitar los marcadores
        // de la vuelta anterior antes de agregar los nuevos.
        format!(
            "map.__motoyaMarkers.push(L.marker([{lat}, {lng}]){popup}.addTo(map));",
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
    clickable: bool,
) -> String {
    let markers_js: String = markers
        .iter()
        .map(MapMarker::to_js_statement)
        .collect::<Vec<_>>()
        .join("\n        ");
    let click_handler_js = if clickable {
        CLICK_HANDLER_TEMPLATE
    } else {
        ""
    };

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
        .replace("__MOTOYA_CLICK_HANDLER__", click_handler_js)
}

/// Contraparte de `build_init_script` para una vuelta de renderizado
/// posterior al montaje (ver `MAP_UPDATE_TEMPLATE`): recalcula centro,
/// zoom y marcadores sobre la instancia de Leaflet ya creada, sin tocar el
/// tile layer ni el listener de click, que no cambian entre renders.
fn build_update_script(
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
        .join("\n    ");

    MAP_UPDATE_TEMPLATE
        .replace("__MOTOYA_MAP_ID__", id)
        .replace("__MOTOYA_LAT__", &center_lat.to_string())
        .replace("__MOTOYA_LNG__", &center_lng.to_string())
        .replace("__MOTOYA_ZOOM__", &zoom.to_string())
        .replace("__MOTOYA_MARKERS__", &markers_js)
}

/// Payload que manda `dioxus.send` desde `CLICK_HANDLER_TEMPLATE`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
struct MapClickPayload {
    lat: f64,
    lng: f64,
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
/// `on_click`, si se pasa, se llama con `(lat, lng)` cada vez que el usuario
/// hace click en el mapa (issue #13: elegir origen/destino de un viaje).
/// Sigue siendo agnostico de dominio — no sabe que representa el punto
/// elegido, eso lo decide quien use `MapView`.
///
/// La primera vez que se monta, `L.map(el)` crea la instancia de Leaflet
/// (ver `MAP_INIT_TEMPLATE`) y la guarda en `window.__motoyaMaps`. Cada vez
/// que el componente vuelve a renderizar con `center`/`zoom`/`markers`
/// distintos, en cambio, se reusa esa misma instancia y solo se actualiza
/// la vista y los marcadores (`MAP_UPDATE_TEMPLATE`, via
/// `build_update_script`) — Leaflet no permite un segundo `L.map(el)`
/// sobre el mismo elemento. Esto es lo que dejaba pendiente
/// `.claude/STANDARDS.md` desde el issue #4, resuelto en el #20 para poder
/// mover el marcador del conductor en el mapa sin recargar la pantalla.
#[component]
pub fn MapView(
    center_lat: f64,
    center_lng: f64,
    #[props(default = 13)] zoom: u8,
    #[props(default = Vec::new())] markers: Vec<MapMarker>,
    #[props(default)] on_click: Option<EventHandler<(f64, f64)>>,
) -> Element {
    let map_id = use_hook(next_map_id);
    // Distingue el primer render (crea el mapa) de los siguientes (lo
    // actualiza). `peek()` a proposito, no una lectura reactiva: leerlo
    // "de verdad" suscribiria este mismo efecto a sus propios cambios y lo
    // haria correr una segunda vez de inmediato tras `initialized.set(true)`.
    let mut initialized = use_signal(|| false);

    {
        let map_id = map_id.clone();
        use_effect(use_reactive(
            (&center_lat, &center_lng, &zoom, &markers),
            move |(center_lat, center_lng, zoom, markers)| {
                let is_first_run = !*initialized.peek();

                let script = if is_first_run {
                    initialized.set(true);
                    build_init_script(
                        &map_id,
                        center_lat,
                        center_lng,
                        zoom,
                        &markers,
                        on_click.is_some(),
                    )
                } else {
                    build_update_script(&map_id, center_lat, center_lng, zoom, &markers)
                };
                let mut eval = document::eval(&script);

                // El listener de click solo tiene sentido atado al `eval`
                // que corrio `CLICK_HANDLER_TEMPLATE` (el de la
                // inicializacion): las vueltas de actualizacion no vuelven
                // a registrar el handler, asi que su `eval` nunca recibe
                // nada por `dioxus.send`.
                if is_first_run && let Some(on_click) = on_click {
                    spawn(async move {
                        while let Ok(payload) = eval.recv::<MapClickPayload>().await {
                            on_click.call((payload.lat, payload.lng));
                        }
                    });
                }
            },
        ));
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
        let script = build_init_script("motoya-map-0", 4.710989, -74.072092, 15, &[], false);

        assert!(script.contains(r#"document.getElementById("motoya-map-0")"#));
        assert!(script.contains("setView([4.710989, -74.072092], 15)"));
        assert!(script.contains(LEAFLET_CSS_URL));
        assert!(script.contains(LEAFLET_JS_URL));
    }

    #[test]
    fn build_init_script_sets_sri_integrity_and_crossorigin_on_css_and_js() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[], false);

        assert!(script.contains(&format!(r#"cssLink.integrity = "{LEAFLET_CSS_INTEGRITY}""#)));
        assert!(script.contains(&format!(r#"script.integrity = "{LEAFLET_JS_INTEGRITY}""#)));
        assert!(script.contains(r#"cssLink.crossOrigin = "anonymous""#));
        assert!(script.contains(r#"script.crossOrigin = "anonymous""#));
    }

    #[test]
    fn build_init_script_with_no_markers_has_no_marker_statements() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[], false);

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

        let script = build_init_script("motoya-map-1", 4.71, -74.07, 13, &markers, false);

        assert_eq!(script.matches("L.marker(").count(), 2);
        assert!(script.contains(
            "map.__motoyaMarkers.push(L.marker([4.71, -74.07]).bindPopup(\"Origen\").addTo(map));"
        ));
        assert!(script.contains("map.__motoyaMarkers.push(L.marker([4.72, -74.08]).addTo(map));"));
    }

    #[test]
    fn marker_label_with_quotes_is_escaped_and_cannot_break_out_of_the_script() {
        let markers = vec![MapMarker {
            lat: 1.0,
            lng: 2.0,
            label: Some(r#""); alert("xss"); ("#.to_string()),
        }];

        let script = build_init_script("motoya-map-2", 1.0, 2.0, 13, &markers, false);

        // El label queda como un literal JSON escapado (comillas escapadas
        // con `\"`), no como codigo JS suelto que rompa fuera del string.
        assert!(script.contains(r#".bindPopup("\"); alert(\"xss\"); (")"#));
    }

    #[test]
    fn build_init_script_without_on_click_has_no_click_handler() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[], false);

        assert!(!script.contains("map.on(\"click\""));
        assert!(!script.contains("dioxus.send"));
    }

    #[test]
    fn build_init_script_with_on_click_binds_a_click_handler_that_sends_lat_lng() {
        let script = build_init_script("motoya-map-0", 0.0, 0.0, 13, &[], true);

        assert!(script.contains("map.on(\"click\""));
        assert!(script.contains("dioxus.send({ lat: e.latlng.lat, lng: e.latlng.lng });"));
    }

    #[test]
    fn next_map_id_returns_unique_ids() {
        let first = next_map_id();
        let second = next_map_id();

        assert_ne!(first, second);
        assert!(first.starts_with("motoya-map-"));
    }

    #[test]
    fn build_update_script_looks_up_the_existing_map_instance_by_id() {
        let script = build_update_script("motoya-map-0", 4.71, -74.07, 15, &[]);

        assert!(script.contains(r#"window.__motoyaMaps && window.__motoyaMaps["motoya-map-0"]"#));
        assert!(script.contains("setView([4.71, -74.07], 15)"));
    }

    #[test]
    fn build_update_script_does_not_recreate_the_map_or_the_tile_layer() {
        let script = build_update_script("motoya-map-0", 0.0, 0.0, 13, &[]);

        assert!(!script.contains("L.map("));
        assert!(!script.contains("L.tileLayer("));
        assert!(!script.contains("map.on(\"click\""));
    }

    #[test]
    fn build_update_script_removes_the_previous_markers_before_adding_new_ones() {
        let markers = vec![MapMarker {
            lat: 4.72,
            lng: -74.08,
            label: None,
        }];

        let script = build_update_script("motoya-map-0", 4.71, -74.07, 13, &markers);

        assert!(script.contains("map.__motoyaMarkers || []).forEach"));
        assert!(script.contains("map.removeLayer(marker);"));
        assert!(script.contains("map.__motoyaMarkers.push(L.marker([4.72, -74.08]).addTo(map));"));
    }
}
