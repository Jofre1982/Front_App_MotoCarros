//! Elegir un archivo (o tomar una foto) sin depender del `<input type="file">`
//! que pinta Dioxus.
//!
//! Por que existe: en el shell nativo (Android), Dioxus intercepta el click
//! sobre cualquier `<input type="file">` que el mismo renderizo y en su
//! lugar pide un dialogo nativo de escritorio (`rfd`), que en Android no
//! existe — `dioxus-desktop` devuelve una lista vacia y el boton "no hace
//! nada" (reportado probando el APK). El WebView de Android si sabe abrir
//! la galeria/camara por su cuenta (`onShowFileChooser` de wry), solo hay que
//! dejarlo: este modulo crea el input desde JavaScript colgado directo de
//! `<body>`, fuera del arbol de Dioxus, asi el interceptor no lo reconoce y
//! el WebView muestra su selector normal. En la web funciona igual.
//!
//! Las fotos se achican en el mismo JavaScript (lado mayor 1600 px, JPEG
//! 0.85) antes de mandarlas a Rust: una foto de camara pasa facil los 5 MB
//! que acepta el backend, y una foto de documento no necesita mas.

use base64::Engine;
use dioxus::prelude::*;
use serde::Deserialize;

/// Archivo listo para subir.
#[derive(Debug, Clone, PartialEq)]
pub struct PickedFile {
    pub name: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Como termino el intento de elegir un archivo.
#[derive(Debug, Clone, PartialEq)]
pub enum PickOutcome {
    Picked(PickedFile),
    /// El usuario cerro el selector sin elegir nada.
    Cancelled,
    Failed(String),
}

/// De donde sale el archivo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickSource {
    /// Abre la camara trasera directamente (si el celular lo permite; si no,
    /// el WebView cae al selector normal).
    Camera,
    /// Galeria o archivos del celular: fotos y PDF.
    Files,
}

#[derive(Deserialize)]
struct PickMessage {
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    content_type: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

/// Traduce lo que manda el JavaScript de [`pick_file`]. Separado para poder
/// probarlo sin WebView.
pub fn parse_pick_message(value: serde_json::Value) -> PickOutcome {
    let Ok(message) = serde_json::from_value::<PickMessage>(value) else {
        return PickOutcome::Failed("Respuesta inesperada al elegir el archivo.".to_string());
    };
    if message.cancelled {
        return PickOutcome::Cancelled;
    }
    if let Some(error) = message.error {
        return PickOutcome::Failed(error);
    }
    let Some(data) = message.data else {
        return PickOutcome::Cancelled;
    };
    match base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) {
        Ok(bytes) if !bytes.is_empty() => PickOutcome::Picked(PickedFile {
            name: message
                .name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "documento.jpg".to_string()),
            content_type: message.content_type.filter(|kind| !kind.is_empty()),
            bytes,
        }),
        Ok(_) => PickOutcome::Failed("El archivo elegido esta vacio.".to_string()),
        Err(_) => PickOutcome::Failed("No se pudo leer el archivo elegido.".to_string()),
    }
}

const PICK_SCRIPT: &str = r#"
const opts = __OPTS__;
const input = document.createElement('input');
input.type = 'file';
input.accept = opts.accept;
if (opts.camera) { input.setAttribute('capture', 'environment'); }
input.style.position = 'fixed';
input.style.left = '-1000px';
input.style.opacity = '0';
document.body.appendChild(input);
let done = false;
const finish = (msg) => { if (done) return; done = true; input.remove(); dioxus.send(msg); };
const readAsBase64 = (blob) => new Promise((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result).split(',')[1] || '');
  reader.onerror = () => reject(reader.error);
  reader.readAsDataURL(blob);
});
const shrink = (file) => new Promise((resolve) => {
  if (!/^image\/(jpeg|png)$/.test(file.type)) { resolve(null); return; }
  const url = URL.createObjectURL(file);
  const img = new Image();
  img.onload = () => {
    const max = 1600;
    const scale = Math.min(1, max / Math.max(img.width, img.height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(img.width * scale);
    canvas.height = Math.round(img.height * scale);
    canvas.getContext('2d').drawImage(img, 0, 0, canvas.width, canvas.height);
    URL.revokeObjectURL(url);
    canvas.toBlob((blob) => resolve(blob), 'image/jpeg', 0.85);
  };
  img.onerror = () => { URL.revokeObjectURL(url); resolve(null); };
  img.src = url;
});
input.addEventListener('change', async () => {
  const file = input.files && input.files[0];
  if (!file) { finish({ cancelled: true }); return; }
  try {
    const small = await shrink(file);
    if (small) {
      const base = (file.name || 'foto').replace(/\.[^.]*$/, '');
      finish({ name: base + '.jpg', type: 'image/jpeg', data: await readAsBase64(small) });
    } else {
      finish({ name: file.name, type: file.type, data: await readAsBase64(file) });
    }
  } catch (e) {
    finish({ error: 'No se pudo leer el archivo elegido.' });
  }
});
input.addEventListener('cancel', () => finish({ cancelled: true }));
input.click();
"#;

/// Abre el selector del sistema y espera el archivo. Si el WebView no avisa
/// que se cancelo (navegadores viejos), el future simplemente queda
/// pendiente: por eso las pantallas no deben bloquear la UI mientras tanto.
pub async fn pick_file(source: PickSource) -> PickOutcome {
    let (accept, camera) = match source {
        PickSource::Camera => ("image/*", true),
        PickSource::Files => ("image/jpeg,image/png,application/pdf", false),
    };
    // Las opciones van dentro del script (y no por `send`) para que el
    // `click()` ocurra sin una ida y vuelta extra: el WebView solo abre el
    // selector mientras dure la "activacion" del toque del usuario.
    let options = serde_json::json!({ "accept": accept, "camera": camera });
    let mut eval = document::eval(&PICK_SCRIPT.replace("__OPTS__", &options.to_string()));
    match eval.recv::<serde_json::Value>().await {
        Ok(value) => parse_pick_message(value),
        Err(_) => PickOutcome::Failed("No se pudo abrir el selector de archivos.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodifica_un_archivo_elegido() {
        let outcome = parse_pick_message(json!({
            "name": "cedula.jpg",
            "type": "image/jpeg",
            "data": "aG9sYQ==",
        }));
        assert_eq!(
            outcome,
            PickOutcome::Picked(PickedFile {
                name: "cedula.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: b"hola".to_vec(),
            })
        );
    }

    #[test]
    fn sin_nombre_ni_tipo_usa_valores_por_defecto() {
        let outcome = parse_pick_message(json!({ "name": "", "type": "", "data": "aG9sYQ==" }));
        let PickOutcome::Picked(file) = outcome else {
            panic!("esperaba un archivo");
        };
        assert_eq!(file.name, "documento.jpg");
        assert_eq!(file.content_type, None);
    }

    #[test]
    fn reconoce_la_cancelacion() {
        assert_eq!(
            parse_pick_message(json!({ "cancelled": true })),
            PickOutcome::Cancelled
        );
    }

    #[test]
    fn propaga_el_error_del_lector() {
        assert_eq!(
            parse_pick_message(json!({ "error": "No se pudo leer el archivo elegido." })),
            PickOutcome::Failed("No se pudo leer el archivo elegido.".to_string())
        );
    }

    #[test]
    fn base64_invalido_o_vacio_falla() {
        assert!(matches!(
            parse_pick_message(json!({ "name": "a.pdf", "data": "%%%" })),
            PickOutcome::Failed(_)
        ));
        assert!(matches!(
            parse_pick_message(json!({ "name": "a.pdf", "data": "" })),
            PickOutcome::Failed(_)
        ));
    }

    #[test]
    fn mensaje_raro_falla_sin_entrar_en_panico() {
        assert!(matches!(
            parse_pick_message(json!("hola")),
            PickOutcome::Failed(_)
        ));
    }
}
