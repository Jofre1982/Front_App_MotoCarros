//! Identidad visual de MotoYa (paleta e iconos del Guainia) — issue #54.
//!
//! `GlobalStyles` inyecta la paleta, la tipografia y las reglas de layout
//! compartidas una sola vez, via `document::Style`/`document::Link`
//! (cabecera de la pagina), asi que funciona igual en el build `web`
//! (WASM) y en el renderer movil basado en webview, sin un `index.html`
//! propio — ver `.claude/STANDARDS.md` para el mismo patron aplicado a
//! Leaflet en `map.rs`.
//!
//! Las reglas de layout compartidas (`[class$="-screen"]`, `[class$="-form"]`,
//! etc.) seleccionan por el sufijo de nombre de clase que cada pantalla ya
//! usa (ver `crates/moto_ui/src/screens/*.rs`), en vez de listar cada una de
//! las ~90 clases existentes una por una: la convencion de nombres ya es
//! consistente en todo el codigo (`*-screen`, `*-form`, `*-list`, `*-row`,
//! `*-error`, `*-empty`, `*-link`, `*-panel`, `*-status`), asi que una
//! pantalla nueva que la siga queda estilada automaticamente.

use dioxus::prelude::*;

const GLOBAL_CSS: &str = r#"
:root {
    --selva: #14231C;
    --selva-2: #1C3327;
    --rio: #1F5C57;
    --cerro: #C97244;
    --cerro-dim: #9A5732;
    --flor: #E23178;
    --roca: #3A2116;
    --arena: #F3EDE2;
    --arena-dim: #B9C4BB;

    --motoya-font-heading: "Fraunces", serif;
    --motoya-font-body: "Manrope", sans-serif;
}

*, *::before, *::after {
    box-sizing: border-box;
}

html, body {
    margin: 0;
    min-height: 100%;
    background: var(--selva);
    color: var(--arena);
}

body {
    font-family: var(--motoya-font-body);
    padding: 1rem;
    line-height: 1.4;
}

h1, h2, h3, h4 {
    font-family: var(--motoya-font-heading);
    color: var(--cerro);
    margin: 0 0 0.5rem;
}

a {
    color: var(--flor);
}

label {
    color: var(--arena-dim);
    font-size: 0.9rem;
}

button {
    font-family: var(--motoya-font-body);
    font-size: 1rem;
    background: var(--rio);
    color: var(--arena);
    border: 1px solid var(--roca);
    border-radius: 0.5rem;
    padding: 0.6rem 1rem;
    cursor: pointer;
    transition: background-color 0.15s ease, opacity 0.15s ease;
}

button:hover:not(:disabled) {
    background: var(--cerro-dim);
}

button:disabled {
    opacity: 0.55;
    cursor: default;
    background: var(--roca);
}

input, select, textarea {
    font-family: var(--motoya-font-body);
    font-size: 1rem;
    background: var(--selva-2);
    color: var(--arena);
    border: 1px solid var(--roca);
    border-radius: 0.375rem;
    padding: 0.5rem 0.6rem;
    width: 100%;
}

input::placeholder, textarea::placeholder {
    color: var(--arena-dim);
}

/* Layout compartido por convencion de sufijo — ver comentario del modulo. */
[class$="-screen"] {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 480px;
    margin: 0 auto;
}

[class$="-form"] {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
}

[class$="-list"] {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
}

[class$="-row"],
[class$="-result"],
[class$="-summary"],
[class*="-panel"] {
    background: var(--selva-2);
    border: 1px solid var(--roca);
    border-radius: 0.75rem;
    padding: 0.9rem;
}

[class$="-empty"] {
    color: var(--arena-dim);
    font-style: italic;
}

[class*="-error"] {
    color: var(--flor);
}

[class*="-status"] {
    font-size: 0.9rem;
    color: var(--arena-dim);
}

/* Los "*-link" son <button> secundarios (navegacion entre pantallas), no
   <a> — se ven como enlace de texto para distinguirse de la accion
   primaria de cada pantalla. La combinacion `button[class$="-link"]` pesa
   mas que el `button` de arriba, asi que gana sin depender del orden. */
button[class$="-link"] {
    background: none;
    border: none;
    color: var(--flor);
    text-decoration: underline;
    padding: 0.25rem 0;
}

button[class$="-link"]:hover:not(:disabled) {
    background: none;
    color: var(--cerro);
}

/* Cancelar/descartar: menos enfasis que la accion primaria de la pantalla. */
button[class*="-cancel"],
button[class*="-dismiss"] {
    background: transparent;
    color: var(--arena-dim);
    border-color: var(--roca);
}

button[class*="-cancel"]:hover:not(:disabled),
button[class*="-dismiss"]:hover:not(:disabled) {
    background: var(--roca);
    color: var(--arena);
}

.home-nav {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
}

.motoya-map-view {
    border-radius: 0.75rem;
    overflow: hidden;
    border: 1px solid var(--roca);
}

.motoya-brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-family: var(--motoya-font-heading);
    color: var(--cerro);
}
"#;

/// Inyecta la paleta/tipografia/layout de MotoYa en la cabecera de la
/// pagina. Se renderiza una sola vez desde `App` (raiz agnostica de
/// plataforma en `lib.rs`), asi que cubre tanto `web` como `mobile` sin
/// wiring adicional por binario.
///
/// Google Fonts se carga con el patron `preconnect` + hoja de estilos
/// recomendado por Google (dos origenes: `fonts.googleapis.com` sirve el
/// CSS, `fonts.gstatic.com` los archivos de fuente reales).
#[component]
pub fn GlobalStyles() -> Element {
    rsx! {
        document::Link {
            rel: "preconnect",
            href: "https://fonts.googleapis.com",
        }
        document::Link {
            rel: "preconnect",
            href: "https://fonts.gstatic.com",
            crossorigin: "anonymous",
        }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Fraunces:ital,wght@0,600;0,700;1,600&family=Manrope:wght@400;500;700&display=swap",
        }
        document::Style { "{GLOBAL_CSS}" }
    }
}
