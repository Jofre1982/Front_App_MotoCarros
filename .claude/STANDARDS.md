# STANDARDS — Front App MotoCarros

Decisiones de arquitectura vigentes. Todo el código nuevo (humano o de los agentes
`front_dev`/`front_reviewer`) debe seguir esto salvo que un PR justifique explícitamente
una excepción.

## Estructura del workspace

Cargo workspace con separación dura por crate (no solo por feature flags), para que
el límite entre código agnóstico de plataforma y código específico sea un límite de
compilación, no solo una convención:

```
Front_App_MotoCarros/
├── Cargo.toml                  # workspace
├── crates/
│   ├── core/                   # dominio: modelos, cliente API (/api/v1), estado, lógica de negocio
│   │   └── src/
│   │       ├── api/            # cliente HTTP hacia Back_App_MotoCarros
│   │       ├── models/         # tipos que reflejan el contrato JSON del backend
│   │       └── state/          # signals/stores de Dioxus, agnósticos de renderer
│   ├── ui/                     # componentes y pantallas Dioxus, usan `core`, sin lógica de plataforma
│   ├── web/                    # binario delgado: entrypoint dioxus-web (WASM), wiring de ui+core
│   └── mobile/                 # binario delgado: entrypoint móvil nativo, wiring de ui+core
├── .github/workflows/ci.yml
└── .claude/
    ├── CLAUDE.md
    └── STANDARDS.md
```

Regla dura: `core` y `ui` **no pueden depender** de `web` ni de `mobile`, ni tener
`#[cfg(target_arch = "wasm32")]` disperso salvo con justificación explícita en el PR.
Si una pantalla necesita comportamiento distinto por plataforma, esa diferencia vive
en `web`/`mobile`, inyectada hacia `ui` (trait/callback), no al revés.

## Cliente API

- Todo acceso al backend pasa por `core::api`. Nada de llamadas HTTP sueltas en
  componentes de `ui`.
- Los tipos de `core::models` reflejan el contrato real de `/api/v1` de
  `Back_App_MotoCarros`. Si un endpoint no existe todavía en el backend, no se
  inventa ni se mockea de forma permanente — se documenta el gap en el PR.
- El JWT se maneja en `core` (obtención, renovación, adjunto a requests). Nunca se
  loguea ni se hardcodea. El almacenamiento debe ser explícito y justificado por
  plataforma (web vs. móvil tienen mecanismos distintos de storage seguro).

## Estado

- Signals/stores de Dioxus viven en `core::state`, expuestos a `ui` vía props o
  contexto — no estado global implícito disperso en componentes.
- Cada pantalla maneja explícitamente sus estados de carga/error, no solo el
  camino feliz.

## Testing

- `cargo test` sobre `core` cubre la lógica de negocio y el cliente API (con
  mocks de HTTP, no contra el backend real).
- Los criterios de aceptación del issue son lo que se testea, no solo que
  "compile" o que el camino feliz funcione.
- El renderizado real en `web`/`mobile` no se testea en CI — se valida
  manualmente en navegador/emulador. No es excusa para no testear la lógica que
  sí es testeable en `core`.

## CI (obligatorio en verde antes de cualquier merge)

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- Build del crate `web` (target `wasm32-unknown-unknown`)

El build de `mobile` no corre en el CI por PR (requiere NDK/SDK, es lento y frágil).
Se valida aparte, no bloquea el merge salvo que el PR toque específicamente ese crate.

## Mapas

Proveedor elegido: **Leaflet** + tiles de **OpenStreetMap** (`tile.openstreetmap.org`),
cargados desde CDN (`unpkg.com`) por JS embebido, invocado desde Rust con
`dioxus::document::eval`. Ver `crates/moto_ui/src/map.rs` (`MapView`).

Por que:

- **Costo:** ambos son gratuitos y sin API key. Evita depender de un proveedor de
  pago (Google Maps, Mapbox) o de gestionar credenciales antes de tener volumen real
  de uso — se puede migrar mas adelante si el volumen de tiles lo justifica, sin
  cambiar el contrato de props de `MapView`.
- **Soporte en WASM:** Leaflet es una libreria JS pura que manipula el DOM
  directamente; no hay binding nativo Rust->WASM que mantener. `MapView` solo
  expone un `div` con id y delega la inicializacion a JS via
  `dioxus::document::eval`, que es parte del core de Dioxus (no requiere
  `#[cfg(target_arch = "wasm32")]` disperso).
- **Soporte en renderer movil:** el renderer movil de Dioxus (feature `mobile`,
  `dioxus-desktop`/`wry`) tambien es un webview — el mismo JS de Leaflet corre ahi
  sin cambios cuando se necesite (fuera de alcance de este issue, que solo pide
  que funcione el build `web`).

`MapView` (en `moto_ui`) es puramente presentacional: recibe centro, zoom y una
lista de marcadores por props, y no conoce nada de viajes ni de `core::api`. Quien
lo use decide que representa cada marcador.

Limitacion conocida: la inicializacion del mapa corre una sola vez al montar el
componente (mismo patron que `App::hydrate` en `moto_ui/src/lib.rs`). Actualizar el
mapa reactivamente cuando cambian centro/zoom/marcadores despues del montaje queda
pendiente para la historia que lo consuma con tracking en tiempo real.

## Higiene

- Nunca commitear `.env`, credenciales, tokens ni URLs del backend hardcodeadas
  fuera de configuración explícita.
- `#[allow(clippy::...)]` requiere justificación explícita en el PR, no se usa
  para silenciar sin más.
- `.unwrap()`/`.expect()` solo en tests o casos verdaderamente infalibles.
