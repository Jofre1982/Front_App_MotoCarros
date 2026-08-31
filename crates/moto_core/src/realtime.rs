//! Cliente de WebSockets hacia el servidor Reverb de `Back_App_MotoCarros`
//! (issue #5), base para las historias de tracking en tiempo real (estado
//! del viaje, ubicacion del conductor).
//!
//! Habla el protocolo Pusher que sirve Reverb: tras el handshake
//! (`pusher:connection_established`), cada canal privado (`private-ride.7`,
//! `private-driver.3`, ver `routes/channels.php` de `Back_App_MotoCarros`)
//! se suscribe firmando `socket_id` + `channel_name` contra
//! `POST /api/v1/broadcasting/auth` (`ApiClient::authenticate_broadcast_channel`)
//! y mandando esa firma en un frame `pusher:subscribe`.
//!
//! El transporte real (`ewebsock`, que soporta nativo y `wasm32-unknown-unknown`
//! con la misma API) esta detras del trait privado `WsTransport` para poder
//! testear la maquina de estados con un doble de prueba, sin abrir un socket
//! real.

use serde::Deserialize;

use crate::api::{ApiClient, BroadcastAuthError};
use crate::models::AuthToken;

/// Prefijo que exige el protocolo Pusher para cualquier canal privado (ver
/// `routes/channels.php` de `Back_App_MotoCarros`: "El cliente los pide con
/// el prefijo `private-`").
const PRIVATE_CHANNEL_PREFIX: &str = "private-";

trait WsTransport {
    fn send_text(&mut self, text: String);
    fn try_recv(&mut self) -> Option<ewebsock::WsEvent>;
    fn close(&mut self);
}

struct EwebsockTransport {
    sender: ewebsock::WsSender,
    receiver: ewebsock::WsReceiver,
}

impl WsTransport for EwebsockTransport {
    fn send_text(&mut self, text: String) {
        self.sender.send(ewebsock::WsMessage::Text(text));
    }

    fn try_recv(&mut self) -> Option<ewebsock::WsEvent> {
        self.receiver.try_recv()
    }

    fn close(&mut self) {
        self.sender.close();
    }
}

/// URL del servidor Reverb (p.ej. `wss://host/app/{key}`), inyectada por el
/// binario de plataforma (`web`/`mobile`) via contexto de Dioxus, igual que
/// `ApiClient` (ver `.claude/STANDARDS.md`). A diferencia de la URL base de
/// la API, esta URL incluye la app key de Reverb, que es un secreto por
/// entorno sin valor por defecto (`REVERB_APP_KEY` en el `.env.example` de
/// `Back_App_MotoCarros`) — por eso no hay un fallback de desarrollo local
/// hardcodeado como con `MOTOYA_API_BASE_URL`. `ws_url` es `None` cuando el
/// entorno no configuro `MOTOYA_WS_URL`; las pantallas que dependan de
/// tiempo real (issue #16 y siguientes) deben mostrar un estado explicito en
/// vez de intentar conectar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RealtimeConfig {
    pub ws_url: Option<String>,
}

/// Estado de la conexion de WebSocket con Reverb.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// Se abrio la conexion, todavia no llego `pusher:connection_established`.
    Connecting,
    /// Handshake completo: `socket_id()` ya tiene un valor y se puede
    /// suscribir a canales.
    Connected,
    /// El socket se cerro o fallo. `attempt` cuenta cuantas veces seguidas
    /// paso esto sin volver a `Connected` en el medio.
    Reconnecting { attempt: u32 },
}

/// Un evento de canal ya desenvuelto del frame de Pusher (`data` viaja
/// stringificado en el protocolo; aca ya se extrajo del frame exterior,
/// pero sigue siendo el JSON crudo que public el backend — el caller lo
/// deserializa al tipo que le corresponda segun `channel`/`event`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelEvent {
    /// Nombre completo del canal, con el prefijo `private-` incluido.
    pub channel: String,
    pub event: String,
    /// JSON crudo (como string) que publico el backend, o vacio si el frame
    /// no traia `data` (p.ej. `pusher_internal:subscription_succeeded`).
    pub data: String,
}

/// Fallos posibles de `RealtimeClient::subscribe`.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscribeError {
    /// Todavia no llego el handshake (`pusher:connection_established`), asi
    /// que no hay `socket_id` con el que firmar la suscripcion.
    NotConnected,
    Auth(BroadcastAuthError),
}

impl std::fmt::Display for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubscribeError::NotConnected => {
                write!(f, "Todavia no se establecio la conexion en tiempo real.")
            }
            SubscribeError::Auth(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SubscribeError {}

#[derive(Debug, Deserialize)]
struct PusherFrame {
    event: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectionEstablishedData {
    socket_id: String,
}

/// Cliente de tiempo real: mantiene una conexion de WebSocket con Reverb y
/// expone una forma de suscribirse a canales privados y recibir sus eventos.
///
/// No hace reconexion automatica con backoff: el crate no tiene runtime
/// propio ni una forma cross-platform de dormir un timer (ver
/// `.claude/STANDARDS.md`, separacion de codigo especifico de plataforma).
/// Cuando la conexion se cae, el caller debe llamar `reconnect()` y volver a
/// suscribirse a `pending_subscriptions()` una vez que `state()` vuelva a
/// `Connected`.
pub struct RealtimeClient {
    api: ApiClient,
    ws_url: String,
    transport: Box<dyn WsTransport>,
    state: ConnectionState,
    socket_id: Option<String>,
    subscriptions: Vec<String>,
}

impl RealtimeClient {
    /// Abre la conexion de WebSocket hacia `ws_url` (p.ej. `wss://host/app/{key}`,
    /// ver configuracion de Reverb en `Back_App_MotoCarros`).
    pub fn connect(api: ApiClient, ws_url: impl Into<String>) -> Result<Self, String> {
        let ws_url = ws_url.into();
        let (sender, receiver) = ewebsock::connect(ws_url.clone(), ewebsock::Options::default())?;
        Ok(Self::with_transport(
            api,
            ws_url,
            Box::new(EwebsockTransport { sender, receiver }),
        ))
    }

    fn with_transport(api: ApiClient, ws_url: String, transport: Box<dyn WsTransport>) -> Self {
        Self {
            api,
            ws_url,
            transport,
            state: ConnectionState::Connecting,
            socket_id: None,
            subscriptions: Vec::new(),
        }
    }

    pub fn state(&self) -> ConnectionState {
        self.state.clone()
    }

    pub fn socket_id(&self) -> Option<&str> {
        self.socket_id.as_deref()
    }

    /// Canales a los que se pidio suscripcion, para volver a suscribirse
    /// tras un `reconnect()`.
    pub fn pending_subscriptions(&self) -> &[String] {
        &self.subscriptions
    }

    /// Suscribe a `channel` (p.ej. `"ride.7"`, sin el prefijo `private-`: lo
    /// agrega este metodo, ver `routes/channels.php` de
    /// `Back_App_MotoCarros`). Firma la suscripcion contra
    /// `POST /api/v1/broadcasting/auth` y manda el frame `pusher:subscribe`.
    pub async fn subscribe(
        &mut self,
        token: &AuthToken,
        channel: &str,
    ) -> Result<(), SubscribeError> {
        let socket_id = self.socket_id.clone().ok_or(SubscribeError::NotConnected)?;
        let channel_name = format!("{PRIVATE_CHANNEL_PREFIX}{channel}");

        let auth = self
            .api
            .authenticate_broadcast_channel(token, &socket_id, &channel_name)
            .await
            .map_err(SubscribeError::Auth)?;

        let frame = serde_json::json!({
            "event": "pusher:subscribe",
            "data": {
                "channel": channel_name,
                "auth": auth.auth,
            },
        });
        self.transport.send_text(frame.to_string());

        if !self.subscriptions.contains(&channel_name) {
            self.subscriptions.push(channel_name);
        }

        Ok(())
    }

    /// Cierra la conexion actual y abre una nueva hacia la misma URL.
    /// No vuelve a suscribirse a nada: el caller debe hacerlo con
    /// `pending_subscriptions()` una vez que `state()` sea `Connected`.
    pub fn reconnect(&mut self) -> Result<(), String> {
        self.transport.close();
        let (sender, receiver) =
            ewebsock::connect(self.ws_url.clone(), ewebsock::Options::default())?;
        self.transport = Box::new(EwebsockTransport { sender, receiver });
        self.socket_id = None;
        self.state = ConnectionState::Connecting;
        Ok(())
    }

    /// Drena los eventos pendientes del transporte, actualiza el estado
    /// interno (handshake, desconexion) y devuelve los eventos de canal
    /// listos para consumir. Pensado para llamarse desde el loop de
    /// renderizado de la pantalla que este escuchando (ver historias de
    /// tracking en tiempo real, issue #19/#20).
    pub fn poll_events(&mut self) -> Vec<ChannelEvent> {
        let mut events = Vec::new();

        while let Some(event) = self.transport.try_recv() {
            match event {
                ewebsock::WsEvent::Opened => {
                    self.state = ConnectionState::Connecting;
                }
                ewebsock::WsEvent::Closed | ewebsock::WsEvent::Error(_) => {
                    let attempt = match self.state {
                        ConnectionState::Reconnecting { attempt } => attempt + 1,
                        _ => 1,
                    };
                    self.state = ConnectionState::Reconnecting { attempt };
                    self.socket_id = None;
                }
                ewebsock::WsEvent::Message(ewebsock::WsMessage::Text(text)) => {
                    if let Some(channel_event) = self.handle_frame(&text) {
                        events.push(channel_event);
                    }
                }
                ewebsock::WsEvent::Message(_) => {}
            }
        }

        events
    }

    /// Frame malformado (JSON invalido) se ignora sin panic: no hay forma de
    /// distinguir un frame corrupto de una version futura del protocolo que
    /// este cliente todavia no entiende, y ninguno de los dos casos deberia
    /// tumbar la conexion.
    fn handle_frame(&mut self, text: &str) -> Option<ChannelEvent> {
        let frame: PusherFrame = serde_json::from_str(text).ok()?;

        if frame.event == "pusher:connection_established" {
            if let Some(data) = &frame.data
                && let Ok(established) = serde_json::from_str::<ConnectionEstablishedData>(data)
            {
                self.socket_id = Some(established.socket_id);
                self.state = ConnectionState::Connected;
            }
            return None;
        }

        let channel = frame.channel?;
        Some(ChannelEvent {
            channel,
            event: frame.event,
            data: frame.data.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FakeTransport {
        incoming: VecDeque<ewebsock::WsEvent>,
        sent: Vec<String>,
        closed: bool,
    }

    impl FakeTransport {
        fn new(incoming: Vec<ewebsock::WsEvent>) -> Self {
            Self {
                incoming: incoming.into(),
                sent: Vec::new(),
                closed: false,
            }
        }
    }

    impl WsTransport for FakeTransport {
        fn send_text(&mut self, text: String) {
            self.sent.push(text);
        }

        fn try_recv(&mut self) -> Option<ewebsock::WsEvent> {
            self.incoming.pop_front()
        }

        fn close(&mut self) {
            self.closed = true;
        }
    }

    fn connection_established_frame(socket_id: &str) -> ewebsock::WsEvent {
        let data = serde_json::json!({ "socket_id": socket_id, "activity_timeout": 30 });
        let frame = serde_json::json!({
            "event": "pusher:connection_established",
            "data": data.to_string(),
        });
        ewebsock::WsEvent::Message(ewebsock::WsMessage::Text(frame.to_string()))
    }

    fn sample_token() -> AuthToken {
        AuthToken {
            access_token: "jwt-token".to_string(),
            token_type: "bearer".to_string(),
            expires_in: Some(900),
        }
    }

    fn client_with_events(events: Vec<ewebsock::WsEvent>) -> RealtimeClient {
        RealtimeClient::with_transport(
            ApiClient::new("https://unreachable.invalid"),
            "wss://unreachable.invalid/app/key".to_string(),
            Box::new(FakeTransport::new(events)),
        )
    }

    #[test]
    fn starts_connecting_without_a_socket_id() {
        let client = client_with_events(vec![]);

        assert_eq!(client.state(), ConnectionState::Connecting);
        assert_eq!(client.socket_id(), None);
    }

    #[test]
    fn poll_events_extracts_the_socket_id_from_the_handshake() {
        let mut client = client_with_events(vec![connection_established_frame("123456.789012")]);

        let events = client.poll_events();

        assert!(events.is_empty());
        assert_eq!(client.state(), ConnectionState::Connected);
        assert_eq!(client.socket_id(), Some("123456.789012"));
    }

    #[test]
    fn poll_events_ignores_a_malformed_frame_without_panicking() {
        let mut client = client_with_events(vec![ewebsock::WsEvent::Message(
            ewebsock::WsMessage::Text("not json at all".to_string()),
        )]);

        let events = client.poll_events();

        assert!(events.is_empty());
        assert_eq!(client.state(), ConnectionState::Connecting);
    }

    #[test]
    fn poll_events_surfaces_subscription_succeeded_as_a_channel_event() {
        let frame = serde_json::json!({
            "event": "pusher_internal:subscription_succeeded",
            "channel": "private-ride.7",
        });
        let mut client = client_with_events(vec![
            connection_established_frame("123456.789012"),
            ewebsock::WsEvent::Message(ewebsock::WsMessage::Text(frame.to_string())),
        ]);

        let events = client.poll_events();

        assert_eq!(
            events,
            vec![ChannelEvent {
                channel: "private-ride.7".to_string(),
                event: "pusher_internal:subscription_succeeded".to_string(),
                data: String::new(),
            }]
        );
    }

    #[test]
    fn poll_events_decodes_the_stringified_data_of_a_custom_channel_event() {
        let inner_data = serde_json::json!({ "status": "in_progress" });
        let frame = serde_json::json!({
            "event": "RideStatusChanged",
            "channel": "private-ride.7",
            "data": inner_data.to_string(),
        });
        let mut client = client_with_events(vec![ewebsock::WsEvent::Message(
            ewebsock::WsMessage::Text(frame.to_string()),
        )]);

        let events = client.poll_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].channel, "private-ride.7");
        assert_eq!(events[0].event, "RideStatusChanged");
        let decoded: HashMap<String, String> = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(decoded.get("status"), Some(&"in_progress".to_string()));
    }

    #[test]
    fn poll_events_moves_to_reconnecting_on_closed_and_clears_the_socket_id() {
        let mut client = client_with_events(vec![
            connection_established_frame("123456.789012"),
            ewebsock::WsEvent::Closed,
        ]);

        client.poll_events();

        assert_eq!(client.state(), ConnectionState::Reconnecting { attempt: 1 });
        assert_eq!(client.socket_id(), None);
    }

    #[test]
    fn poll_events_increments_the_reconnect_attempt_on_repeated_failures() {
        let mut client = client_with_events(vec![ewebsock::WsEvent::Error("boom".to_string())]);
        client.poll_events();
        assert_eq!(client.state(), ConnectionState::Reconnecting { attempt: 1 });

        client = client_with_events(vec![ewebsock::WsEvent::Closed]);
        client.state = ConnectionState::Reconnecting { attempt: 1 };
        client.poll_events();

        assert_eq!(client.state(), ConnectionState::Reconnecting { attempt: 2 });
    }

    #[tokio::test]
    async fn subscribe_fails_without_sending_anything_when_not_connected_yet() {
        let mut client = client_with_events(vec![]);

        let error = client
            .subscribe(&sample_token(), "ride.7")
            .await
            .unwrap_err();

        assert_eq!(error, SubscribeError::NotConnected);
        assert!(client.pending_subscriptions().is_empty());
    }

    #[tokio::test]
    async fn subscribe_authenticates_and_sends_the_pusher_subscribe_frame() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/broadcasting/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "auth": "motoya-local:8f3c1a2b4d5e6f70",
            })))
            .mount(&server)
            .await;

        let mut client = RealtimeClient::with_transport(
            ApiClient::new(server.uri()),
            "wss://unreachable.invalid/app/key".to_string(),
            Box::new(FakeTransport::new(vec![connection_established_frame(
                "123456.789012",
            )])),
        );
        client.poll_events();

        client.subscribe(&sample_token(), "ride.7").await.unwrap();

        assert_eq!(
            client.pending_subscriptions(),
            &["private-ride.7".to_string()]
        );
    }

    #[tokio::test]
    async fn subscribe_returns_forbidden_when_the_backend_rejects_the_channel() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/broadcasting/auth"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "This action is unauthorized.",
            })))
            .mount(&server)
            .await;

        let mut client = RealtimeClient::with_transport(
            ApiClient::new(server.uri()),
            "wss://unreachable.invalid/app/key".to_string(),
            Box::new(FakeTransport::new(vec![connection_established_frame(
                "123456.789012",
            )])),
        );
        client.poll_events();

        let error = client
            .subscribe(&sample_token(), "driver.3")
            .await
            .unwrap_err();

        assert_eq!(error, SubscribeError::Auth(BroadcastAuthError::Forbidden));
        assert!(client.pending_subscriptions().is_empty());
    }

    #[test]
    fn reconnect_resets_the_socket_id_and_the_state() {
        let mut client = client_with_events(vec![connection_established_frame("123456.789012")]);
        client.poll_events();
        assert_eq!(client.state(), ConnectionState::Connected);

        client.reconnect().unwrap();

        assert_eq!(client.state(), ConnectionState::Connecting);
        assert_eq!(client.socket_id(), None);
    }
}
