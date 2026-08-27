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
/// (issue #13) y de `POST /api/v1/rides` (issue #14).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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

/// Estado de un viaje (`openapi.yaml#/components/schemas/Ride.status`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RideStatus {
    Requested,
    Accepted,
    InProgress,
    Completed,
    Cancelled,
}

/// Body de `POST /api/v1/rides`
/// (`openapi.yaml#/components/schemas/RideRequest`).
///
/// Misma forma que `RideEstimateRequestPayload` a proposito (el backend lo
/// documenta asi para que la app mande el mismo cuerpo con el que estimo la
/// tarifa), pero es un tipo aparte porque son la entrada de dos operaciones
/// distintas que pueden divergir mas adelante (issue #14).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct RideRequestPayload {
    pub origin: Coordinates,
    pub destination: Coordinates,
}

/// `openapi.yaml#/components/schemas/RideDriver` — el conductor asignado a un
/// viaje, visto desde el viaje. `None` mientras nadie lo haya aceptado
/// todavia (historia #17, fuera de alcance de #14).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RideDriver {
    pub id: u64,
    pub name: String,
}

/// Resultado del cobro de un viaje completado
/// (`openapi.yaml#/components/schemas/Payment`, historia #25).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Paid,
    Failed,
}

/// `openapi.yaml#/components/schemas/Payment`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payment {
    pub status: PaymentStatus,
}

/// `openapi.yaml#/components/schemas/Ride` — respuesta de `POST /api/v1/rides`
/// (issue #14). `driver`, `started_at`, `completed_at`, `final_fare` y
/// `payment` viajan siempre presentes pero en `null` hasta que la historia
/// correspondiente los produzca (aceptar #17, iniciar #18, completar #23,
/// pagar #24) — por eso son `Option` en vez de campos opcionales del struct.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Ride {
    pub id: u64,
    pub status: RideStatus,
    pub origin: Coordinates,
    pub destination: Coordinates,
    pub distance_meters: u32,
    pub duration_seconds: u32,
    pub currency: String,
    pub estimated_fare: i64,
    pub driver: Option<RideDriver>,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub final_fare: Option<i64>,
    pub payment: Option<Payment>,
}

/// `openapi.yaml#/components/schemas/Ride` + `cancellation_fee_applies` —
/// respuesta de `POST /api/v1/rides/{ride}/cancel` (issue #15).
///
/// `cancellation_fee_applies` esta ausente cuando quien cancela es el
/// conductor asignado (historia #23, fuera de alcance de #15): ese caso no
/// cancela el viaje, solo lo devuelve al pool sin conductor. `#[serde(flatten)]`
/// reutiliza `Ride` en vez de repetir sus campos.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RideCancellation {
    #[serde(flatten)]
    pub ride: Ride,
    pub cancellation_fee_applies: Option<bool>,
}

/// Body de `POST /api/v1/broadcasting/auth`
/// (`openapi.yaml#/components/schemas/BroadcastAuthRequest`).
///
/// Los nombres de los campos los fija el protocolo Pusher que habla Reverb,
/// no esta API (issue #5).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BroadcastAuthPayload {
    pub socket_id: String,
    pub channel_name: String,
}

/// `openapi.yaml#/components/schemas/BroadcastAuthResponse` — respuesta de
/// `POST /api/v1/broadcasting/auth` (issue #5). Sin el envelope `data` del
/// resto de la API: el formato lo fija el protocolo Pusher, que lo espera
/// asi para reenviarlo tal cual a Reverb.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BroadcastAuthResponse {
    pub auth: String,
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

    #[test]
    fn ride_request_payload_serializes_origin_and_destination() {
        let payload = RideRequestPayload {
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
    fn deserializes_ride_envelope_with_no_driver_yet() {
        let json = r#"{
            "data": {
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
                "payment": null
            }
        }"#;

        let envelope: DataEnvelope<Ride> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.id, 1);
        assert_eq!(envelope.data.status, RideStatus::Requested);
        assert_eq!(envelope.data.estimated_fare, 8850);
        assert_eq!(envelope.data.driver, None);
        assert_eq!(envelope.data.started_at, None);
        assert_eq!(envelope.data.payment, None);
    }

    #[test]
    fn deserializes_ride_with_an_assigned_driver_and_payment() {
        let json = r#"{
            "id": 1,
            "status": "completed",
            "origin": {"latitude": 4.710989, "longitude": -74.072092},
            "destination": {"latitude": 4.698, "longitude": -74.061},
            "distance_meters": 7421,
            "duration_seconds": 842,
            "currency": "COP",
            "estimated_fare": 8850,
            "driver": {"id": 42, "name": "Carlos Perez"},
            "requested_at": "2026-07-31T14:03:21+00:00",
            "started_at": "2026-07-31T14:05:00+00:00",
            "completed_at": "2026-07-31T14:20:00+00:00",
            "final_fare": 9100,
            "payment": {"status": "paid"}
        }"#;

        let ride: Ride = serde_json::from_str(json).unwrap();

        assert_eq!(ride.status, RideStatus::Completed);
        assert_eq!(
            ride.driver,
            Some(RideDriver {
                id: 42,
                name: "Carlos Perez".to_string(),
            })
        );
        assert_eq!(
            ride.payment,
            Some(Payment {
                status: PaymentStatus::Paid,
            })
        );
    }

    #[test]
    fn deserializes_ride_cancellation_envelope_with_the_fee_flag() {
        let json = r#"{
            "data": {
                "id": 1,
                "status": "cancelled",
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
                "cancellation_fee_applies": false
            }
        }"#;

        let envelope: DataEnvelope<RideCancellation> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.ride.status, RideStatus::Cancelled);
        assert_eq!(envelope.data.cancellation_fee_applies, Some(false));
    }

    #[test]
    fn deserializes_ride_cancellation_without_the_fee_flag_when_the_driver_cancels() {
        let json = r#"{
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
            "payment": null
        }"#;

        let cancellation: RideCancellation = serde_json::from_str(json).unwrap();

        assert_eq!(cancellation.ride.status, RideStatus::Requested);
        assert_eq!(cancellation.cancellation_fee_applies, None);
    }

    #[test]
    fn broadcast_auth_payload_serializes_socket_id_and_channel_name() {
        let payload = BroadcastAuthPayload {
            socket_id: "123456.789012".to_string(),
            channel_name: "private-ride.7".to_string(),
        };

        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "socket_id": "123456.789012",
                "channel_name": "private-ride.7",
            })
        );
    }

    #[test]
    fn deserializes_broadcast_auth_response() {
        let json = r#"{"auth": "motoya-local:8f3c1a2b4d5e6f70"}"#;

        let response: BroadcastAuthResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.auth, "motoya-local:8f3c1a2b4d5e6f70");
    }
}
