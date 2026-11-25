use crate::discovery::Endpoint;
use serde_json::{Value, json};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Warcraft III rejected the local UI connection")]
    Connect,
    #[error("Local UI connection closed or failed")]
    Disconnected,
    #[error("Invalid JSON from the local UI")]
    Json(#[from] serde_json::Error),
}

pub struct Client(WebSocket<TcpStream>, Duration);
#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
impl Client {
    pub fn connect(endpoint: &Endpoint) -> Result<Self, TransportError> {
        for &port in &endpoint.ports {
            let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else {
                continue;
            };
            // Small lobby replies must not wait for TCP packet coalescing.
            let _ = stream.set_nodelay(true);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let url = format!("ws://127.0.0.1:{port}/webui-socket/{}", endpoint.session);
            let config = WebSocketConfig::default()
                .max_message_size(Some(8 * 1024 * 1024))
                .max_frame_size(Some(8 * 1024 * 1024));
            if let Ok((socket, _)) =
                tungstenite::client::client_with_config(url, stream, Some(config))
            {
                let _ = socket
                    .get_ref()
                    .set_read_timeout(Some(Duration::from_millis(250)));
                return Ok(Self(socket, Duration::from_millis(250)));
            }
        }
        Err(TransportError::Connect)
    }

    pub fn send(&mut self, command: &str, payload: Value) -> Result<(), TransportError> {
        let text = json!({"type": "webui", "message": command, "payload": payload}).to_string();
        crate::protocol_log::log("SEND", &text); // TEMP protocol diagnostics
        self.0
            .send(Message::Text(text.into()))
            .map_err(|_| TransportError::Disconnected)
    }

    pub fn receive(&mut self) -> Result<Option<Value>, TransportError> {
        match self.0.read() {
            Ok(Message::Text(text)) => {
                crate::protocol_log::log("RECV", &text); // TEMP: before deserialization
                Ok(Some(serde_json::from_str(&text)?))
            }
            Ok(Message::Ping(_)) => {
                self.0.flush().map_err(|_| TransportError::Disconnected)?;
                // A keepalive is not an idle input queue. Callers draining old
                // callbacks must keep reading before requesting a fresh roster.
                Ok(Some(Value::Null))
            }
            Ok(Message::Close(_)) => {
                crate::protocol_log::log_note("RECV", "<WebSocket Close frame>");
                Err(TransportError::Disconnected)
            }
            Ok(_) => {
                crate::protocol_log::log_note("RECV", "<non-text WebSocket frame>");
                Ok(Some(Value::Null))
            }
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(_) => Err(TransportError::Disconnected),
        }
    }

    pub(crate) fn receive_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Value>, TransportError> {
        if timeout != self.1 {
            self.0
                .get_ref()
                .set_read_timeout(Some(timeout))
                .map_err(|_| TransportError::Disconnected)?;
            self.1 = timeout;
        }
        self.receive()
    }

    /// Drain queued callbacks without paying the normal idle polling delay.
    pub fn receive_available(&mut self) -> Result<Option<Value>, TransportError> {
        self.0
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(5)))
            .map_err(|_| TransportError::Disconnected)?;
        let received = self.receive();
        self.0
            .get_ref()
            .set_read_timeout(Some(self.1))
            .map_err(|_| TransportError::Disconnected)?;
        received
    }
}
