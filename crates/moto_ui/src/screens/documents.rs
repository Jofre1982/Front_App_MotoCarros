//! Pantalla de documentos de verificacion del conductor — historia #62 del
//! backend (`Back_App_MotoCarros`).
//!
//! Consulta `GET /api/v1/me/documents` al entrar y ofrece un campo de
//! archivo por cada tipo exigido hoy (`identidad`, `tarjeta_propiedad`,
//! `foto_vehiculo`) para subirlo o reemplazarlo con `POST /api/v1/me/documents`.
//! Elegir el archivo no lo sube solo: hace falta confirmar con el boton
//! "Cargar", para que quede claro si la subida funciono en vez de que
//! parezca que no paso nada. No es una pantalla de aprobacion: eso lo hace
//! un administrador desde fuera de esta app (ver historia tecnica #64 del
//! backend) — acá solo se sube y se ve el estado.
//!
//! El archivo se elige con [`crate::file_picker`] ("Tomar foto" o "Elegir
//! de la galeria") y no con un `<input type="file">` de Dioxus: en Android
//! ese input no abria nada (ver la doc del modulo). Es la misma API en web
//! y en movil nativo, asi que esta pantalla no distingue plataforma (ver
//! `.claude/STANDARDS.md`).

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{ApiClient, AuthenticatedRequestError, UploadDriverDocumentError};
use moto_core::models::{
    DocumentStatus, DocumentType, DriverDocumentStatus, DriverVerification, UploadedDriverDocument,
    VerificationStatus,
};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::file_picker::{PickOutcome, PickSource, PickedFile, pick_file};

fn document_type_label(document_type: DocumentType) -> &'static str {
    match document_type {
        DocumentType::Identidad => "Documento de identidad (cedula, cedula de extranjeria o PTP)",
        DocumentType::TarjetaPropiedad => "Tarjeta de propiedad del motocarro",
        DocumentType::FotoVehiculo => "Foto del vehiculo",
    }
}

fn document_type_slug(document_type: DocumentType) -> &'static str {
    match document_type {
        DocumentType::Identidad => "identidad",
        DocumentType::TarjetaPropiedad => "tarjeta-propiedad",
        DocumentType::FotoVehiculo => "foto-vehiculo",
    }
}

fn status_label(status: Option<DocumentStatus>) -> &'static str {
    match status {
        None => "Sin subir",
        Some(DocumentStatus::Pending) => "Pendiente de revision",
        Some(DocumentStatus::Approved) => "Aprobado",
        Some(DocumentStatus::Rejected) => "Rechazado",
    }
}

fn verification_status_label(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Pending => "Pendiente",
        VerificationStatus::Verified => "Verificado",
        VerificationStatus::Rejected => "Rechazado",
    }
}

#[component]
pub fn DocumentsScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);
    let mut verification = use_signal(|| None::<DriverVerification>);
    // Misma guarda que `ProfileScreen`/`VehicleScreen`: el efecto lee
    // `session.token()` en su primera corrida, y el fetch puede terminar
    // escribiendo en `session` (refresh de token, logout).
    let mut has_fetched = use_signal(|| false);

    use_effect(move || {
        if has_fetched() {
            return;
        }

        let Some(token) = session.token() else {
            return;
        };
        has_fetched.set(true);

        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_loading.set(true);
            load_error.set(None);

            match api_client.get_driver_documents(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    verification.set(Some(fetch.data));
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    load_error.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    });

    // Actualiza en el lugar la fila cuya subida termino, en vez de volver a
    // pedir `GET /me/documents`: la respuesta de `POST /me/documents` ya trae
    // el documento actualizado completo.
    let on_uploaded = move |updated: UploadedDriverDocument| {
        verification.with_mut(|current| {
            let Some(current) = current.as_mut() else {
                return;
            };
            for document in current.documents.iter_mut() {
                if document.document_type == updated.document_type {
                    document.status = Some(updated.status);
                    document.rejection_reason = None;
                    document.uploaded_at = Some(updated.uploaded_at.clone());
                }
            }
        });
    };

    rsx! {
        div { class: "documents-screen",
            h2 { "Mis documentos" }
            if is_loading() {
                p { "Cargando documentos..." }
            } else if let Some(message) = load_error() {
                p { class: "documents-error", role: "alert", "{message}" }
            } else if let Some(current) = verification() {
                p { class: "documents-verification-status",
                    "Estado general: {verification_status_label(current.verification_status)}"
                }
                for document in current.documents.clone() {
                    DocumentRow {
                        key: "{document_type_slug(document.document_type)}",
                        document,
                        on_uploaded,
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DocumentRowProps {
    document: DriverDocumentStatus,
    on_uploaded: EventHandler<UploadedDriverDocument>,
}

#[component]
fn DocumentRow(props: DocumentRowProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_uploading = use_signal(|| false);
    let mut upload_error = use_signal(|| None::<UploadDriverDocumentError>);
    let mut selected_file = use_signal(|| None::<PickedFile>);
    let mut pick_error = use_signal(|| None::<String>);

    let document_type = props.document.document_type;
    let on_uploaded = props.on_uploaded;

    // Elegir el archivo no lo sube: solo lo deja listo para que el
    // conductor confirme con el boton "Cargar" (pedido del usuario viendo
    // la app: sin este paso no quedaba claro si la subida habia funcionado).
    let mut start_pick = move |source: PickSource| {
        upload_error.set(None);
        pick_error.set(None);
        spawn(async move {
            match pick_file(source).await {
                PickOutcome::Picked(file) => selected_file.set(Some(file)),
                PickOutcome::Cancelled => {}
                PickOutcome::Failed(message) => pick_error.set(Some(message)),
            }
        });
    };

    let on_upload_click = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some(file) = selected_file() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_uploading.set(true);
            upload_error.set(None);

            let PickedFile {
                name: file_name,
                content_type: mime_type,
                bytes,
            } = file;

            match api_client
                .upload_driver_document(
                    &token,
                    document_type,
                    &file_name,
                    mime_type.as_deref(),
                    bytes,
                )
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    on_uploaded.call(fetch.data);
                    selected_file.set(None);
                }
                Err(UploadDriverDocumentError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    upload_error.set(Some(err));
                }
            }

            is_uploading.set(false);
        });
    };

    let current_error = upload_error();
    let file_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("file"));
    // Igual que en el resto de los formularios (issue #6): el mensaje
    // generico solo se muestra cuando el error no trae desglose por campo.
    let general_message = if file_error.is_some() {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        div { class: "document-row",
            h3 { "{document_type_label(document_type)}" }
            p { "Estado: {status_label(props.document.status)}" }
            if let Some(reason) = &props.document.rejection_reason {
                p { class: "document-rejection-reason", role: "alert",
                    "Motivo del rechazo: {reason}"
                }
            }
            div { class: "document-pick-buttons",
                button {
                    r#type: "button",
                    class: "document-pick-camera",
                    disabled: is_uploading(),
                    onclick: move |_| start_pick(PickSource::Camera),
                    "📷 Tomar foto"
                }
                button {
                    r#type: "button",
                    class: "document-pick-files",
                    disabled: is_uploading(),
                    onclick: move |_| start_pick(PickSource::Files),
                    "🖼️ Galeria o PDF"
                }
            }
            if let Some(file) = selected_file() {
                p { class: "document-selected-file", "Archivo elegido: {file.name}" }
            }
            if let Some(message) = pick_error() {
                p { class: "document-error", role: "alert", "{message}" }
            }
            button {
                r#type: "button",
                disabled: is_uploading() || selected_file().is_none(),
                onclick: on_upload_click,
                if is_uploading() {
                    "Cargando..."
                } else {
                    "Cargar"
                }
            }
            if let Some(message) = &file_error {
                p { class: "document-field-error", role: "alert", "{message}" }
            }
            if let Some(message) = general_message {
                p { class: "document-error", role: "alert", "{message}" }
            }
        }
    }
}
