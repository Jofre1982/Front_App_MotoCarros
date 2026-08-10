//! Cliente HTTP hacia `Back_App_MotoCarros` (`/api/v1`).
//!
//! Los endpoints reales se agregan issue por issue, reflejando el contrato
//! que expone el backend en cada momento (ver `openapi.yaml` de
//! `Back_App_MotoCarros`).

use crate::models::{ApiErrorBody, AuthenticatedUser, DataEnvelope, LoginPayload};

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
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
