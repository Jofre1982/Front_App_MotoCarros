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
    /// Si el celular ya se confirmo con un codigo por SMS (issue #69 del
    /// backend). Ver `ApiClient::request_phone_verification` y
    /// `ApiClient::confirm_phone_verification`.
    pub phone_verified: bool,
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

/// Body de `POST /api/v1/auth/register/driver`
/// (`openapi.yaml#/components/schemas/DriverRegistrationRequest`).
///
/// `license_number` es obligatorio en el backend (`RegisterDriverRequest`,
/// ver `Back_App_MotoCarros`) — sin el, la request siempre responde 422. En
/// esta etapa de pruebas la UI lo pide como "Numero de documento" en vez de
/// "licencia" (ver issue #7), pero viaja tal cual bajo el nombre de campo que
/// espera el backend.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RegisterDriverPayload {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub password: String,
    pub license_number: String,
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

/// Body de `POST /api/v1/me/phone/verification/confirm`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfirmPhoneVerificationPayload {
    pub code: String,
}

/// Tipo de documento de verificacion del conductor
/// (`openapi.yaml#/components/schemas/DriverDocument` de `Back_App_MotoCarros`).
///
/// Licencia de conduccion y SOAT no son obligatorios todavia (decision de
/// negocio explicita del backend) y por eso no tienen variante aca: agregar
/// una sin que el backend la exija se romperia contra la lista real de
/// documentos que expone `GET /me/documents`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Identidad,
    TarjetaPropiedad,
}

/// Estado de revision de un documento de verificacion.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Pending,
    Approved,
    Rejected,
}

/// Estado de un documento tal como lo devuelve `GET /me/documents`: `status`
/// es `None` cuando el conductor todavia no lo subio (el backend lo publica
/// igual, con el valor en `null`, para que la UI conozca los tipos exigidos
/// sin adivinarlos).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DriverDocumentStatus {
    #[serde(rename = "type")]
    pub document_type: DocumentType,
    pub status: Option<DocumentStatus>,
    pub rejection_reason: Option<String>,
    pub uploaded_at: Option<String>,
}

/// Estado de verificacion general del conductor
/// (`openapi.yaml#/components/schemas/DriverVerification`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
}

/// `GET /api/v1/me/documents` — estado de verificacion del conductor
/// autenticado.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DriverVerification {
    pub verification_status: VerificationStatus,
    pub documents: Vec<DriverDocumentStatus>,
}

/// Respuesta de `POST /api/v1/me/documents`: el documento recien subido
/// (`openapi.yaml#/components/schemas/DriverDocument`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UploadedDriverDocument {
    #[serde(rename = "type")]
    pub document_type: DocumentType,
    pub status: DocumentStatus,
    pub uploaded_at: String,
}

/// Categoria del vehiculo (`openapi.yaml#/components/schemas/Vehicle`,
/// historia tecnica #75 del backend). Reemplaza el antiguo campo `model`
/// (texto libre): el anio ya identifica el modelo puntual, y lo que importa
/// operativamente es si el vehiculo es un motocarro o una motocarga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VehicleType {
    Motocarro,
    Motocarga,
}

/// Body de `POST /api/v1/me/vehicle`
/// (`openapi.yaml#/components/schemas/VehicleRegistrationRequest`).
///
/// `plate` viaja ya normalizada (recortada y en mayusculas): el backend hace
/// lo mismo antes de validar (`RegisterVehicleRequest::prepareForValidation`
/// en `Back_App_MotoCarros`), asi que el cliente lo hace antes de mandar la
/// request para que el valor mostrado y el guardado coincidan (issue #11).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RegisterVehiclePayload {
    pub plate: String,
    #[serde(rename = "type")]
    pub vehicle_type: VehicleType,
    pub year: u16,
}

/// `openapi.yaml#/components/schemas/Vehicle` — respuesta de
/// `POST /api/v1/me/vehicle` (issue #11) y tambien de `GET`/`PATCH
/// /api/v1/me/vehicle` (issue #12): las tres operaciones comparten el mismo
/// `VehicleResource` del lado del backend. Solo estos tres campos: no expone
/// `id`/`user_id`/timestamps.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Vehicle {
    pub plate: String,
    #[serde(rename = "type")]
    pub vehicle_type: VehicleType,
    pub year: u16,
}

/// Body de `PATCH /api/v1/me/vehicle` (issue #12).
///
/// PATCH parcial, mismo criterio que `UpdateProfilePayload` (issue #10):
/// cada campo ausente se omite del JSON en vez de viajar como `null`, para
/// que el backend conserve el valor actual en vez de interpretarlo como
/// "borrar este dato".
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct UpdateVehiclePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub vehicle_type: Option<VehicleType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
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
/// conductor asignado (historia #22, fuera de alcance de #15): ese caso no
/// cancela el viaje, solo lo devuelve al pool sin conductor. `#[serde(flatten)]`
/// reutiliza `Ride` en vez de repetir sus campos.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RideCancellation {
    #[serde(flatten)]
    pub ride: Ride,
    pub cancellation_fee_applies: Option<bool>,
}

/// `openapi.yaml#/components/schemas/RideReceipt` — respuesta de
/// `GET /api/v1/rides/{ride}/receipt` (historia #25): el desglose que
/// produjo el cobro publicado en `Ride.payment`/`Ride.final_fare`. A
/// diferencia de `Payment` embebido en `Ride`, repite `currency` y `total`:
/// este es el recurso principal de la respuesta y tiene que bastarse solo,
/// sin depender de que el cliente ya tenga el viaje cargado.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RideReceipt {
    pub ride_id: u64,
    pub currency: String,
    pub base_fare: i64,
    pub distance_fare: i64,
    pub time_fare: i64,
    pub waiting_fee: i64,
    pub subtotal: i64,
    pub minimum_applied: bool,
    pub total: i64,
    pub payment_status: PaymentStatus,
    pub completed_at: String,
}

/// Body de `POST /api/v1/rides/{ride}/rate-driver` (historia #26). `comment`
/// se omite del JSON cuando es `None` en vez de mandarse como `null`: el
/// backend lo declara `nullable` pero no `required`, y el resto de los
/// payloads de esta app (p.ej. `UpdateVehiclePayload`) siguen el mismo
/// criterio para campos opcionales.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RateDriverPayload {
    pub score: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// `openapi.yaml#/components/schemas/RideRating` — respuesta de
/// `POST /api/v1/rides/{ride}/rate-driver` (historia #26).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RideRating {
    pub ride_id: u64,
    pub score: u8,
    pub comment: Option<String>,
    pub rated_at: String,
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

/// `openapi.yaml#/components/schemas/DriverEarningsSummary` — respuesta de
/// `GET /api/v1/me/earnings` (historia #29). `from`/`to` vuelven tal como los
/// normalizo el backend (`toDateString()`, formato `YYYY-MM-DD`), no
/// necesariamente iguales en formato al string que mando el cliente en la
/// query string. `total_earned` es un entero en la unidad minima de
/// `currency`, mismo criterio que `RideEstimate::estimated_fare`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DriverEarningsSummary {
    pub from: String,
    pub to: String,
    pub currency: String,
    pub total_earned: i64,
    pub completed_rides: u32,
}

/// Payload del evento `ride.requested`, publicado sobre el canal privado
/// `driver.{id}` (`id` = `User.id` del conductor, no el de `driver_profiles`)
/// cuando hay un viaje nuevo cerca (issue #16). Sin envelope `data`: viaja tal
/// cual lo publica `app/Events/Realtime/RideRequested.php` de
/// `Back_App_MotoCarros` dentro del frame de Pusher, no como respuesta HTTP.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NearbyRideRequest {
    pub ride_id: u64,
    pub origin: Coordinates,
    pub destination: Coordinates,
    pub currency: String,
    pub estimated_fare: i64,
}

/// Payload del evento `ride.unavailable` sobre el mismo canal `driver.{id}`:
/// otro conductor acepto `ride_id` primero, asi que ya no esta disponible
/// (`app/Events/Realtime/RideNoLongerAvailable.php` de
/// `Back_App_MotoCarros`, issue #16).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct RideNoLongerAvailable {
    pub ride_id: u64,
}

/// Aplica un evento del canal `driver.{id}` (issue #16) a la lista de
/// solicitudes cercanas que ve un conductor, deduplicando por `ride_id`.
///
/// Logica pura y sincronica a proposito: es la maquina de estados que
/// consume `NearbyRidesList` (`moto_ui`) en cada vuelta de su loop de
/// sondeo sobre `RealtimeClient::poll_events()`, extraida de la pantalla
/// para poder testearla sin Dioxus ni un socket real (ver review de la PR
/// del issue #16: dos bugs de esta clase — `Reconnecting` que nunca
/// reconectaba y `subscribe()` reintentado para siempre tras un fallo de
/// auth — se detectaron solo por revision manual). `event_name`/`data` ya
/// vienen filtrados por `channel` en el caller; un `data` que no
/// deserializa al tipo esperado (frame corrupto o version del protocolo
/// que este cliente no entiende) se ignora sin error, igual que
/// `RealtimeClient::handle_frame`.
pub fn apply_nearby_ride_event(
    requests: &mut Vec<NearbyRideRequest>,
    event_name: &str,
    data: &str,
) {
    match event_name {
        "ride.requested" => {
            if let Ok(request) = serde_json::from_str::<NearbyRideRequest>(data)
                && !requests.iter().any(|r| r.ride_id == request.ride_id)
            {
                requests.push(request);
            }
        }
        "ride.unavailable" => {
            if let Ok(gone) = serde_json::from_str::<RideNoLongerAvailable>(data) {
                requests.retain(|r| r.ride_id != gone.ride_id);
            }
        }
        _ => {}
    }
}

/// Estado que arma la pantalla de seguimiento en tiempo real de un viaje
/// activo (issue #20): el viaje tal como lo devolvio el ultimo fetch a
/// `GET /api/v1/rides/{ride}` (`ApiClient::get_ride`), mas la ultima
/// posicion del conductor que llego por el canal `ride.{id}`. El fetch
/// nunca trae la ubicacion — solo viaja por el evento `location.updated`,
/// ver `RideResource` de `Back_App_MotoCarros` — asi que vive aparte del
/// resto de los campos del viaje en vez de ser un campo mas de `Ride`.
#[derive(Debug, Clone, PartialEq)]
pub struct RideTracking {
    pub ride: Ride,
    pub driver_location: Option<Coordinates>,
}

impl RideTracking {
    pub fn new(ride: Ride) -> Self {
        Self {
            ride,
            driver_location: None,
        }
    }
}

/// Payload del evento `status.changed` sobre el canal privado `ride.{id}`
/// (issue #20): trae el estado **nuevo** y el conductor asignado por id
/// (`app/Events/Realtime/RideStatusChanged.php` de `Back_App_MotoCarros`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct RideStatusChangedEvent {
    status: RideStatus,
    driver_id: Option<u64>,
}

/// Payload del evento `location.updated` sobre el mismo canal: la posicion
/// actual del conductor asignado, publicada por `ShareLocationPanel` (issue
/// #19) del lado del conductor
/// (`app/Events/Realtime/DriverLocationUpdated.php` de
/// `Back_App_MotoCarros`).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
struct DriverLocationUpdatedEvent {
    driver_id: u64,
    latitude: f64,
    longitude: f64,
}

/// Aplica un evento del canal `ride.{id}` (issue #20) al estado de
/// seguimiento que consume la pantalla de tracking (`moto_ui`). Logica
/// pura y sincronica, mismo criterio que `apply_nearby_ride_event`:
/// extraida del componente para poder testearla sin Dioxus ni un socket
/// real. `event_name`/`data` ya vienen filtrados por `channel` en el
/// caller; un `event_name` desconocido o un `data` que no deserializa al
/// tipo esperado se ignora sin error (frame corrupto, o version del
/// protocolo que este cliente no entiende todavia), igual que
/// `RealtimeClient::handle_frame`.
///
/// `location.updated` se ignora si el `driver_id` del evento no coincide
/// con el conductor asignado al viaje: no deberia pasar
/// (`RidePolicy::shareLocation` en el backend exige ser el conductor
/// asignado), pero la pantalla no confia ciegamente en lo que llega por el
/// canal.
///
/// `status.changed` solo trae el id del conductor, no su nombre. Mientras
/// coincida con el que ya tenia el viaje (o siga sin haber ninguno) no
/// cambia nada; si aparece un conductor nuevo se guarda con el nombre
/// vacio en vez de inventarlo — la proxima vez que la pantalla vuelva a
/// pedir el viaje por HTTP (`GET /api/v1/rides/{ride}`, tras reconectar)
/// lo completa.
pub fn apply_ride_tracking_event(tracking: &mut RideTracking, event_name: &str, data: &str) {
    match event_name {
        "status.changed" => {
            if let Ok(event) = serde_json::from_str::<RideStatusChangedEvent>(data) {
                tracking.ride.status = event.status;
                match event.driver_id {
                    None => tracking.ride.driver = None,
                    Some(id) => {
                        if tracking.ride.driver.as_ref().map(|d| d.id) != Some(id) {
                            tracking.ride.driver = Some(RideDriver {
                                id,
                                name: String::new(),
                            });
                        }
                    }
                }
            }
        }
        "location.updated" => {
            if let Ok(event) = serde_json::from_str::<DriverLocationUpdatedEvent>(data)
                && tracking.ride.driver.as_ref().map(|d| d.id) == Some(event.driver_id)
            {
                tracking.driver_location = Some(Coordinates {
                    latitude: event.latitude,
                    longitude: event.longitude,
                });
            }
        }
        _ => {}
    }
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
                    "phone_verified": false,
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
    fn register_vehicle_payload_serializes_plate_type_and_year() {
        let payload = RegisterVehiclePayload {
            plate: "ABC12D".to_string(),
            vehicle_type: VehicleType::Motocarro,
            year: 2022,
        };

        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "plate": "ABC12D",
                "type": "motocarro",
                "year": 2022,
            })
        );
    }

    #[test]
    fn deserializes_vehicle_envelope() {
        let json = r#"{
            "data": {
                "plate": "ABC12D",
                "type": "motocarro",
                "year": 2022
            }
        }"#;

        let envelope: DataEnvelope<Vehicle> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.plate, "ABC12D");
        assert_eq!(envelope.data.vehicle_type, VehicleType::Motocarro);
        assert_eq!(envelope.data.year, 2022);
    }

    #[test]
    fn deserializes_driver_verification_envelope_with_a_document_not_uploaded_yet() {
        let json = r#"{
            "data": {
                "verification_status": "pending",
                "documents": [
                    {
                        "type": "identidad",
                        "status": "approved",
                        "rejection_reason": null,
                        "uploaded_at": "2026-09-04T15:00:00.000000Z"
                    },
                    {
                        "type": "tarjeta_propiedad",
                        "status": null,
                        "rejection_reason": null,
                        "uploaded_at": null
                    }
                ]
            }
        }"#;

        let envelope: DataEnvelope<DriverVerification> = serde_json::from_str(json).unwrap();

        assert_eq!(
            envelope.data.verification_status,
            VerificationStatus::Pending
        );
        assert_eq!(envelope.data.documents.len(), 2);
        assert_eq!(
            envelope.data.documents[0].document_type,
            DocumentType::Identidad
        );
        assert_eq!(
            envelope.data.documents[0].status,
            Some(DocumentStatus::Approved)
        );
        assert_eq!(
            envelope.data.documents[1].document_type,
            DocumentType::TarjetaPropiedad
        );
        assert_eq!(envelope.data.documents[1].status, None);
    }

    #[test]
    fn deserializes_uploaded_driver_document_envelope() {
        let json = r#"{
            "data": {
                "type": "identidad",
                "status": "pending",
                "uploaded_at": "2026-09-04T15:00:00.000000Z"
            }
        }"#;

        let envelope: DataEnvelope<UploadedDriverDocument> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.document_type, DocumentType::Identidad);
        assert_eq!(envelope.data.status, DocumentStatus::Pending);
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
    fn deserializes_driver_earnings_summary_envelope() {
        let json = r#"{
            "data": {
                "from": "2026-07-01",
                "to": "2026-07-31",
                "currency": "COP",
                "total_earned": 17500,
                "completed_rides": 2
            }
        }"#;

        let envelope: DataEnvelope<DriverEarningsSummary> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.from, "2026-07-01");
        assert_eq!(envelope.data.to, "2026-07-31");
        assert_eq!(envelope.data.currency, "COP");
        assert_eq!(envelope.data.total_earned, 17500);
        assert_eq!(envelope.data.completed_rides, 2);
    }

    #[test]
    fn deserializes_ride_receipt_envelope() {
        let json = r#"{
            "data": {
                "ride_id": 1,
                "currency": "COP",
                "base_fare": 1500,
                "distance_fare": 5937,
                "time_fare": 1000,
                "waiting_fee": 0,
                "subtotal": 8437,
                "minimum_applied": false,
                "total": 8450,
                "payment_status": "paid",
                "completed_at": "2026-07-31T14:19:05+00:00"
            }
        }"#;

        let envelope: DataEnvelope<RideReceipt> = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.data.ride_id, 1);
        assert_eq!(envelope.data.currency, "COP");
        assert_eq!(envelope.data.total, 8450);
        assert!(!envelope.data.minimum_applied);
        assert_eq!(envelope.data.payment_status, PaymentStatus::Paid);
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

    #[test]
    fn deserializes_nearby_ride_request_without_a_data_envelope() {
        let json = r#"{
            "ride_id": 7,
            "origin": {"latitude": 4.710989, "longitude": -74.072092},
            "destination": {"latitude": 4.698, "longitude": -74.061},
            "currency": "COP",
            "estimated_fare": 8850
        }"#;

        let request: NearbyRideRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.ride_id, 7);
        assert_eq!(request.currency, "COP");
        assert_eq!(request.estimated_fare, 8850);
        assert_eq!(request.origin.latitude, 4.710989);
    }

    #[test]
    fn deserializes_ride_no_longer_available() {
        let json = r#"{"ride_id": 7}"#;

        let event: RideNoLongerAvailable = serde_json::from_str(json).unwrap();

        assert_eq!(event.ride_id, 7);
    }

    fn nearby_ride_request_json(ride_id: u64) -> String {
        serde_json::json!({
            "ride_id": ride_id,
            "origin": {"latitude": 4.710989, "longitude": -74.072092},
            "destination": {"latitude": 4.698, "longitude": -74.061},
            "currency": "COP",
            "estimated_fare": 8850,
        })
        .to_string()
    }

    #[test]
    fn apply_nearby_ride_event_adds_a_ride_requested_event() {
        let mut requests = Vec::new();

        apply_nearby_ride_event(
            &mut requests,
            "ride.requested",
            &nearby_ride_request_json(7),
        );

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].ride_id, 7);
    }

    #[test]
    fn apply_nearby_ride_event_deduplicates_by_ride_id() {
        let mut requests = Vec::new();
        apply_nearby_ride_event(
            &mut requests,
            "ride.requested",
            &nearby_ride_request_json(7),
        );

        apply_nearby_ride_event(
            &mut requests,
            "ride.requested",
            &nearby_ride_request_json(7),
        );

        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn apply_nearby_ride_event_removes_on_ride_unavailable() {
        let mut requests = Vec::new();
        apply_nearby_ride_event(
            &mut requests,
            "ride.requested",
            &nearby_ride_request_json(7),
        );
        apply_nearby_ride_event(
            &mut requests,
            "ride.requested",
            &nearby_ride_request_json(9),
        );

        apply_nearby_ride_event(&mut requests, "ride.unavailable", r#"{"ride_id": 7}"#);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].ride_id, 9);
    }

    #[test]
    fn apply_nearby_ride_event_ignores_an_unknown_event_name() {
        let mut requests = Vec::new();

        apply_nearby_ride_event(&mut requests, "pusher_internal:subscription_succeeded", "");

        assert!(requests.is_empty());
    }

    #[test]
    fn apply_nearby_ride_event_ignores_malformed_data_without_panicking() {
        let mut requests = Vec::new();

        apply_nearby_ride_event(&mut requests, "ride.requested", "not json at all");
        apply_nearby_ride_event(&mut requests, "ride.unavailable", "not json at all");

        assert!(requests.is_empty());
    }

    fn sample_tracked_ride() -> Ride {
        Ride {
            id: 1,
            status: RideStatus::Requested,
            origin: Coordinates {
                latitude: 4.710989,
                longitude: -74.072092,
            },
            destination: Coordinates {
                latitude: 4.698,
                longitude: -74.061,
            },
            distance_meters: 7421,
            duration_seconds: 842,
            currency: "COP".to_string(),
            estimated_fare: 8850,
            driver: None,
            requested_at: "2026-07-31T14:03:21+00:00".to_string(),
            started_at: None,
            completed_at: None,
            final_fare: None,
            payment: None,
        }
    }

    #[test]
    fn apply_ride_tracking_event_updates_the_status_on_status_changed() {
        let mut tracking = RideTracking::new(sample_tracked_ride());

        apply_ride_tracking_event(
            &mut tracking,
            "status.changed",
            r#"{"status": "accepted", "driver_id": 42}"#,
        );

        assert_eq!(tracking.ride.status, RideStatus::Accepted);
    }

    #[test]
    fn apply_ride_tracking_event_assigns_a_new_driver_by_id_without_a_name() {
        let mut tracking = RideTracking::new(sample_tracked_ride());

        apply_ride_tracking_event(
            &mut tracking,
            "status.changed",
            r#"{"status": "accepted", "driver_id": 42}"#,
        );

        assert_eq!(
            tracking.ride.driver,
            Some(RideDriver {
                id: 42,
                name: String::new(),
            })
        );
    }

    #[test]
    fn apply_ride_tracking_event_keeps_the_known_driver_name_when_the_id_matches() {
        let mut ride = sample_tracked_ride();
        ride.status = RideStatus::Accepted;
        ride.driver = Some(RideDriver {
            id: 42,
            name: "Carlos Perez".to_string(),
        });
        let mut tracking = RideTracking::new(ride);

        apply_ride_tracking_event(
            &mut tracking,
            "status.changed",
            r#"{"status": "in_progress", "driver_id": 42}"#,
        );

        assert_eq!(
            tracking.ride.driver,
            Some(RideDriver {
                id: 42,
                name: "Carlos Perez".to_string(),
            })
        );
    }

    #[test]
    fn apply_ride_tracking_event_clears_the_driver_when_the_event_carries_none() {
        let mut ride = sample_tracked_ride();
        ride.driver = Some(RideDriver {
            id: 42,
            name: "Carlos Perez".to_string(),
        });
        let mut tracking = RideTracking::new(ride);

        apply_ride_tracking_event(
            &mut tracking,
            "status.changed",
            r#"{"status": "requested", "driver_id": null}"#,
        );

        assert_eq!(tracking.ride.driver, None);
    }

    #[test]
    fn apply_ride_tracking_event_updates_the_driver_location_on_location_updated() {
        let mut ride = sample_tracked_ride();
        ride.driver = Some(RideDriver {
            id: 42,
            name: "Carlos Perez".to_string(),
        });
        let mut tracking = RideTracking::new(ride);

        apply_ride_tracking_event(
            &mut tracking,
            "location.updated",
            r#"{"driver_id": 42, "latitude": 4.71, "longitude": -74.07}"#,
        );

        assert_eq!(
            tracking.driver_location,
            Some(Coordinates {
                latitude: 4.71,
                longitude: -74.07,
            })
        );
    }

    #[test]
    fn apply_ride_tracking_event_ignores_location_from_an_unassigned_driver() {
        let mut ride = sample_tracked_ride();
        ride.driver = Some(RideDriver {
            id: 42,
            name: "Carlos Perez".to_string(),
        });
        let mut tracking = RideTracking::new(ride);

        apply_ride_tracking_event(
            &mut tracking,
            "location.updated",
            r#"{"driver_id": 99, "latitude": 4.71, "longitude": -74.07}"#,
        );

        assert_eq!(tracking.driver_location, None);
    }

    #[test]
    fn apply_ride_tracking_event_ignores_an_unknown_event_name() {
        let mut tracking = RideTracking::new(sample_tracked_ride());

        apply_ride_tracking_event(&mut tracking, "pusher_internal:subscription_succeeded", "");

        assert_eq!(tracking, RideTracking::new(sample_tracked_ride()));
    }

    #[test]
    fn apply_ride_tracking_event_ignores_malformed_data_without_panicking() {
        let mut tracking = RideTracking::new(sample_tracked_ride());

        apply_ride_tracking_event(&mut tracking, "status.changed", "not json at all");
        apply_ride_tracking_event(&mut tracking, "location.updated", "not json at all");

        assert_eq!(tracking, RideTracking::new(sample_tracked_ride()));
    }
}
