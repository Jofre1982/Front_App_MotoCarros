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

/// Body de `PATCH /api/v1/me`
/// (`openapi.yaml#/components/schemas/UpdateProfileRequest`).
///
/// PATCH parcial: cada campo ausente (`None`) se omite del JSON en vez de
/// viajar como `null`, para que el backend conserve el valor actual en vez
/// de interpretarlo como "borrar este dato" (ver issue #10).
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct UpdateProfilePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

/// Un punto geografico (`openapi.yaml#/components/schemas/Coordinates`), usado
/// tanto en el origen como en el destino de `POST /api/v1/rides/estimate`
/// (issue #13).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

/// Body de `POST /api/v1/rides/estimate`
/// (`openapi.yaml#/components/schemas/RideEstimateRequest`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct RideEstimateRequestPayload {
    pub origin: Coordinates,
    pub destination: Coordinates,
}

/// `openapi.yaml#/components/schemas/RideEstimate` — respuesta de
/// `POST /api/v1/rides/estimate` (issue #13).
///
/// `estimated_fare` es un entero en la unidad minima de `currency`, nunca un
/// decimal: el backend nunca lo devuelve fraccionado (ver `FareBreakdown` de
/// `Back_App_MotoCarros`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RideEstimate {
    pub distance_meters: u32,
    pub duration_seconds: u32,
    pub currency: String,
    pub estimated_fare: i64,
    pub is_estimate: bool,
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
    fn update_profile_payload_omits_absent_fields_from_the_json() {
        let payload = UpdateProfilePayload {
            name: Some("Ana Garcia Perez".to_string()),
            email: None,
            phone: Some("+573007654321".to_string()),
        };

        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "name": "Ana Garcia Perez",
                "phone": "+573007654321",
            })
        );
    }

    #[test]
    fn deserializes_error_body_without_errors_field() {
        let json = r#"{"message": "El email o la contrasena no son correctos."}"#;

        let error: ApiErrorBody = serde_json::from_str(json).unwrap();

        assert_eq!(error.message, "El email o la contrasena no son correctos.");
        assert!(error.errors.is_none());
    }

    #[test]
    fn ride_estimate_request_payload_serializes_origin_and_destination() {
        let payload = RideEstimateRequestPayload {
            origin: Coordinates {
                latitude: 4.710989,
                longitude: -74.072092,
            },
            destination: Coordinates {
                latitude: 4.698,
                longitude: -74.061,
            },
        };

        let json = serde_json::to_value(payload).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "origin": {"latitude": 4.710989, "longitude": -74.072092},
                "destination": {"latitude": 4.698, "longitude": -74.061},
            })
        );
    }

    #[test]
    fn deserializes_ride_estimate_envelope() {
        let json = r#"{
            "data": {
                "distance_meters": 7421,
                "duration_seconds": 842,
                "currency": "COP",
                "estimated_fare": 8850,
                "is_estimate": true
            }
        }"#;

        let envelope: DataEnvelope<RideEstimate> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.distance_meters, 7421);
        assert_eq!(envelope.data.duration_seconds, 842);
        assert_eq!(envelope.data.currency, "COP");
        assert_eq!(envelope.data.estimated_fare, 8850);
        assert!(envelope.data.is_estimate);
    }
}
