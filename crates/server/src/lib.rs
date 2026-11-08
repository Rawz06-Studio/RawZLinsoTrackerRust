pub mod hub;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{delete, get};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use hub::{Broadcast, Hub, encode};
use humantime::format_rfc3339_millis;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast::{self, error::RecvError};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracker_core::{ClientMsg, ErrorCode, ServerMsg};

const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const PING_INTERVAL: Duration = Duration::from_secs(25);

/// WebSocket and monitoring API.
pub fn app(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/ws", get(upgrade))
        .route("/api", get(hello))
        .route("/api/tracker", get(list_rooms))
        .route("/api/tracker/{id}", delete(delete_room))
        .with_state(hub)
}

/// [`app`] plus the tracker files and the WASM client. Unknown paths get the
/// client's `index.html` so that links such as `/tracker/compact/room` work.
pub fn site(hub: Arc<Hub>, public: &Path, web: &Path) -> Router {
    app(hub)
        .nest_service("/sources", ServeDir::new(public.join("sources")))
        .nest_service("/default", ServeDir::new(public.join("default")))
        .fallback_service(ServeDir::new(web).fallback(ServeFile::new(web.join("index.html"))))
        .layer(CompressionLayer::new())
}

async fn hello() -> Json<serde_json::Value> {
    Json(json!({ "hello": "world" }))
}

/// `HH:MM:SS` since `time`.
fn delay(time: SystemTime) -> String {
    let seconds = time.elapsed().unwrap_or_default().as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

async fn list_rooms(State(hub): State<Arc<Hub>>) -> Json<serde_json::Value> {
    let rooms: serde_json::Map<_, _> = hub
        .list()
        .into_iter()
        .map(|(id, room)| {
            let value = json!({
                "tracker": room.tracker,
                "url": format!("/tracker/{}/{id}", room.tracker),
                "createdAt": format_rfc3339_millis(room.created_at).to_string(),
                "createdDelay": delay(room.created_at),
                "connectedUsers": room.connected_users,
                "lastActivityAt": format_rfc3339_millis(room.last_activity).to_string(),
                "lastActivityDelay": delay(room.last_activity),
                "isActive": room.connected_users > 0,
            });
            (id, value)
        })
        .collect();
    Json(rooms.into())
}

async fn delete_room(
    State(hub): State<Arc<Hub>>,
    UrlPath(id): UrlPath<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if hub.delete(&id) {
        let message = format!("Tracker with ID {id} has been deleted");
        (
            StatusCode::OK,
            Json(json!({ "status": "success", "message": message })),
        )
    } else {
        let message = format!("Tracker with ID {id} not found");
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "status": "error", "message": message })),
        )
    }
}

async fn upgrade(ws: WebSocketUpgrade, State(hub): State<Arc<Hub>>) -> Response {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(|socket| serve(socket, hub))
}

struct Session {
    room: String,
    rx: broadcast::Receiver<Broadcast>,
}

async fn next_broadcast(session: &mut Option<Session>) -> Result<Broadcast, RecvError> {
    match session {
        Some(session) => session.rx.recv().await,
        None => std::future::pending().await,
    }
}

fn error(code: ErrorCode) -> String {
    encode(&ServerMsg::Error { code })
}

/// Handles one client message and returns the direct reply, if any.
fn handle(text: &str, hub: &Hub, conn: u64, session: &mut Option<Session>) -> Option<String> {
    let Ok(msg) = serde_json::from_str::<ClientMsg>(text) else {
        return Some(error(ErrorCode::BadMessage));
    };
    let result = match (msg, session.as_ref()) {
        (ClientMsg::Join { room, name }, None) => match hub.join(&room, &name, conn) {
            Ok(joined) => {
                *session = Some(Session {
                    room: joined.room,
                    rx: joined.rx,
                });
                return Some(joined.snapshot);
            }
            Err(code) => Err(code),
        },
        (ClientMsg::Join { .. }, Some(_)) => Err(ErrorCode::BadMessage),
        (ClientMsg::Set { item, value }, Some(s)) => hub.set(&s.room, conn, item, value),
        (ClientMsg::Reset, Some(s)) => hub.reset(&s.room),
        (_, None) => Err(ErrorCode::NotJoined),
    };
    result.err().map(error)
}

async fn serve(socket: WebSocket, hub: Arc<Hub>) {
    let conn = hub.next_conn();
    let (mut sink, mut stream) = socket.split();
    let mut session = None;
    let mut ping = tokio::time::interval(PING_INTERVAL);

    loop {
        let outgoing = tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    handle(text.as_str(), &hub, conn, &mut session).map(|r| Message::Text(r.into()))
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => None,
            },
            event = next_broadcast(&mut session) => match event {
                Ok(text) => Some(Message::Text(text.as_ref().into())),
                Err(RecvError::Lagged(_)) => {
                    let room = &session.as_ref().expect("a broadcast implies a session").room;
                    hub.snapshot(room, conn).map(|s| Message::Text(s.into()))
                }
                Err(RecvError::Closed) => break,
            },
            _ = ping.tick() => Some(Message::Ping(Default::default())),
        };
        if let Some(message) = outgoing
            && sink.send(message).await.is_err()
        {
            break;
        }
    }

    if let Some(session) = session {
        hub.touch(&session.room);
    }
}
