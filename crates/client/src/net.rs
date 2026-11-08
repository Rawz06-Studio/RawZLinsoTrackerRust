//! WebSocket connection to a room, with automatic reconnection.

use eframe::egui::Context;
use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};
use tracker_core::{ClientMsg, ErrorCode, ServerMsg, SyncState};

const RETRY_DELAY_SECONDS: f64 = 2.0;

struct Connection {
    sender: WsSender,
    receiver: WsReceiver,
}

pub struct Net {
    ctx: Context,
    url: String,
    room: String,
    name: String,
    connection: Option<Connection>,
    joined: bool,
    retry_at: f64,
    /// A refusal that retrying cannot fix, such as joining with the wrong tracker.
    pub fatal: Option<ErrorCode>,
}

impl Net {
    pub fn new(ctx: Context, url: String, room: String, name: String) -> Self {
        Self {
            ctx,
            url,
            room,
            name,
            connection: None,
            joined: false,
            retry_at: 0.0,
            fatal: None,
        }
    }

    pub fn is_joined(&self) -> bool {
        self.joined
    }

    /// Sends when joined. Returns whether the message left.
    pub fn send(&mut self, msg: &ClientMsg) -> bool {
        match (&mut self.connection, self.joined) {
            (Some(connection), true) => {
                Self::write(connection, msg);
                true
            }
            _ => false,
        }
    }

    fn write(connection: &mut Connection, msg: &ClientMsg) {
        let text = serde_json::to_string(msg).expect("client messages always serialize");
        connection.sender.send(WsMessage::Text(text));
    }

    fn connect(&mut self) {
        let ctx = self.ctx.clone();
        match ewebsock::connect_with_wakeup(&self.url, Options::default(), move || {
            ctx.request_repaint()
        }) {
            Ok((sender, receiver)) => self.connection = Some(Connection { sender, receiver }),
            Err(error) => {
                eprintln!("connection failed: {error}");
                self.schedule_retry();
            }
        }
    }

    fn schedule_retry(&mut self) {
        let now = self.ctx.input(|i| i.time);
        self.retry_at = now + RETRY_DELAY_SECONDS;
    }

    fn lost(&mut self, sync: &mut SyncState) {
        self.connection = None;
        self.joined = false;
        sync.on_disconnect();
        self.schedule_retry();
    }

    /// Drains network events into `sync`. Called once per frame.
    pub fn poll(&mut self, sync: &mut SyncState) {
        if self.fatal.is_some() {
            return;
        }
        if self.connection.is_none() {
            let now = self.ctx.input(|i| i.time);
            if now >= self.retry_at {
                self.connect();
            } else {
                self.ctx
                    .request_repaint_after_secs((self.retry_at - now) as f32);
            }
        }

        while let Some(event) = self.connection.as_ref().and_then(|c| c.receiver.try_recv()) {
            match event {
                WsEvent::Opened => {
                    let join = ClientMsg::Join {
                        room: self.room.clone(),
                        name: self.name.clone(),
                    };
                    if let Some(connection) = &mut self.connection {
                        Self::write(connection, &join);
                    }
                }
                WsEvent::Message(WsMessage::Text(text)) => {
                    match serde_json::from_str::<ServerMsg>(&text) {
                        Ok(msg) => self.on_message(msg, sync),
                        Err(error) => eprintln!("ignoring invalid server message: {error}"),
                    }
                }
                WsEvent::Message(_) => {}
                WsEvent::Error(error) => {
                    eprintln!("connection error: {error}");
                    self.lost(sync);
                }
                WsEvent::Closed => self.lost(sync),
            }
        }
    }

    fn on_message(&mut self, msg: ServerMsg, sync: &mut SyncState) {
        match msg {
            ServerMsg::Error {
                code:
                    code @ (ErrorCode::NameMismatch | ErrorCode::ServerFull | ErrorCode::InvalidRoom),
            } => {
                self.fatal = Some(code);
                self.connection = None;
                self.joined = false;
                sync.on_disconnect();
            }
            ServerMsg::Error { code } => eprintln!("server error: {code:?}"),
            msg => {
                if matches!(msg, ServerMsg::Snapshot { .. }) {
                    self.joined = true;
                }
                sync.apply(msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tracker_core::ItemValue;
    use tracker_server::{app, hub::Hub};

    struct Peer {
        ctx: Context,
        net: Net,
        sync: SyncState,
        time: f64,
    }

    impl Peer {
        fn new(url: &str) -> Self {
            let ctx = Context::default();
            let net = Net::new(ctx.clone(), url.into(), "room".into(), "compact".into());
            Self {
                ctx,
                net,
                sync: SyncState::default(),
                time: 0.0,
            }
        }

        /// One UI frame, advancing the clock that drives reconnection.
        fn poll(&mut self) {
            self.time += 0.1;
            let input = egui::RawInput {
                time: Some(self.time),
                ..Default::default()
            };
            let mut output = self.ctx.run_ui(input, |_| {});
            output.textures_delta.clear(); // No GPU in tests to hand them to.
            self.net.poll(&mut self.sync);
        }

        fn poll_until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done(self) {
                assert!(Instant::now() < deadline, "timed out waiting for: {what}");
                self.poll();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn start_server(listener: std::net::TcpListener) -> tokio::runtime::Runtime {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        listener.set_nonblocking(true).unwrap();
        runtime.spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(listener, app(Arc::new(Hub::default()))).await
        });
        runtime
    }

    #[test]
    fn peers_sync_and_recover_from_a_server_restart() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("ws://{addr}/ws");
        let runtime = start_server(listener);

        let mut a = Peer::new(&url);
        let mut b = Peer::new(&url);
        a.poll_until("a joined", |p| p.net.is_joined());
        b.poll_until("b joined", |p| p.net.is_joined());

        let value = ItemValue::Number(2);
        assert!(a.net.send(&ClientMsg::Set {
            item: 9,
            value: value.clone()
        }));
        a.sync.set_local(9, value.clone(), true);
        b.poll_until("b sees a's change", |p| p.sync.get(9).is_some());
        assert_eq!(b.sync.get(9), Some(&value));

        runtime.shutdown_background();
        a.poll_until("a notices the outage", |p| !p.net.is_joined());
        assert!(
            !a.net.send(&ClientMsg::Reset),
            "nothing is sent while offline"
        );

        let _runtime = start_server(std::net::TcpListener::bind(addr).unwrap());
        a.poll_until("a rejoins", |p| p.net.is_joined());
        // The restarted server is empty, and its state wins.
        assert_eq!(a.sync.get(9), None);
    }

    #[test]
    fn joining_with_the_wrong_tracker_stops_retrying() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        let _runtime = start_server(listener);

        let mut a = Peer::new(&url);
        a.poll_until("a joined", |p| p.net.is_joined());
        let mut other = Peer::new(&url);
        other.net.name = "compactfr".into();
        other.poll_until("refusal", |p| p.net.fatal.is_some());
        assert_eq!(other.net.fatal, Some(ErrorCode::NameMismatch));
        assert!(!other.net.is_joined());
    }
}
