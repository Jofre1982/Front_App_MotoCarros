//! Cliente HTTP hacia `Back_App_MotoCarros` (`/api/v1`).
//!
//! Los endpoints reales se agregan issue por issue, reflejando el contrato
//! que expone el backend en cada momento (ver `openapi.yaml` de
//! `Back_App_MotoCarros`).

use crate::models::{
    ApiErrorBody, AuthToken, AuthenticatedUser, DataEnvelope, LoginPayload,
    RegisterPassengerPayload,
};

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

/// Fallos posibles de `POST /api/v1/auth/register/passenger`.
///
/// `InvalidEmail`/`InvalidPhone` se detectan en el cliente antes de mandar la
/// request (formato basico), sin duplicar las reglas completas del backend
/// (unicidad, normalizacion) — esas siguen viajando como `Validation` en un
/// 422 (ver `openapi.yaml` de `Back_App_MotoCarros`).
#[derive(Debug, Clone, PartialEq)]
pub enum RegisterError {
    EmptyFields,
    InvalidEmail,
    InvalidPhone,
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

enum GetOutcome<T> {
    Success(T),
    Unauthorized,
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
}
