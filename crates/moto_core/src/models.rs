//! Tipos que reflejan el contrato JSON de `/api/v1`.
//!
//! Se completan a medida que se implementan los issues correspondientes — no
//! se inventan campos que el backend no exponga todavia.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Rol de autenticacion de la cuenta (`openapi.yaml#/components/schemas/User`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Passenger,
    Driver,
}

/// `openapi.yaml#/components/schemas/User`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub role: Role,
}

/// `openapi.yaml#/components/schemas/AuthToken`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
}

/// `openapi.yaml#/components/schemas/AuthenticatedUser` — respuesta comun del
/// login y del registro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthenticatedUser {
    pub user: User,
    pub token: AuthToken,
}

/// Envelope `{ "data": ... }` que agregan los API Resources de Laravel en el
/// nivel superior de la respuesta (ver `.claude/STANDARDS.md` de
/// `Back_App_MotoCarros`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DataEnvelope<T> {
    pub data: T,
}

/// `openapi.yaml#/components/schemas/Error` — formato de error del exception
/// handler de Laravel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiErrorBody {
    pub message: String,
    #[serde(default)]
    pub errors: Option<HashMap<String, Vec<String>>>,
}

/// Body de `POST /api/v1/auth/login`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}

/// Body de `POST /api/v1/auth/register/passenger`
/// (`openapi.yaml#/components/schemas/PassengerRegistrationRequest`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RegisterPassengerPayload {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_authenticated_user_envelope() {
        let json = r#"{
            "data": {
                "user": {
                    "id": 1,
                    "name": "Ana Garcia",
                    "email": "ana@example.com",
                    "phone": "+573001234567",
                    "role": "passenger"
                },
                "token": {
                    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9",
                    "token_type": "bearer",
                    "expires_in": 900
                }
            }
        }"#;

        let envelope: DataEnvelope<AuthenticatedUser> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.user.role, Role::Passenger);
        assert_eq!(
            envelope.data.token.access_token,
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9"
        );
        assert_eq!(envelope.data.token.expires_in, Some(900));
    }

    #[test]
    fn deserializes_error_body_without_errors_field() {
        let json = r#"{"message": "El email o la contrasena no son correctos."}"#;

        let error: ApiErrorBody = serde_json::from_str(json).unwrap();

        assert_eq!(error.message, "El email o la contrasena no son correctos.");
        assert!(error.errors.is_none());
    }
}
