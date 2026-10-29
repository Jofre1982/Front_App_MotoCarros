//! Pantalla de perfil (pasajero y conductor) — issue #9 — mas la edicion de
//! datos de contacto — issue #10.
//!
//! Consume `GET /api/v1/me` a traves de `ApiClient::me`, que ya trae el
//! reintento con refresh de token (issue #3). Si el refresh tambien falla,
//! la sesion se cierra en vez de mostrar un error suelto en pantalla — es
//! justo el criterio de aceptacion del issue.
//!
//! La edicion (issue #10) reusa esta misma pantalla en vez de ser una
//! pantalla aparte: el formulario se precarga con `session.user()` y, al
//! guardar, llama a `ApiClient::update_profile` (`PATCH /api/v1/me`).
//!
//! La verificacion del celular (issue #69 del backend) tambien vive aca y no
//! en una pantalla propia, mismo criterio que la edicion: es un dato mas de
//! la cuenta, no una seccion de navegacion aparte. `PhoneVerificationSection`
//! solo se muestra cuando `user.phone_verified` es `false`; al confirmar el
//! codigo, `session.set_user()` deja `phone_verified` en `true` y la propia
//! reactividad de Dioxus hace que la seccion desaparezca sin ningun callback
//! explicito.
//!
//! La disponibilidad para mandados (historia #92 del backend, issue #77 de
//! este repo) tambien vive aca, solo visible para cuentas de conductor
//! (`AvailabilitySection`). El backend no expone ningun `GET` para
//! leer el valor actual — el unico endpoint que existe es el `PATCH` que lo
//! cambia (`ApiClient::update_errand_availability`) — asi que la seccion
//! arranca en un estado "desconocido" en vez de asumir `false`, y solo
//! refleja un valor real despues de que el conductor lo cambia el mismo en
//! esta sesion.

use std::sync::Arc;

use dioxus::prelude::*;
use moto_core::api::{
    ApiClient, AuthenticatedRequestError, BecomeDriverError, ConfirmPhoneVerificationError,
    RequestPhoneVerificationError, ServiceActionError, UpdateErrandAvailabilityError,
    UpdateProfileError,
};
use moto_core::location::LocationProvider;
use moto_core::models::{Role, UpdateProfilePayload, User};
use moto_core::state::SessionState;
use moto_core::storage::TokenStorage;

use crate::avatar::Avatar;
use crate::file_picker::{PhotoPicker, PickedFile};

/// Como se describe la cuenta (issue #95): con los dos roles, ambos.
fn roles_label(user: &User) -> &'static str {
    match (user.can_ride(), user.can_drive()) {
        (true, true) => "Pasajero y conductor",
        (_, true) => "Conductor",
        _ => "Pasajero",
    }
}

#[component]
pub fn ProfileScreen() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    // Guarda contra volver a disparar el fetch: el efecto lee `session.token()`
    // en su primera corrida (para mandarlo en la request), lo que lo suscribe
    // a ese signal. Sin esta bandera, el `update_token`/`logout` que dispara
    // el propio fetch reprogramaria el efecto en un loop. Una vez que
    // `has_fetched` es `true`, las corridas siguientes retornan antes de leer
    // `session.token()` de nuevo, asi que dejan de depender de el.
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
            error_message.set(None);

            match api_client.me(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                }
                Err(AuthenticatedRequestError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error_message.set(Some(err.to_string()));
                }
            }

            is_loading.set(false);
        });
    });

    let mut is_editing = use_signal(|| false);

    rsx! {
        div { class: "profile-screen",
            h2 { "Mi perfil" }
            if is_loading() {
                p { "Cargando perfil..." }
            } else if let Some(message) = error_message() {
                p { class: "profile-error", role: "alert", "{message}" }
            } else if let Some(user) = session.user() {
                if is_editing() {
                    EditProfileForm {
                        user,
                        on_saved: move |()| is_editing.set(false),
                        on_cancel: move |()| is_editing.set(false),
                    }
                } else {
                    ProfilePhotoSection { user: user.clone() }
                    dl {
                        dt { "Nombre" }
                        dd { "{user.name}" }
                        dt { "Email" }
                        dd { "{user.email}" }
                        dt { "Telefono" }
                        dd {
                            "{user.phone}"
                            " "
                            if user.phone_verified {
                                span { class: "profile-phone-verified", "(verificado)" }
                            } else {
                                span { class: "profile-phone-unverified", "(sin verificar)" }
                            }
                        }
                        dt { "Rol" }
                        dd { "{roles_label(&user)}" }
                    }
                    if !user.phone_verified {
                        PhoneVerificationSection {}
                    }
                    if session.active_mode() == Some(Role::Driver) {
                        AvailabilitySection {}
                    }
                    if !user.can_drive() {
                        BecomeDriverSection {}
                    }
                    button {
                        r#type: "button",
                        class: "profile-edit-button",
                        onclick: move |_| is_editing.set(true),
                        "Editar perfil"
                    }
                }
            }
        }
    }
}

/// Que decirle a la persona sobre su foto segun `avatar_status`: la de un
/// conductor la revisa el admin antes de que la vean los pasajeros.
fn avatar_status_message(user: &User) -> Option<(&'static str, &'static str)> {
    match user.avatar_status.as_deref() {
        None => Some((
            if user.can_drive() {
                "Sube una foto donde se vea bien tu cara: los pasajeros la ven para reconocerte."
            } else {
                "Sube una foto tuya: el conductor la ve para reconocerte."
            },
            "upload-status",
        )),
        Some("pending") => Some((
            "⏳ Tu foto esta en revision. Los pasajeros la veran cuando el administrador la apruebe.",
            "upload-status",
        )),
        Some("rejected") => Some(("✗ Tu foto fue rechazada.", "price-offer-error")),
        _ => None,
    }
}

/// Foto de perfil (backend #109): se ve la actual y se cambia con la camara
/// o la galeria.
#[component]
fn ProfilePhotoSection(user: User) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();
    let selected = use_signal(|| None::<PickedFile>);
    let mut is_uploading = use_signal(|| false);
    let mut upload_error = use_signal(|| None::<String>);

    let mut selected_for_upload = selected;
    let on_save = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let Some(file) = selected_for_upload() else {
            return;
        };
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_uploading.set(true);
            upload_error.set(None);
            match api_client
                .upload_avatar(&token, &file.name, file.content_type.as_deref(), file.bytes)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                    selected_for_upload.set(None);
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => upload_error.set(Some(err.to_string())),
            }
            is_uploading.set(false);
        });
    };

    rsx! {
        div { class: "profile-photo",
            Avatar { url: user.avatar_url.clone(), name: user.name.clone(), size: 96 }
            if let Some((message, class)) = avatar_status_message(&user) {
                p { class: "{class}", "{message}" }
            }
            if let Some(reason) = &user.avatar_rejection_reason {
                p { class: "price-offer-error", "Motivo: {reason}. Sube otra." }
            }
            PhotoPicker { selected, disabled: is_uploading(), selfie: true }
            if selected().is_some() {
                button {
                    r#type: "button",
                    class: "profile-photo-save",
                    disabled: is_uploading(),
                    onclick: on_save,
                    if is_uploading() { "Subiendo foto..." } else { "Guardar foto de perfil" }
                }
            }
            if let Some(message) = upload_error() {
                p { class: "price-offer-error", role: "alert", "{message}" }
            }
        }
    }
}

/// "Quiero ser conductor" (historia #99 del backend, issue #95): un pasajero
/// activa el modo conductor en su misma cuenta con su licencia
/// (`ApiClient::become_driver`). Al lograrlo, la cuenta queda con los dos
/// roles y la app pasa a modo conductor, donde el menu ya ofrece registrar
/// el vehiculo y subir los documentos.
#[component]
fn BecomeDriverSection() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut is_open = use_signal(|| false);
    let mut license_number = use_signal(String::new);
    let mut error = use_signal(|| None::<BecomeDriverError>);
    let mut is_saving = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();
        let Some(token) = session.token() else {
            return;
        };
        let license = license_number();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_saving.set(true);
            error.set(None);

            match api_client.become_driver(&token, &license).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                    session.set_mode(Role::Driver);
                }
                Err(BecomeDriverError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    error.set(Some(err));
                }
            }

            is_saving.set(false);
        });
    };

    rsx! {
        div { class: "become-driver-section",
            h3 { "¿Tienes un motocarro?" }
            p { "Trabaja como conductor con esta misma cuenta, sin crear otra." }
            if is_open() {
                form { onsubmit: on_submit,
                    label { r#for: "become-driver-license", "Numero de licencia de conduccion" }
                    input {
                        id: "become-driver-license",
                        r#type: "text",
                        autocomplete: "off",
                        disabled: is_saving(),
                        value: "{license_number}",
                        oninput: move |event| license_number.set(event.value()),
                    }
                    p { class: "become-driver-hint",
                        "Despues registras tu vehiculo y subes tus documentos para que te aprueben."
                    }
                    button {
                        r#type: "submit",
                        disabled: is_saving() || license_number().trim().is_empty(),
                        if is_saving() { "Activando..." } else { "Activar modo conductor" }
                    }
                    button {
                        r#type: "button",
                        class: "become-driver-cancel-link",
                        disabled: is_saving(),
                        onclick: move |_| {
                            is_open.set(false);
                            error.set(None);
                        },
                        "Ahora no"
                    }
                    if let Some(err) = error() {
                        p { class: "become-driver-error", role: "alert", "{err}" }
                    }
                }
            } else {
                button {
                    r#type: "button",
                    class: "become-driver-open-button",
                    onclick: move |_| is_open.set(true),
                    "Quiero ser conductor"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EditProfileFormProps {
    user: User,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
}

/// Formulario de edicion de nombre/email/telefono — issue #10. Precargado
/// con `props.user`; al guardar manda solo los campos presentes en el
/// formulario a `ApiClient::update_profile` (PATCH parcial, no PUT — ver
/// `openapi.yaml` de `Back_App_MotoCarros`).
#[component]
fn EditProfileForm(props: EditProfileFormProps) -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut name = use_signal(|| props.user.name.clone());
    let mut email = use_signal(|| props.user.email.clone());
    let mut phone = use_signal(|| props.user.phone.clone());
    let mut update_error = use_signal(|| None::<UpdateProfileError>);
    let mut is_saving = use_signal(|| false);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();

        let Some(token) = session.token() else {
            return;
        };
        let payload = UpdateProfilePayload {
            name: Some(name()),
            email: Some(email()),
            phone: Some(phone()),
        };
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_saving.set(true);
            update_error.set(None);

            match api_client.update_profile(&token, payload).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                    is_saving.set(false);
                    props.on_saved.call(());
                    return;
                }
                Err(UpdateProfileError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    update_error.set(Some(err));
                }
            }

            is_saving.set(false);
        });
    };

    let current_error = update_error();
    let name_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("name"));
    let email_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("email"));
    let phone_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("phone"));
    // Igual que en el registro (issue #6): el mensaje generico solo se
    // muestra cuando el error no trae desglose campo por campo, para no
    // repetir el mismo texto dos veces.
    let has_field_errors = name_error.is_some() || email_error.is_some() || phone_error.is_some();
    let general_message = if has_field_errors {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        form { class: "profile-edit-form", onsubmit: on_submit,
            label { r#for: "profile-edit-name", "Nombre" }
            input {
                id: "profile-edit-name",
                r#type: "text",
                autocomplete: "name",
                disabled: is_saving(),
                value: "{name}",
                oninput: move |event| name.set(event.value()),
            }
            if let Some(message) = &name_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            label { r#for: "profile-edit-email", "Email" }
            input {
                id: "profile-edit-email",
                r#type: "email",
                autocomplete: "email",
                disabled: is_saving(),
                value: "{email}",
                oninput: move |event| email.set(event.value()),
            }
            if let Some(message) = &email_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            label { r#for: "profile-edit-phone", "Telefono" }
            input {
                id: "profile-edit-phone",
                r#type: "tel",
                autocomplete: "tel",
                disabled: is_saving(),
                value: "{phone}",
                oninput: move |event| phone.set(event.value()),
            }
            if let Some(message) = &phone_error {
                p { class: "profile-edit-field-error", role: "alert", "{message}" }
            }
            button { r#type: "submit", disabled: is_saving(),
                if is_saving() {
                    "Guardando..."
                } else {
                    "Guardar cambios"
                }
            }
            button {
                r#type: "button",
                disabled: is_saving(),
                onclick: move |_| props.on_cancel.call(()),
                "Cancelar"
            }
        }
        if let Some(message) = general_message {
            p { class: "profile-edit-error", role: "alert", "{message}" }
        }
    }
}

/// Flujo de verificacion de la cuenta (issue #69 del backend; el codigo llega
/// al correo desde el backend #104, no por SMS). Sin
/// props: lee la cuenta y el token del mismo `SessionState` del contexto, y
/// al confirmar deja el `User` actualizado ahi mismo, mismo criterio que
/// `EditProfileForm`.
///
/// Dos pasos con el mismo signal `code_requested` como bandera: mientras es
/// `false` solo hay un boton para pedir el codigo; una vez pedido, aparece el
/// formulario para confirmarlo. "Reenviar codigo" reutiliza el mismo pedido
/// que el boton inicial — el backend reemplaza cualquier codigo vigente en
/// vez de acumularlo (ver `Back_App_MotoCarros`), asi que no hace falta
/// distinguir un primer pedido de un reenvio.
#[component]
fn PhoneVerificationSection() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let mut session = use_context::<SessionState>();

    let mut code_requested = use_signal(|| false);
    let mut code = use_signal(String::new);
    let mut is_requesting = use_signal(|| false);
    let mut is_confirming = use_signal(|| false);
    let mut request_error = use_signal(|| None::<RequestPhoneVerificationError>);
    let mut confirm_error = use_signal(|| None::<ConfirmPhoneVerificationError>);

    let api_client_for_request = api_client.clone();
    let storage_for_request = storage.clone();

    let on_request = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let api_client = api_client_for_request.clone();
        let storage = storage_for_request.clone();

        spawn(async move {
            is_requesting.set(true);
            request_error.set(None);

            match api_client.request_phone_verification(&token).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    code_requested.set(true);
                }
                Err(RequestPhoneVerificationError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    request_error.set(Some(err));
                }
            }

            is_requesting.set(false);
        });
    };

    let on_confirm = move |event: FormEvent| {
        event.prevent_default();

        let Some(token) = session.token() else {
            return;
        };
        let code_value = code();
        let api_client = api_client.clone();
        let storage = storage.clone();

        spawn(async move {
            is_confirming.set(true);
            confirm_error.set(None);

            match api_client
                .confirm_phone_verification(&token, &code_value)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_user(fetch.data);
                }
                Err(ConfirmPhoneVerificationError::SessionExpired) => {
                    session.logout(storage.as_ref());
                }
                Err(err) => {
                    confirm_error.set(Some(err));
                }
            }

            is_confirming.set(false);
        });
    };

    if !code_requested() {
        let request_message = request_error().as_ref().map(|err| err.to_string());

        return rsx! {
            div { class: "phone-verification-section",
                p { "Tu cuenta todavia no esta verificada." }
                button {
                    r#type: "button",
                    disabled: is_requesting(),
                    onclick: on_request,
                    if is_requesting() {
                        "Enviando codigo..."
                    } else {
                        "Verificar mi cuenta"
                    }
                }
                if let Some(message) = request_message {
                    p { class: "phone-verification-error", role: "alert", "{message}" }
                }
            }
        };
    }

    let current_error = confirm_error();
    let code_error = current_error
        .as_ref()
        .and_then(|err| err.field_message("code"));
    // Igual que en el registro (issue #6): el mensaje generico solo se
    // muestra cuando el error no trae desglose campo por campo.
    let general_message = if code_error.is_some() {
        None
    } else {
        current_error.as_ref().map(|err| err.to_string())
    };

    rsx! {
        div { class: "phone-verification-section",
            p { "Te enviamos un codigo a tu correo (mira tambien en Spam). Ingresalo para verificar tu cuenta." }
            form { onsubmit: on_confirm,
                label { r#for: "phone-verification-code", "Codigo" }
                input {
                    id: "phone-verification-code",
                    r#type: "text",
                    inputmode: "numeric",
                    autocomplete: "one-time-code",
                    disabled: is_confirming(),
                    value: "{code}",
                    oninput: move |event| code.set(event.value()),
                }
                if let Some(message) = &code_error {
                    p { class: "phone-verification-field-error", role: "alert", "{message}" }
                }
                button { r#type: "submit", disabled: is_confirming(),
                    if is_confirming() {
                        "Confirmando..."
                    } else {
                        "Confirmar codigo"
                    }
                }
            }
            button {
                r#type: "button",
                disabled: is_requesting(),
                onclick: on_request,
                if is_requesting() {
                    "Reenviando..."
                } else {
                    "Reenviar codigo"
                }
            }
            if let Some(message) = general_message {
                p { class: "phone-verification-error", role: "alert", "{message}" }
            }
        }
    }
}

/// Interruptores "Recibir viajes" y "Recibir mandados" (pedido del dueno:
/// poder estar dentro de la app sin que lleguen solicitudes). Arrancan en el
/// estado real que trae `GET /me` (`SessionState::set_user`). Con un
/// interruptor apagado, la lista correspondiente viene vacia y no hay aviso.
#[component]
fn AvailabilitySection() -> Element {
    let api_client = use_context::<ApiClient>();
    let storage = use_context::<Arc<dyn TokenStorage>>();
    let location = use_context::<Arc<dyn LocationProvider>>();
    let mut session = use_context::<SessionState>();
    let mut is_saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let rides_api = api_client.clone();
    let rides_storage = storage.clone();
    let toggle_rides = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let turn_on = session.ride_availability() != Some(true);
        let api_client = rides_api.clone();
        let storage = rides_storage.clone();
        let location = location.clone();
        spawn(async move {
            is_saving.set(true);
            error.set(None);
            // La ubicacion ayuda al aviso en tiempo real, pero no es
            // obligatoria: si el celular no la da, igual se activa.
            let point = if turn_on {
                location.current_position().await.ok()
            } else {
                None
            };
            match api_client
                .update_ride_availability(&token, turn_on, point)
                .await
            {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_ride_availability(fetch.data.is_available);
                }
                Err(ServiceActionError::SessionExpired) => session.logout(storage.as_ref()),
                Err(err) => error.set(Some(err.to_string())),
            }
            is_saving.set(false);
        });
    };

    let toggle_errands = move |_| {
        let Some(token) = session.token() else {
            return;
        };
        let turn_on = session.errand_availability() != Some(true);
        let api_client = api_client.clone();
        let storage = storage.clone();
        spawn(async move {
            is_saving.set(true);
            error.set(None);
            match api_client.update_errand_availability(&token, turn_on).await {
                Ok(fetch) => {
                    if let Some(refreshed) = fetch.refreshed_token {
                        session.update_token(refreshed, storage.as_ref());
                    }
                    session.set_errand_availability(fetch.data.is_available_for_errands);
                }
                Err(UpdateErrandAvailabilityError::SessionExpired) => {
                    session.logout(storage.as_ref())
                }
                Err(err) => error.set(Some(err.to_string())),
            }
            is_saving.set(false);
        });
    };

    let rides_on = session.ride_availability() == Some(true);
    let errands_on = session.errand_availability() == Some(true);

    rsx! {
        div { class: "availability-section",
            h3 { "Disponibilidad" }
            p { class: "availability-hint",
                "Apagado, puedes usar la app sin que te lleguen solicitudes."
            }
            button {
                r#type: "button",
                class: if rides_on { "availability-switch availability-switch-on" } else { "availability-switch" },
                role: "switch",
                aria_checked: if rides_on { "true" } else { "false" },
                disabled: is_saving(),
                onclick: toggle_rides,
                span { class: "availability-switch-label", "🛺 Recibir viajes" }
                span { class: "availability-switch-state", if rides_on { "Encendido" } else { "Apagado" } }
            }
            button {
                r#type: "button",
                class: if errands_on { "availability-switch availability-switch-on" } else { "availability-switch" },
                role: "switch",
                aria_checked: if errands_on { "true" } else { "false" },
                disabled: is_saving(),
                onclick: toggle_errands,
                span { class: "availability-switch-label", "🛍️ Recibir mandados y motocargas" }
                span { class: "availability-switch-state", if errands_on { "Encendido" } else { "Apagado" } }
            }
            if let Some(message) = error() {
                p { class: "price-offer-error", role: "alert", "{message}" }
            }
        }
    }
}
