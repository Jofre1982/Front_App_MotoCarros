//! Cliente HTTP hacia `Back_App_MotoCarros` (`/api/v1`).
//!
//! Los endpoints reales se agregan issue por issue, reflejando el contrato
//! que expone el backend en cada momento (ver `openapi.yaml` de
//! `Back_App_MotoCarros`).

use crate::models::{
    ApiErrorBody, AuthToken, AuthenticatedUser, BroadcastAuthPayload, BroadcastAuthResponse,
    Coordinates, DataEnvelope, LoginPayload, RegisterDriverPayload, RegisterPassengerPayload, Ride,
    RideCancellation, RideEstimate, RideEstimateRequestPayload, RideRequestPayload,
    UpdateProfilePayload, User,
};

#[cfg(test)]
use crate::models::Role;

#[derive(Debug, Clone)]
pub struct ApiClient {
    pub base_url: String,
    http: reqwest::Client,
}

/// Fallos posibles de `POST /api/v1/auth/login`.
///
/// `InvalidCredentials` cubre tanto password incorrecta como email
/// inexistente: el backend responde exactamente el mismo 401 para ambos
/// casos a proposito (ver `openapi.yaml`), asi que el cliente no puede ni
/// debe distinguirlos.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginError {
    EmptyFields,
    InvalidCredentials(String),
    Validation(ApiErrorBody),
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginError::EmptyFields => write!(f, "Ingresa tu email y tu contrasena."),
            LoginError::InvalidCredentials(message) => write!(f, "{message}"),
            LoginError::Validation(body) => write!(f, "{}", body.message),
            LoginError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            LoginError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for LoginError {}

/// Fallos posibles de `POST /api/v1/auth/register/passenger` y de
/// `POST /api/v1/auth/register/driver` (issue #7) — comparten forma porque
/// ambos registros validan el mismo tipo de errores, salvo
/// `InvalidLicenseNumber`, que solo aplica al registro de conductor.
///
/// `InvalidEmail`/`InvalidPhone`/`InvalidLicenseNumber` se detectan en el
/// cliente antes de mandar la request (formato basico), sin duplicar las
/// reglas completas del backend (unicidad, normalizacion) — esas siguen
/// viajando como `Validation` en un 422 (ver `openapi.yaml` de
/// `Back_App_MotoCarros`).
#[derive(Debug, Clone, PartialEq)]
pub enum RegisterError {
    EmptyFields,
    InvalidEmail,
    InvalidPhone,
    /// `license_number` no cumple el formato que exige el backend
    /// (`RegisterDriverRequest`, regex `^[A-Z0-9-]{5,50}$`).
    InvalidLicenseNumber,
    Validation(ApiErrorBody),
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterError::EmptyFields => write!(f, "Completa todos los campos."),
            RegisterError::InvalidEmail => write!(f, "Ingresa un email valido."),
            RegisterError::InvalidPhone => {
                write!(f, "Ingresa un telefono valido (7 a 15 digitos).")
            }
            RegisterError::InvalidLicenseNumber => {
                write!(
                    f,
                    "Ingresa un numero de documento valido (5 a 50 caracteres, solo letras mayusculas, numeros y guiones)."
                )
            }
            RegisterError::Validation(body) => write!(f, "{}", body.message),
            RegisterError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            RegisterError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for RegisterError {}

impl RegisterError {
    /// Mensaje de validacion especifico para `field` (p. ej. `"email"`), tal
    /// como lo devuelve el backend campo por campo en `ApiErrorBody.errors`
    /// de un 422 (ver `openapi.yaml`). `None` si el error no es de
    /// validacion o el backend no reporto ese campo.
    pub fn field_message(&self, field: &str) -> Option<String> {
        match self {
            RegisterError::Validation(body) => body
                .errors
                .as_ref()
                .and_then(|errors| errors.get(field))
                .and_then(|messages| messages.first())
                .cloned(),
            _ => None,
        }
    }
}

/// Formato basico (no unicidad, eso lo valida el backend): una arroba, con
/// algo antes y un dominio con un punto despues.
fn is_valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && !domain.is_empty() && domain.contains('.') && !email.contains(' ')
}

/// Formato E.164 (ver `openapi.yaml`): `+` opcional seguido de 7 a 15
/// digitos.
fn is_valid_phone(phone: &str) -> bool {
    let digits = phone.strip_prefix('+').unwrap_or(phone);
    (7..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit())
}

/// Mismo regex que `RegisterDriverRequest` en el backend
/// (`^[A-Z0-9-]{5,50}$`, ver issue #7): 5 a 50 caracteres, solo letras
/// mayusculas ASCII, digitos y guiones.
fn is_valid_license_number(license_number: &str) -> bool {
    (5..=50).contains(&license_number.len())
        && license_number
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

/// Fallos posibles de `POST /api/v1/auth/refresh`.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshError {
    /// El token esta vacio, es ilegible, ya esta en la blacklist, o supero
    /// la ventana de refresh (`JWT_REFRESH_TTL`) — en cualquiera de esos
    /// casos el backend responde 401 y ya no hay forma de renovar: hay que
    /// volver a pedir credenciales.
    Unauthorized(String),
    /// El endpoint esta limitado a 10 requests/minuto por IP (ver
    /// `openapi.yaml`); no es motivo para cerrar la sesion, el caller puede
    /// reintentar mas tarde.
    RateLimited,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for RefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::Unauthorized(message) => write!(f, "{message}"),
            RefreshError::RateLimited => {
                write!(
                    f,
                    "Demasiados intentos de renovar la sesion. Intenta mas tarde."
                )
            }
            RefreshError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            RefreshError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for RefreshError {}

/// Fallos posibles de `POST /api/v1/auth/logout`.
///
/// El caller (`SessionState::logout`, ver issue #8) nunca deja de cerrar la
/// sesion local por estos errores — solo le sirven para decidir si vale la
/// pena loguearlos.
#[derive(Debug, Clone, PartialEq)]
pub enum LogoutError {
    /// El token ya no era valido para el guard (`auth:api`): no hay nada que
    /// cerrar del lado del backend, pero la sesion local igual se limpia.
    Unauthorized,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for LogoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogoutError::Unauthorized => write!(f, "La sesion ya no era valida."),
            LogoutError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            LogoutError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for LogoutError {}

/// Resultado de una request autenticada que pudo haber renovado el token en
/// el camino.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthenticatedFetch<T> {
    pub data: T,
    /// `Some` solo si el primer intento respondio 401 y hubo que renovar el
    /// token antes de reintentar — el caller (dueno del `SessionState`) debe
    /// persistirlo con `SessionState::update_token`.
    pub refreshed_token: Option<AuthToken>,
}

/// Fallos posibles de una request autenticada con reintento automatico.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthenticatedRequestError {
    /// El token vencio y el intento de renovarlo tambien fallo (ventana de
    /// refresh superada, token en blacklist, etc.): no queda otra que forzar
    /// logout y volver al login.
    SessionExpired,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for AuthenticatedRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthenticatedRequestError::SessionExpired => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            AuthenticatedRequestError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            AuthenticatedRequestError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for AuthenticatedRequestError {}

/// Fallos posibles de `PATCH /api/v1/me`.
///
/// `NoFields` se detecta en el cliente antes de mandar la request: un PATCH
/// sin ningun campo no tiene nada que actualizar (ver issue #10).
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateProfileError {
    NoFields,
    InvalidEmail,
    InvalidPhone,
    Validation(ApiErrorBody),
    SessionExpired,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for UpdateProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateProfileError::NoFields => write!(f, "No hay ningun cambio para guardar."),
            UpdateProfileError::InvalidEmail => write!(f, "Ingresa un email valido."),
            UpdateProfileError::InvalidPhone => {
                write!(f, "Ingresa un telefono valido (7 a 15 digitos).")
            }
            UpdateProfileError::Validation(body) => write!(f, "{}", body.message),
            UpdateProfileError::SessionExpired => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            UpdateProfileError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            UpdateProfileError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for UpdateProfileError {}

impl UpdateProfileError {
    /// Mensaje de validacion especifico para `field`, igual que
    /// `RegisterError::field_message`.
    pub fn field_message(&self, field: &str) -> Option<String> {
        match self {
            UpdateProfileError::Validation(body) => body
                .errors
                .as_ref()
                .and_then(|errors| errors.get(field))
                .and_then(|messages| messages.first())
                .cloned(),
            _ => None,
        }
    }
}

/// Fallos posibles de `POST /api/v1/rides/estimate` (issue #14, consumido
/// desde la app en issue #13).
///
/// `Validation` cubre tanto un 422 de forma (coordenada fuera de rango) como
/// uno de negocio (el proveedor de mapas no encontro ruta entre origen y
/// destino, zona sin cobertura): el backend responde el mismo status para
/// ambos casos a proposito (ver `openapi.yaml`), asi que el cliente no puede
/// ni debe distinguirlos — el mensaje que trae el body ya es explicito sobre
/// cual de los dos paso.
#[derive(Debug, Clone, PartialEq)]
pub enum EstimateRideError {
    Validation(ApiErrorBody),
    SessionExpired,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for EstimateRideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EstimateRideError::Validation(body) => write!(f, "{}", body.message),
            EstimateRideError::SessionExpired => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            EstimateRideError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            EstimateRideError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for EstimateRideError {}

/// Fallos posibles de `POST /api/v1/rides` (issue #14).
///
/// `Validation` cubre tanto un 422 de forma (coordenada invalida) como uno de
/// negocio (el pasajero ya tiene un viaje activo, o el proveedor de mapas no
/// encontro ruta): el backend responde el mismo status para ambos casos a
/// proposito (ver `openapi.yaml`), asi que el cliente no puede ni debe
/// distinguirlos — el mensaje que trae el body ya es explicito sobre cual de
/// los dos paso.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestRideError {
    /// La cuenta no es de pasajero (`RidePolicy` en el backend). Solicitar un
    /// viaje es una operacion exclusiva del rol pasajero.
    Forbidden,
    Validation(ApiErrorBody),
    SessionExpired,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for RequestRideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestRideError::Forbidden => {
                write!(f, "Esta cuenta no puede solicitar viajes.")
            }
            RequestRideError::Validation(body) => write!(f, "{}", body.message),
            RequestRideError::SessionExpired => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            RequestRideError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            RequestRideError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for RequestRideError {}

/// Fallos posibles de `POST /api/v1/rides/{ride}/cancel` (issue #15).
///
/// El mismo endpoint sirve tanto para que el pasajero cancele (historias
/// #16/#22) como para que el conductor asignado libere el viaje (historia
/// #23) — el cliente no distingue esos dos casos, los resuelve el backend
/// segun quien llama (ver `openapi.yaml`). `Validation` cubre que el viaje ya
/// no este en un estado cancelable (`in_progress`, `completed` o ya
/// `cancelled`).
#[derive(Debug, Clone, PartialEq)]
pub enum CancelRideError {
    /// El viaje no le pertenece al pasajero autenticado ni esta asignado al
    /// conductor autenticado.
    Forbidden,
    /// No existe ningun viaje con ese id.
    NotFound,
    Validation(ApiErrorBody),
    SessionExpired,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for CancelRideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CancelRideError::Forbidden => {
                write!(f, "Este viaje no te pertenece.")
            }
            CancelRideError::NotFound => write!(f, "El viaje ya no existe."),
            CancelRideError::Validation(body) => write!(f, "{}", body.message),
            CancelRideError::SessionExpired => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            CancelRideError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            CancelRideError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for CancelRideError {}

/// Fallos posibles de `POST /api/v1/broadcasting/auth` (issue #5).
///
/// A diferencia del resto de las requests autenticadas, esta nunca reintenta
/// con un refresh de token: el endpoint exige explicitamente un access token
/// vigente (ver `openapi.yaml`), asi que un 401 aca es directamente un fallo
/// de autenticacion, no una senal de "token vencido, renovar y reintentar".
#[derive(Debug, Clone, PartialEq)]
pub enum BroadcastAuthError {
    Unauthorized,
    /// El usuario autenticado no participa de la entidad del canal (no es el
    /// conductor de `driver.{id}`, o no participa del viaje de `ride.{id}`).
    Forbidden,
    Network(String),
    Unexpected(u16),
}

impl std::fmt::Display for BroadcastAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BroadcastAuthError::Unauthorized => {
                write!(f, "La sesion expiro. Inicia sesion de nuevo.")
            }
            BroadcastAuthError::Forbidden => {
                write!(f, "No tienes permiso para suscribirte a este canal.")
            }
            BroadcastAuthError::Network(_) => {
                write!(
                    f,
                    "No se pudo conectar con el servidor. Revisa tu conexion."
                )
            }
            BroadcastAuthError::Unexpected(status) => {
                write!(f, "Ocurrio un error inesperado (codigo {status}).")
            }
        }
    }
}

impl std::error::Error for BroadcastAuthError {}

enum GetOutcome<T> {
    Success(T),
    Unauthorized,
}

enum PatchOutcome<T> {
    Success(T),
    Unauthorized,
    Validation(ApiErrorBody),
}

enum PostOutcome<T> {
    Success(T),
    Unauthorized,
    Validation(ApiErrorBody),
}

enum PostRideOutcome<T> {
    Success(T),
    Unauthorized,
    Forbidden,
    Validation(ApiErrorBody),
}

enum CancelRideOutcome {
    Success(RideCancellation),
    Unauthorized,
    Forbidden,
    NotFound,
    Validation(ApiErrorBody),
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    /// `POST /api/v1/auth/login`.
    ///
    /// No manda ningun rol: el backend lo determina por la cuenta y viaja de
    /// vuelta en `user.role`.
    pub async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<AuthenticatedUser, LoginError> {
        if email.trim().is_empty() || password.is_empty() {
            return Err(LoginError::EmptyFields);
        }

        let url = format!("{}/api/v1/auth/login", self.base_url);
        let payload = LoginPayload {
            email: email.to_string(),
            password: password.to_string(),
        };

        let response = self
            .http
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| LoginError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<AuthenticatedUser> = response
                .json()
                .await
                .map_err(|err| LoginError::Network(err.to_string()))?;
            return Ok(envelope.data);
        }

        match status.as_u16() {
            401 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| LoginError::Network(err.to_string()))?;
                Err(LoginError::InvalidCredentials(body.message))
            }
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| LoginError::Network(err.to_string()))?;
                Err(LoginError::Validation(body))
            }
            other => Err(LoginError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/auth/register/passenger`.
    ///
    /// El rol no se manda: lo fija el endpoint. Si la respuesta es exitosa,
    /// la cuenta queda con sesion iniciada igual que tras un login (mismo
    /// shape `AuthenticatedUser`).
    pub async fn register_passenger(
        &self,
        name: &str,
        email: &str,
        phone: &str,
        password: &str,
    ) -> Result<AuthenticatedUser, RegisterError> {
        let name = name.trim();
        let email = email.trim();
        let phone = phone.trim();

        if name.is_empty() || email.is_empty() || phone.is_empty() || password.is_empty() {
            return Err(RegisterError::EmptyFields);
        }
        if !is_valid_email(email) {
            return Err(RegisterError::InvalidEmail);
        }
        if !is_valid_phone(phone) {
            return Err(RegisterError::InvalidPhone);
        }

        let url = format!("{}/api/v1/auth/register/passenger", self.base_url);
        let payload = RegisterPassengerPayload {
            name: name.to_string(),
            email: email.to_string(),
            phone: phone.to_string(),
            password: password.to_string(),
        };

        let response = self
            .http
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| RegisterError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<AuthenticatedUser> = response
                .json()
                .await
                .map_err(|err| RegisterError::Network(err.to_string()))?;
            return Ok(envelope.data);
        }

        match status.as_u16() {
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| RegisterError::Network(err.to_string()))?;
                Err(RegisterError::Validation(body))
            }
            other => Err(RegisterError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/auth/register/driver` — issue #7.
    ///
    /// El rol no se manda: lo fija el endpoint. `license_number` es
    /// obligatorio para el backend (`RegisterDriverRequest`) — sin el,
    /// siempre responde 422, sin importar el resto del formulario. Si la
    /// respuesta es exitosa, la cuenta queda con sesion iniciada igual que
    /// tras un login (mismo shape `AuthenticatedUser`).
    pub async fn register_driver(
        &self,
        name: &str,
        email: &str,
        phone: &str,
        password: &str,
        license_number: &str,
    ) -> Result<AuthenticatedUser, RegisterError> {
        let name = name.trim();
        let email = email.trim();
        let phone = phone.trim();
        let license_number = license_number.trim();

        if name.is_empty()
            || email.is_empty()
            || phone.is_empty()
            || password.is_empty()
            || license_number.is_empty()
        {
            return Err(RegisterError::EmptyFields);
        }
        if !is_valid_email(email) {
            return Err(RegisterError::InvalidEmail);
        }
        if !is_valid_phone(phone) {
            return Err(RegisterError::InvalidPhone);
        }
        if !is_valid_license_number(license_number) {
            return Err(RegisterError::InvalidLicenseNumber);
        }

        let url = format!("{}/api/v1/auth/register/driver", self.base_url);
        let payload = RegisterDriverPayload {
            name: name.to_string(),
            email: email.to_string(),
            phone: phone.to_string(),
            password: password.to_string(),
            license_number: license_number.to_string(),
        };

        let response = self
            .http
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| RegisterError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<AuthenticatedUser> = response
                .json()
                .await
                .map_err(|err| RegisterError::Network(err.to_string()))?;
            return Ok(envelope.data);
        }

        match status.as_u16() {
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| RegisterError::Network(err.to_string()))?;
                Err(RegisterError::Validation(body))
            }
            other => Err(RegisterError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/auth/refresh`.
    ///
    /// Acepta el access token vigente aunque ya haya expirado, siempre que
    /// siga dentro de la ventana de refresh que define el backend (ver
    /// `openapi.yaml` de `Back_App_MotoCarros`). No manda body, solo el
    /// `Authorization: Bearer` con el token a renovar.
    pub async fn refresh(&self, access_token: &str) -> Result<AuthToken, RefreshError> {
        let url = format!("{}/api/v1/auth/refresh", self.base_url);

        let response = self
            .http
            .post(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| RefreshError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<AuthToken> = response
                .json()
                .await
                .map_err(|err| RefreshError::Network(err.to_string()))?;
            return Ok(envelope.data);
        }

        match status.as_u16() {
            401 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| RefreshError::Network(err.to_string()))?;
                Err(RefreshError::Unauthorized(body.message))
            }
            429 => Err(RefreshError::RateLimited),
            other => Err(RefreshError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/auth/logout`.
    ///
    /// Invalida `access_token` del lado del backend (204 sin cuerpo). El
    /// caller debe limpiar `SessionState` localmente sin importar el
    /// resultado (ver issue #8) — cerrar sesion nunca debe dejar al usuario
    /// atrapado por un error de red o un token ya vencido.
    pub async fn logout(&self, access_token: &str) -> Result<(), LogoutError> {
        let url = format!("{}/api/v1/auth/logout", self.base_url);

        let response = self
            .http
            .post(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| LogoutError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            return Ok(());
        }

        match status.as_u16() {
            401 => Err(LogoutError::Unauthorized),
            other => Err(LogoutError::Unexpected(other)),
        }
    }

    /// GET autenticado hacia `path` (p.ej. `/api/v1/me`), reintentando una
    /// vez con un token renovado si el backend responde 401 por token
    /// vencido. Pensado para que las pantallas que consuman endpoints
    /// protegidos (ver issue #9 y siguientes) no tengan que reimplementar
    /// este reintento cada una — ver issue #3.
    pub async fn get_authenticated<T>(
        &self,
        path: &str,
        token: &AuthToken,
    ) -> Result<AuthenticatedFetch<T>, AuthenticatedRequestError>
    where
        T: serde::de::DeserializeOwned,
    {
        if let GetOutcome::Success(data) = self.get_with_token(path, &token.access_token).await? {
            return Ok(AuthenticatedFetch {
                data,
                refreshed_token: None,
            });
        }

        let renewed = self
            .refresh(&token.access_token)
            .await
            .map_err(|_| AuthenticatedRequestError::SessionExpired)?;

        match self.get_with_token(path, &renewed.access_token).await? {
            GetOutcome::Success(data) => Ok(AuthenticatedFetch {
                data,
                refreshed_token: Some(renewed),
            }),
            GetOutcome::Unauthorized => Err(AuthenticatedRequestError::SessionExpired),
        }
    }

    async fn get_with_token<T>(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<GetOutcome<T>, AuthenticatedRequestError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .http
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| AuthenticatedRequestError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<T> = response
                .json()
                .await
                .map_err(|err| AuthenticatedRequestError::Network(err.to_string()))?;
            return Ok(GetOutcome::Success(envelope.data));
        }

        if status.as_u16() == 401 {
            return Ok(GetOutcome::Unauthorized);
        }

        Err(AuthenticatedRequestError::Unexpected(status.as_u16()))
    }

    /// `GET /api/v1/me` — issue #9. Reintenta una vez con refresh de token
    /// (ver `get_authenticated`) antes de forzar `SessionExpired`.
    pub async fn me(
        &self,
        token: &AuthToken,
    ) -> Result<AuthenticatedFetch<User>, AuthenticatedRequestError> {
        self.get_authenticated::<User>("/api/v1/me", token).await
    }

    /// `PATCH /api/v1/me` — issue #10. PATCH parcial: solo los campos
    /// presentes en `update` viajan en el body (ver `UpdateProfilePayload`).
    /// Reintenta una vez con refresh de token ante un 401, igual que
    /// `get_authenticated`; un 422 de validacion, en cambio, nunca se
    /// reintenta.
    pub async fn update_profile(
        &self,
        token: &AuthToken,
        update: UpdateProfilePayload,
    ) -> Result<AuthenticatedFetch<User>, UpdateProfileError> {
        if update.name.is_none() && update.email.is_none() && update.phone.is_none() {
            return Err(UpdateProfileError::NoFields);
        }
        if let Some(email) = &update.email
            && !is_valid_email(email)
        {
            return Err(UpdateProfileError::InvalidEmail);
        }
        if let Some(phone) = &update.phone
            && !is_valid_phone(phone)
        {
            return Err(UpdateProfileError::InvalidPhone);
        }

        match self
            .patch_with_token::<User>("/api/v1/me", &token.access_token, &update)
            .await?
        {
            PatchOutcome::Success(data) => {
                return Ok(AuthenticatedFetch {
                    data,
                    refreshed_token: None,
                });
            }
            PatchOutcome::Validation(body) => return Err(UpdateProfileError::Validation(body)),
            PatchOutcome::Unauthorized => {}
        }

        let renewed = self
            .refresh(&token.access_token)
            .await
            .map_err(|_| UpdateProfileError::SessionExpired)?;

        match self
            .patch_with_token::<User>("/api/v1/me", &renewed.access_token, &update)
            .await?
        {
            PatchOutcome::Success(data) => Ok(AuthenticatedFetch {
                data,
                refreshed_token: Some(renewed),
            }),
            PatchOutcome::Validation(body) => Err(UpdateProfileError::Validation(body)),
            PatchOutcome::Unauthorized => Err(UpdateProfileError::SessionExpired),
        }
    }

    async fn patch_with_token<T>(
        &self,
        path: &str,
        access_token: &str,
        body: &UpdateProfilePayload,
    ) -> Result<PatchOutcome<T>, UpdateProfileError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .http
            .patch(url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|err| UpdateProfileError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<T> = response
                .json()
                .await
                .map_err(|err| UpdateProfileError::Network(err.to_string()))?;
            return Ok(PatchOutcome::Success(envelope.data));
        }

        match status.as_u16() {
            401 => Ok(PatchOutcome::Unauthorized),
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| UpdateProfileError::Network(err.to_string()))?;
                Ok(PatchOutcome::Validation(body))
            }
            other => Err(UpdateProfileError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/rides/estimate` — issue #13. No manda nada a `/me`: es
    /// una consulta puntual, no un sub-recurso de la cuenta (ver
    /// `openapi.yaml`). Reintenta una vez con refresh de token ante un 401,
    /// igual que `update_profile`; un 422 (validacion o ruta no encontrada)
    /// nunca se reintenta.
    pub async fn estimate_ride(
        &self,
        token: &AuthToken,
        origin: Coordinates,
        destination: Coordinates,
    ) -> Result<AuthenticatedFetch<RideEstimate>, EstimateRideError> {
        let payload = RideEstimateRequestPayload {
            origin,
            destination,
        };

        match self
            .post_estimate_with_token(&token.access_token, &payload)
            .await?
        {
            PostOutcome::Success(data) => {
                return Ok(AuthenticatedFetch {
                    data,
                    refreshed_token: None,
                });
            }
            PostOutcome::Validation(body) => return Err(EstimateRideError::Validation(body)),
            PostOutcome::Unauthorized => {}
        }

        let renewed = self
            .refresh(&token.access_token)
            .await
            .map_err(|_| EstimateRideError::SessionExpired)?;

        match self
            .post_estimate_with_token(&renewed.access_token, &payload)
            .await?
        {
            PostOutcome::Success(data) => Ok(AuthenticatedFetch {
                data,
                refreshed_token: Some(renewed),
            }),
            PostOutcome::Validation(body) => Err(EstimateRideError::Validation(body)),
            PostOutcome::Unauthorized => Err(EstimateRideError::SessionExpired),
        }
    }

    async fn post_estimate_with_token(
        &self,
        access_token: &str,
        body: &RideEstimateRequestPayload,
    ) -> Result<PostOutcome<RideEstimate>, EstimateRideError> {
        let url = format!("{}/api/v1/rides/estimate", self.base_url);

        let response = self
            .http
            .post(url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|err| EstimateRideError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<RideEstimate> = response
                .json()
                .await
                .map_err(|err| EstimateRideError::Network(err.to_string()))?;
            return Ok(PostOutcome::Success(envelope.data));
        }

        match status.as_u16() {
            401 => Ok(PostOutcome::Unauthorized),
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| EstimateRideError::Network(err.to_string()))?;
                Ok(PostOutcome::Validation(body))
            }
            other => Err(EstimateRideError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/rides` — issue #14. Manda el mismo par de coordenadas
    /// que `estimate_ride`: el backend vuelve a calcular distancia, duracion
    /// y tarifa al crear el viaje, no reutiliza el estimado anterior.
    /// Reintenta una vez con refresh de token ante un 401, igual que
    /// `estimate_ride`; un 403 (cuenta no pasajero) o un 422 (validacion o
    /// viaje activo ya existente) nunca se reintentan.
    pub async fn request_ride(
        &self,
        token: &AuthToken,
        origin: Coordinates,
        destination: Coordinates,
    ) -> Result<AuthenticatedFetch<Ride>, RequestRideError> {
        let payload = RideRequestPayload {
            origin,
            destination,
        };

        match self
            .post_ride_with_token(&token.access_token, &payload)
            .await?
        {
            PostRideOutcome::Success(data) => {
                return Ok(AuthenticatedFetch {
                    data,
                    refreshed_token: None,
                });
            }
            PostRideOutcome::Forbidden => return Err(RequestRideError::Forbidden),
            PostRideOutcome::Validation(body) => return Err(RequestRideError::Validation(body)),
            PostRideOutcome::Unauthorized => {}
        }

        let renewed = self
            .refresh(&token.access_token)
            .await
            .map_err(|_| RequestRideError::SessionExpired)?;

        match self
            .post_ride_with_token(&renewed.access_token, &payload)
            .await?
        {
            PostRideOutcome::Success(data) => Ok(AuthenticatedFetch {
                data,
                refreshed_token: Some(renewed),
            }),
            PostRideOutcome::Forbidden => Err(RequestRideError::Forbidden),
            PostRideOutcome::Validation(body) => Err(RequestRideError::Validation(body)),
            PostRideOutcome::Unauthorized => Err(RequestRideError::SessionExpired),
        }
    }

    async fn post_ride_with_token(
        &self,
        access_token: &str,
        body: &RideRequestPayload,
    ) -> Result<PostRideOutcome<Ride>, RequestRideError> {
        let url = format!("{}/api/v1/rides", self.base_url);

        let response = self
            .http
            .post(url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|err| RequestRideError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<Ride> = response
                .json()
                .await
                .map_err(|err| RequestRideError::Network(err.to_string()))?;
            return Ok(PostRideOutcome::Success(envelope.data));
        }

        match status.as_u16() {
            401 => Ok(PostRideOutcome::Unauthorized),
            403 => Ok(PostRideOutcome::Forbidden),
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| RequestRideError::Network(err.to_string()))?;
                Ok(PostRideOutcome::Validation(body))
            }
            other => Err(RequestRideError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/rides/{ride}/cancel` — issue #15. No manda body: todo lo
    /// que decide esta operacion es el id del viaje y quien la pide. Reintenta
    /// una vez con refresh de token ante un 401, igual que `request_ride`; un
    /// 403 (viaje ajeno), 404 (no existe) o 422 (estado no cancelable) nunca
    /// se reintentan.
    pub async fn cancel_ride(
        &self,
        token: &AuthToken,
        ride_id: u64,
    ) -> Result<AuthenticatedFetch<RideCancellation>, CancelRideError> {
        match self
            .post_cancel_with_token(ride_id, &token.access_token)
            .await?
        {
            CancelRideOutcome::Success(data) => {
                return Ok(AuthenticatedFetch {
                    data,
                    refreshed_token: None,
                });
            }
            CancelRideOutcome::Forbidden => return Err(CancelRideError::Forbidden),
            CancelRideOutcome::NotFound => return Err(CancelRideError::NotFound),
            CancelRideOutcome::Validation(body) => return Err(CancelRideError::Validation(body)),
            CancelRideOutcome::Unauthorized => {}
        }

        let renewed = self
            .refresh(&token.access_token)
            .await
            .map_err(|_| CancelRideError::SessionExpired)?;

        match self
            .post_cancel_with_token(ride_id, &renewed.access_token)
            .await?
        {
            CancelRideOutcome::Success(data) => Ok(AuthenticatedFetch {
                data,
                refreshed_token: Some(renewed),
            }),
            CancelRideOutcome::Forbidden => Err(CancelRideError::Forbidden),
            CancelRideOutcome::NotFound => Err(CancelRideError::NotFound),
            CancelRideOutcome::Validation(body) => Err(CancelRideError::Validation(body)),
            CancelRideOutcome::Unauthorized => Err(CancelRideError::SessionExpired),
        }
    }

    async fn post_cancel_with_token(
        &self,
        ride_id: u64,
        access_token: &str,
    ) -> Result<CancelRideOutcome, CancelRideError> {
        let url = format!("{}/api/v1/rides/{}/cancel", self.base_url, ride_id);

        let response = self
            .http
            .post(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| CancelRideError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let envelope: DataEnvelope<RideCancellation> = response
                .json()
                .await
                .map_err(|err| CancelRideError::Network(err.to_string()))?;
            return Ok(CancelRideOutcome::Success(envelope.data));
        }

        match status.as_u16() {
            401 => Ok(CancelRideOutcome::Unauthorized),
            403 => Ok(CancelRideOutcome::Forbidden),
            404 => Ok(CancelRideOutcome::NotFound),
            422 => {
                let body: ApiErrorBody = response
                    .json()
                    .await
                    .map_err(|err| CancelRideError::Network(err.to_string()))?;
                Ok(CancelRideOutcome::Validation(body))
            }
            other => Err(CancelRideError::Unexpected(other)),
        }
    }

    /// `POST /api/v1/broadcasting/auth` — issue #5. Firma la suscripcion de
    /// `channel_name` (ya con el prefijo `private-` que espera el protocolo
    /// Pusher, ver `crate::realtime`) para el `socket_id` que asigno Reverb
    /// al abrir la conexion de WebSocket. No reintenta con refresh de token:
    /// este endpoint exige un access token vigente (ver `openapi.yaml`).
    pub async fn authenticate_broadcast_channel(
        &self,
        token: &AuthToken,
        socket_id: &str,
        channel_name: &str,
    ) -> Result<BroadcastAuthResponse, BroadcastAuthError> {
        let url = format!("{}/api/v1/broadcasting/auth", self.base_url);
        let payload = BroadcastAuthPayload {
            socket_id: socket_id.to_string(),
            channel_name: channel_name.to_string(),
        };

        let response = self
            .http
            .post(url)
            .bearer_auth(&token.access_token)
            .json(&payload)
            .send()
            .await
            .map_err(|err| BroadcastAuthError::Network(err.to_string()))?;

        let status = response.status();

        if status.is_success() {
            return response
                .json()
                .await
                .map_err(|err| BroadcastAuthError::Network(err.to_string()));
        }

        match status.as_u16() {
            401 => Err(BroadcastAuthError::Unauthorized),
            403 => Err(BroadcastAuthError::Forbidden),
            other => Err(BroadcastAuthError::Unexpected(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn new_stores_base_url() {
        let client = ApiClient::new("https://api.example.com");
        assert_eq!(client.base_url, "https://api.example.com");
    }

    #[tokio::test]
    async fn login_rejects_empty_fields_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client.login("", "secret").await,
            Err(LoginError::EmptyFields)
        );
        assert_eq!(
            client.login("ana@example.com", "").await,
            Err(LoginError::EmptyFields)
        );
        assert_eq!(
            client.login("   ", "secret").await,
            Err(LoginError::EmptyFields)
        );
    }

    #[tokio::test]
    async fn login_returns_authenticated_user_on_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .and(body_json(serde_json::json!({
                "email": "ana@example.com",
                "password": "motoya2026",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "user": {
                        "id": 1,
                        "name": "Ana Garcia",
                        "email": "ana@example.com",
                        "phone": "+573001234567",
                        "role": "passenger",
                    },
                    "token": {
                        "access_token": "jwt-token",
                        "token_type": "bearer",
                        "expires_in": 900,
                    },
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let authenticated = client.login("ana@example.com", "motoya2026").await.unwrap();

        assert_eq!(authenticated.user.email, "ana@example.com");
        assert_eq!(authenticated.token.access_token, "jwt-token");
    }

    #[tokio::test]
    async fn login_returns_invalid_credentials_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El email o la contrasena no son correctos.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .login("ana@example.com", "wrong-password")
            .await
            .unwrap_err();

        assert_eq!(
            error,
            LoginError::InvalidCredentials(
                "El email o la contrasena no son correctos.".to_string()
            )
        );
    }

    #[tokio::test]
    async fn login_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .login("ana@example.com", "motoya2026")
            .await
            .unwrap_err();

        assert!(matches!(error, LoginError::Network(_)));
    }

    #[tokio::test]
    async fn register_passenger_rejects_empty_fields_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_passenger("", "ana@example.com", "+573001234567", "motoya2026")
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_passenger("Ana Garcia", "", "+573001234567", "motoya2026")
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_passenger("Ana Garcia", "ana@example.com", "", "motoya2026")
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_passenger("Ana Garcia", "ana@example.com", "+573001234567", "")
                .await,
            Err(RegisterError::EmptyFields)
        );
    }

    #[tokio::test]
    async fn register_passenger_rejects_invalid_email_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_passenger("Ana Garcia", "not-an-email", "+573001234567", "motoya2026")
                .await,
            Err(RegisterError::InvalidEmail)
        );
    }

    #[tokio::test]
    async fn register_passenger_rejects_invalid_phone_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_passenger("Ana Garcia", "ana@example.com", "123", "motoya2026")
                .await,
            Err(RegisterError::InvalidPhone)
        );
        assert_eq!(
            client
                .register_passenger(
                    "Ana Garcia",
                    "ana@example.com",
                    "+57-300-123-4567",
                    "motoya2026"
                )
                .await,
            Err(RegisterError::InvalidPhone)
        );
    }

    #[tokio::test]
    async fn register_passenger_returns_authenticated_user_on_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/register/passenger"))
            .and(body_json(serde_json::json!({
                "name": "Ana Garcia",
                "email": "ana@example.com",
                "phone": "+573001234567",
                "password": "motoya2026",
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "data": {
                    "user": {
                        "id": 1,
                        "name": "Ana Garcia",
                        "email": "ana@example.com",
                        "phone": "+573001234567",
                        "role": "passenger",
                    },
                    "token": {
                        "access_token": "jwt-token",
                        "token_type": "bearer",
                        "expires_in": 900,
                    },
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let authenticated = client
            .register_passenger(
                "Ana Garcia",
                "ana@example.com",
                "+573001234567",
                "motoya2026",
            )
            .await
            .unwrap();

        assert_eq!(authenticated.user.email, "ana@example.com");
        assert_eq!(authenticated.token.access_token, "jwt-token");
    }

    #[tokio::test]
    async fn register_passenger_returns_validation_error_on_422() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/register/passenger"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "The email has already been taken.",
                "errors": {
                    "email": ["The email has already been taken."],
                },
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .register_passenger(
                "Ana Garcia",
                "ana@example.com",
                "+573001234567",
                "motoya2026",
            )
            .await
            .unwrap_err();

        assert_eq!(
            error,
            RegisterError::Validation(ApiErrorBody {
                message: "The email has already been taken.".to_string(),
                errors: Some(HashMap::from([(
                    "email".to_string(),
                    vec!["The email has already been taken.".to_string()]
                )])),
            })
        );
        assert_eq!(
            error.field_message("email"),
            Some("The email has already been taken.".to_string())
        );
        assert_eq!(error.field_message("phone"), None);
    }

    #[test]
    fn field_message_is_none_for_non_validation_errors() {
        assert_eq!(RegisterError::EmptyFields.field_message("email"), None);
        assert_eq!(
            RegisterError::Network("boom".to_string()).field_message("email"),
            None
        );
    }

    #[tokio::test]
    async fn register_passenger_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .register_passenger(
                "Ana Garcia",
                "ana@example.com",
                "+573001234567",
                "motoya2026",
            )
            .await
            .unwrap_err();

        assert!(matches!(error, RegisterError::Network(_)));
    }

    #[tokio::test]
    async fn register_driver_rejects_empty_fields_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_driver(
                    "",
                    "carlos@example.com",
                    "+573001234567",
                    "motoya2026",
                    "ABC1234"
                )
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_driver("Carlos Perez", "", "+573001234567", "motoya2026", "ABC1234")
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "",
                    "motoya2026",
                    "ABC1234"
                )
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "+573001234567",
                    "",
                    "ABC1234"
                )
                .await,
            Err(RegisterError::EmptyFields)
        );
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "+573001234567",
                    "motoya2026",
                    ""
                )
                .await,
            Err(RegisterError::EmptyFields)
        );
    }

    #[tokio::test]
    async fn register_driver_rejects_invalid_email_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "not-an-email",
                    "+573001234567",
                    "motoya2026",
                    "ABC1234"
                )
                .await,
            Err(RegisterError::InvalidEmail)
        );
    }

    #[tokio::test]
    async fn register_driver_rejects_invalid_phone_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "123",
                    "motoya2026",
                    "ABC1234"
                )
                .await,
            Err(RegisterError::InvalidPhone)
        );
    }

    #[tokio::test]
    async fn register_driver_rejects_invalid_license_number_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        // Muy corto.
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "+573001234567",
                    "motoya2026",
                    "AB1"
                )
                .await,
            Err(RegisterError::InvalidLicenseNumber)
        );
        // Minusculas: el backend solo acepta mayusculas (regex
        // `^[A-Z0-9-]{5,50}$`).
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "+573001234567",
                    "motoya2026",
                    "abc1234"
                )
                .await,
            Err(RegisterError::InvalidLicenseNumber)
        );
        // Caracter fuera del set permitido.
        assert_eq!(
            client
                .register_driver(
                    "Carlos Perez",
                    "carlos@example.com",
                    "+573001234567",
                    "motoya2026",
                    "ABC 1234"
                )
                .await,
            Err(RegisterError::InvalidLicenseNumber)
        );
    }

    #[tokio::test]
    async fn register_driver_returns_authenticated_user_on_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/register/driver"))
            .and(body_json(serde_json::json!({
                "name": "Carlos Perez",
                "email": "carlos@example.com",
                "phone": "+573001234567",
                "password": "motoya2026",
                "license_number": "ABC1234",
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "data": {
                    "user": {
                        "id": 2,
                        "name": "Carlos Perez",
                        "email": "carlos@example.com",
                        "phone": "+573001234567",
                        "role": "driver",
                    },
                    "token": {
                        "access_token": "jwt-token",
                        "token_type": "bearer",
                        "expires_in": 900,
                    },
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let authenticated = client
            .register_driver(
                "Carlos Perez",
                "carlos@example.com",
                "+573001234567",
                "motoya2026",
                "ABC1234",
            )
            .await
            .unwrap();

        assert_eq!(authenticated.user.email, "carlos@example.com");
        assert_eq!(authenticated.user.role, Role::Driver);
        assert_eq!(authenticated.token.access_token, "jwt-token");
    }

    #[tokio::test]
    async fn register_driver_returns_validation_error_on_422() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/register/driver"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "The license number has already been taken.",
                "errors": {
                    "license_number": ["The license number has already been taken."],
                },
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .register_driver(
                "Carlos Perez",
                "carlos@example.com",
                "+573001234567",
                "motoya2026",
                "ABC1234",
            )
            .await
            .unwrap_err();

        assert_eq!(
            error,
            RegisterError::Validation(ApiErrorBody {
                message: "The license number has already been taken.".to_string(),
                errors: Some(HashMap::from([(
                    "license_number".to_string(),
                    vec!["The license number has already been taken.".to_string()]
                )])),
            })
        );
        assert_eq!(
            error.field_message("license_number"),
            Some("The license number has already been taken.".to_string())
        );
    }

    #[tokio::test]
    async fn register_driver_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .register_driver(
                "Carlos Perez",
                "carlos@example.com",
                "+573001234567",
                "motoya2026",
                "ABC1234",
            )
            .await
            .unwrap_err();

        assert!(matches!(error, RegisterError::Network(_)));
    }

    fn sample_token() -> AuthToken {
        AuthToken {
            access_token: "jwt-token".to_string(),
            token_type: "bearer".to_string(),
            expires_in: Some(900),
        }
    }

    #[tokio::test]
    async fn refresh_returns_a_new_token_on_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let refreshed = client.refresh("jwt-token").await.unwrap();

        assert_eq!(refreshed.access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn refresh_returns_unauthorized_when_outside_the_refresh_window() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.refresh("stale-token").await.unwrap_err();

        assert_eq!(
            error,
            RefreshError::Unauthorized("El token no es valido o ya expiro.".to_string())
        );
    }

    #[tokio::test]
    async fn refresh_returns_rate_limited_on_429() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.refresh("jwt-token").await.unwrap_err();

        assert_eq!(error, RefreshError::RateLimited);
    }

    #[tokio::test]
    async fn logout_succeeds_on_204() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/logout"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());

        assert_eq!(client.logout("jwt-token").await, Ok(()));
    }

    #[tokio::test]
    async fn logout_returns_unauthorized_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/logout"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());

        assert_eq!(
            client.logout("stale-token").await,
            Err(LogoutError::Unauthorized)
        );
    }

    #[tokio::test]
    async fn logout_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client.logout("jwt-token").await.unwrap_err();

        assert!(matches!(error, LogoutError::Network(_)));
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Probe {
        id: u64,
    }

    #[tokio::test]
    async fn get_authenticated_returns_data_without_refreshing_when_token_is_valid() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v1/_probe"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "data": { "id": 1 } })),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .get_authenticated::<Probe>("/api/v1/_probe", &sample_token())
            .await
            .unwrap();

        assert_eq!(fetch.data, Probe { id: 1 });
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn get_authenticated_refreshes_once_and_retries_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v1/_probe"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v1/_probe"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer new-jwt-token",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "data": { "id": 1 } })),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .get_authenticated::<Probe>("/api/v1/_probe", &sample_token())
            .await
            .unwrap();

        assert_eq!(fetch.data, Probe { id: 1 });
        assert_eq!(fetch.refreshed_token.unwrap().access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn get_authenticated_forces_session_expired_when_refresh_also_fails() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v1/_probe"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .get_authenticated::<Probe>("/api/v1/_probe", &sample_token())
            .await
            .unwrap_err();

        assert_eq!(error, AuthenticatedRequestError::SessionExpired);
    }

    #[tokio::test]
    async fn me_returns_the_authenticated_account() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v1/me"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 1,
                    "name": "Ana Garcia",
                    "email": "ana@example.com",
                    "phone": "+573001234567",
                    "role": "passenger",
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client.me(&sample_token()).await.unwrap();

        assert_eq!(fetch.data.name, "Ana Garcia");
        assert_eq!(fetch.data.role, crate::models::Role::Passenger);
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn me_forces_session_expired_when_the_token_cannot_be_renewed() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.me(&sample_token()).await.unwrap_err();

        assert_eq!(error, AuthenticatedRequestError::SessionExpired);
    }

    #[tokio::test]
    async fn update_profile_rejects_an_empty_patch_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        let error = client
            .update_profile(&sample_token(), UpdateProfilePayload::default())
            .await
            .unwrap_err();

        assert_eq!(error, UpdateProfileError::NoFields);
    }

    #[tokio::test]
    async fn update_profile_rejects_invalid_email_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        let error = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    email: Some("not-an-email".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();

        assert_eq!(error, UpdateProfileError::InvalidEmail);
    }

    #[tokio::test]
    async fn update_profile_rejects_invalid_phone_without_sending_a_request() {
        let client = ApiClient::new("https://unreachable.invalid");

        let error = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    phone: Some("123".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();

        assert_eq!(error, UpdateProfileError::InvalidPhone);
    }

    #[tokio::test]
    async fn update_profile_sends_only_the_present_fields_and_returns_the_updated_account() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/api/v1/me"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .and(body_json(serde_json::json!({
                "name": "Ana Garcia Perez",
                "phone": "+573007654321",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 1,
                    "name": "Ana Garcia Perez",
                    "email": "ana@example.com",
                    "phone": "+573007654321",
                    "role": "passenger",
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    name: Some("Ana Garcia Perez".to_string()),
                    email: None,
                    phone: Some("+573007654321".to_string()),
                },
            )
            .await
            .unwrap();

        assert_eq!(fetch.data.name, "Ana Garcia Perez");
        assert_eq!(fetch.data.phone, "+573007654321");
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn update_profile_returns_validation_error_on_422_without_retrying() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "The email has already been taken.",
                "errors": {
                    "email": ["The email has already been taken."],
                },
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    email: Some("ana@example.com".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();

        assert_eq!(
            error,
            UpdateProfileError::Validation(ApiErrorBody {
                message: "The email has already been taken.".to_string(),
                errors: Some(HashMap::from([(
                    "email".to_string(),
                    vec!["The email has already been taken.".to_string()]
                )])),
            })
        );
        assert_eq!(
            error.field_message("email"),
            Some("The email has already been taken.".to_string())
        );
    }

    #[tokio::test]
    async fn update_profile_refreshes_once_and_retries_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/api/v1/me"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("PATCH"))
            .and(path("/api/v1/me"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer new-jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 1,
                    "name": "Ana Garcia Perez",
                    "email": "ana@example.com",
                    "phone": "+573001234567",
                    "role": "passenger",
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    name: Some("Ana Garcia Perez".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(fetch.data.name, "Ana Garcia Perez");
        assert_eq!(fetch.refreshed_token.unwrap().access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn update_profile_forces_session_expired_when_the_token_cannot_be_renewed() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    name: Some("Ana Garcia Perez".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();

        assert_eq!(error, UpdateProfileError::SessionExpired);
    }

    #[tokio::test]
    async fn update_profile_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .update_profile(
                &sample_token(),
                UpdateProfilePayload {
                    name: Some("Ana Garcia Perez".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(error, UpdateProfileError::Network(_)));
    }

    fn sample_origin() -> Coordinates {
        Coordinates {
            latitude: 4.710989,
            longitude: -74.072092,
        }
    }

    fn sample_destination() -> Coordinates {
        Coordinates {
            latitude: 4.698,
            longitude: -74.061,
        }
    }

    #[tokio::test]
    async fn estimate_ride_sends_origin_and_destination_and_returns_the_estimate() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/estimate"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .and(body_json(serde_json::json!({
                "origin": {"latitude": 4.710989, "longitude": -74.072092},
                "destination": {"latitude": 4.698, "longitude": -74.061},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "distance_meters": 7421,
                    "duration_seconds": 842,
                    "currency": "COP",
                    "estimated_fare": 8850,
                    "is_estimate": true,
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .estimate_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap();

        assert_eq!(fetch.data.distance_meters, 7421);
        assert_eq!(fetch.data.duration_seconds, 842);
        assert_eq!(fetch.data.currency, "COP");
        assert_eq!(fetch.data.estimated_fare, 8850);
        assert!(fetch.data.is_estimate);
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn estimate_ride_returns_validation_error_on_422_without_retrying() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/estimate"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "No fue posible calcular una ruta entre esas coordenadas. Puede que la zona no este cubierta por el servicio.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .estimate_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            EstimateRideError::Validation(ApiErrorBody {
                message: "No fue posible calcular una ruta entre esas coordenadas. Puede que la zona no este cubierta por el servicio.".to_string(),
                errors: None,
            })
        );
    }

    #[tokio::test]
    async fn estimate_ride_refreshes_once_and_retries_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/estimate"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/estimate"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer new-jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "distance_meters": 7421,
                    "duration_seconds": 842,
                    "currency": "COP",
                    "estimated_fare": 8850,
                    "is_estimate": true,
                }
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .estimate_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap();

        assert_eq!(fetch.data.distance_meters, 7421);
        assert_eq!(fetch.refreshed_token.unwrap().access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn estimate_ride_forces_session_expired_when_the_token_cannot_be_renewed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/estimate"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .estimate_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert_eq!(error, EstimateRideError::SessionExpired);
    }

    #[tokio::test]
    async fn estimate_ride_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .estimate_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert!(matches!(error, EstimateRideError::Network(_)));
    }

    fn sample_ride_json() -> serde_json::Value {
        serde_json::json!({
            "id": 1,
            "status": "requested",
            "origin": {"latitude": 4.710989, "longitude": -74.072092},
            "destination": {"latitude": 4.698, "longitude": -74.061},
            "distance_meters": 7421,
            "duration_seconds": 842,
            "currency": "COP",
            "estimated_fare": 8850,
            "driver": null,
            "requested_at": "2026-07-31T14:03:21+00:00",
            "started_at": null,
            "completed_at": null,
            "final_fare": null,
            "payment": null,
        })
    }

    #[tokio::test]
    async fn request_ride_sends_origin_and_destination_and_returns_the_created_ride() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .and(body_json(serde_json::json!({
                "origin": {"latitude": 4.710989, "longitude": -74.072092},
                "destination": {"latitude": 4.698, "longitude": -74.061},
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "data": sample_ride_json(),
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap();

        assert_eq!(fetch.data.id, 1);
        assert_eq!(fetch.data.status, crate::models::RideStatus::Requested);
        assert_eq!(fetch.data.estimated_fare, 8850);
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn request_ride_returns_forbidden_on_403_without_retrying() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "This action is unauthorized.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert_eq!(error, RequestRideError::Forbidden);
    }

    #[tokio::test]
    async fn request_ride_returns_validation_error_on_422_when_an_active_ride_already_exists() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "Ya tienes un viaje en curso; terminalo o cancelalo antes de solicitar otro.",
                "errors": {
                    "ride": ["Ya tienes un viaje en curso; terminalo o cancelalo antes de solicitar otro."],
                },
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            RequestRideError::Validation(ApiErrorBody {
                message: "Ya tienes un viaje en curso; terminalo o cancelalo antes de solicitar otro."
                    .to_string(),
                errors: Some(HashMap::from([(
                    "ride".to_string(),
                    vec![
                        "Ya tienes un viaje en curso; terminalo o cancelalo antes de solicitar otro."
                            .to_string()
                    ]
                )])),
            })
        );
    }

    #[tokio::test]
    async fn request_ride_refreshes_once_and_retries_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer new-jwt-token",
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "data": sample_ride_json(),
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap();

        assert_eq!(fetch.data.id, 1);
        assert_eq!(fetch.refreshed_token.unwrap().access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn request_ride_forces_session_expired_when_the_token_cannot_be_renewed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert_eq!(error, RequestRideError::SessionExpired);
    }

    #[tokio::test]
    async fn request_ride_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .request_ride(&sample_token(), sample_origin(), sample_destination())
            .await
            .unwrap_err();

        assert!(matches!(error, RequestRideError::Network(_)));
    }

    fn sample_cancelled_ride_json() -> serde_json::Value {
        let mut ride = sample_ride_json();
        ride["status"] = serde_json::json!("cancelled");
        ride["cancellation_fee_applies"] = serde_json::json!(false);
        ride
    }

    #[tokio::test]
    async fn cancel_ride_returns_the_cancelled_ride_without_a_fee_when_no_driver_was_assigned() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": sample_cancelled_ride_json(),
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client.cancel_ride(&sample_token(), 1).await.unwrap();

        assert_eq!(fetch.data.ride.id, 1);
        assert_eq!(fetch.data.ride.status, crate::models::RideStatus::Cancelled);
        assert_eq!(fetch.data.cancellation_fee_applies, Some(false));
        assert_eq!(fetch.refreshed_token, None);
    }

    #[tokio::test]
    async fn cancel_ride_returns_forbidden_on_403_without_retrying() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "This action is unauthorized.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.cancel_ride(&sample_token(), 1).await.unwrap_err();

        assert_eq!(error, CancelRideError::Forbidden);
    }

    #[tokio::test]
    async fn cancel_ride_returns_not_found_on_404_without_retrying() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/999/cancel"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "No query results for model [App\\Models\\Ride] 999.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.cancel_ride(&sample_token(), 999).await.unwrap_err();

        assert_eq!(error, CancelRideError::NotFound);
    }

    #[tokio::test]
    async fn cancel_ride_returns_validation_error_on_422_when_the_ride_is_not_cancellable() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "El viaje está en curso; no se puede cancelar de esta forma.",
                "errors": {
                    "ride": ["El viaje está en curso; no se puede cancelar de esta forma."],
                },
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.cancel_ride(&sample_token(), 1).await.unwrap_err();

        assert_eq!(
            error,
            CancelRideError::Validation(ApiErrorBody {
                message: "El viaje está en curso; no se puede cancelar de esta forma.".to_string(),
                errors: Some(HashMap::from([(
                    "ride".to_string(),
                    vec!["El viaje está en curso; no se puede cancelar de esta forma.".to_string()]
                )])),
            })
        );
    }

    #[tokio::test]
    async fn cancel_ride_refreshes_once_and_retries_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "access_token": "new-jwt-token",
                    "token_type": "bearer",
                    "expires_in": 900,
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer new-jwt-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": sample_cancelled_ride_json(),
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let fetch = client.cancel_ride(&sample_token(), 1).await.unwrap();

        assert_eq!(fetch.data.ride.id, 1);
        assert_eq!(fetch.refreshed_token.unwrap().access_token, "new-jwt-token");
    }

    #[tokio::test]
    async fn cancel_ride_forces_session_expired_when_the_token_cannot_be_renewed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/rides/1/cancel"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "El token no es valido o ya expiro.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client.cancel_ride(&sample_token(), 1).await.unwrap_err();

        assert_eq!(error, CancelRideError::SessionExpired);
    }

    #[tokio::test]
    async fn cancel_ride_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client.cancel_ride(&sample_token(), 1).await.unwrap_err();

        assert!(matches!(error, CancelRideError::Network(_)));
    }

    #[tokio::test]
    async fn authenticate_broadcast_channel_returns_the_signature_on_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/broadcasting/auth"))
            .and(wiremock::matchers::header(
                "Authorization",
                "Bearer jwt-token",
            ))
            .and(body_json(serde_json::json!({
                "socket_id": "123456.789012",
                "channel_name": "private-ride.7",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "auth": "motoya-local:8f3c1a2b4d5e6f70",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let response = client
            .authenticate_broadcast_channel(&sample_token(), "123456.789012", "private-ride.7")
            .await
            .unwrap();

        assert_eq!(response.auth, "motoya-local:8f3c1a2b4d5e6f70");
    }

    #[tokio::test]
    async fn authenticate_broadcast_channel_returns_unauthorized_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/broadcasting/auth"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Unauthenticated.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .authenticate_broadcast_channel(&sample_token(), "123456.789012", "private-ride.7")
            .await
            .unwrap_err();

        assert_eq!(error, BroadcastAuthError::Unauthorized);
    }

    #[tokio::test]
    async fn authenticate_broadcast_channel_returns_forbidden_on_403() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/broadcasting/auth"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "This action is unauthorized.",
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let error = client
            .authenticate_broadcast_channel(&sample_token(), "123456.789012", "private-ride.7")
            .await
            .unwrap_err();

        assert_eq!(error, BroadcastAuthError::Forbidden);
    }

    #[tokio::test]
    async fn authenticate_broadcast_channel_returns_network_error_when_server_is_unreachable() {
        let client = ApiClient::new("http://127.0.0.1:1");

        let error = client
            .authenticate_broadcast_channel(&sample_token(), "123456.789012", "private-ride.7")
            .await
            .unwrap_err();

        assert!(matches!(error, BroadcastAuthError::Network(_)));
    }
}
