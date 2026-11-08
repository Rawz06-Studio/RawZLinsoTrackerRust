//! Drives the real server over real WebSockets.

use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tracker_core::{ClientMsg, ErrorCode, ItemValue, ServerMsg, SyncState};
use tracker_server::{app, hub::Hub};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app(Arc::new(Hub::default()))).await });
    format!("ws://{addr}/ws")
}

async fn send(ws: &mut Ws, msg: ClientMsg) {
    ws.send(Message::text(serde_json::to_string(&msg).unwrap()))
        .await
        .unwrap();
}

async fn recv(ws: &mut Ws) -> ServerMsg {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for the server")
            .unwrap()
            .unwrap();
        if let Message::Text(text) = frame {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn join(url: &str, room: &str, name: &str) -> (Ws, ServerMsg) {
    let (mut ws, _) = connect_async(url).await.unwrap();
    send(
        &mut ws,
        ClientMsg::Join {
            room: room.into(),
            name: name.into(),
        },
    )
    .await;
    let first = recv(&mut ws).await;
    (ws, first)
}

fn set(item: u32, n: i64) -> ClientMsg {
    ClientMsg::Set {
        item,
        value: ItemValue::Number(n),
    }
}

#[tokio::test]
async fn clicks_from_two_clients_on_different_items_are_both_kept() {
    let url = server().await;
    let (mut a, snap_a) = join(&url, "room", "compact").await;
    let (mut b, snap_b) = join(&url, "room", "compact").await;

    let mut state_a = SyncState::default();
    let mut state_b = SyncState::default();
    state_a.apply(snap_a);
    state_b.apply(snap_b);

    // Each side changes its item before seeing the other's change: the case
    // where whole-state sync loses a click.
    state_a.set_local(1, ItemValue::Number(1), true);
    state_b.set_local(2, ItemValue::Number(2), true);
    send(&mut a, set(1, 1)).await;
    send(&mut b, set(2, 2)).await;

    for _ in 0..2 {
        state_a.apply(recv(&mut a).await);
        state_b.apply(recv(&mut b).await);
    }
    for state in [&state_a, &state_b] {
        assert_eq!(state.get(1), Some(&ItemValue::Number(1)));
        assert_eq!(state.get(2), Some(&ItemValue::Number(2)));
    }
}

#[tokio::test]
async fn a_late_joiner_receives_the_current_state() {
    let url = server().await;
    let (mut a, _) = join(&url, "room", "compact").await;
    send(&mut a, set(1, 5)).await;
    recv(&mut a).await;

    let (_b, snapshot) = join(&url, "ROOM", "compact").await;
    let ServerMsg::Snapshot { state, rev, .. } = snapshot else {
        panic!("{snapshot:?}")
    };
    assert_eq!(state, vec![(1, ItemValue::Number(5))]);
    assert_eq!(rev, 1);
}

#[tokio::test]
async fn a_client_with_the_wrong_tracker_is_kept_out_of_the_room() {
    let url = server().await;
    let (mut a, _) = join(&url, "room", "compact").await;
    let (mut intruder, reply) = join(&url, "room", "compactfr").await;
    assert_eq!(
        reply,
        ServerMsg::Error {
            code: ErrorCode::NameMismatch
        }
    );

    send(&mut intruder, set(1, 9)).await;
    assert_eq!(
        recv(&mut intruder).await,
        ServerMsg::Error {
            code: ErrorCode::NotJoined
        }
    );

    // The room is untouched: the next message `a` sees is its own.
    send(&mut a, set(2, 3)).await;
    let ServerMsg::Update { item, .. } = recv(&mut a).await else {
        panic!()
    };
    assert_eq!(item, 2);
}

#[tokio::test]
async fn reset_clears_the_room_for_everyone() {
    let url = server().await;
    let (mut a, _) = join(&url, "room", "compact").await;
    let (mut b, _) = join(&url, "room", "compact").await;
    send(&mut a, set(1, 5)).await;
    recv(&mut a).await;
    recv(&mut b).await;

    send(&mut b, ClientMsg::Reset).await;
    assert_eq!(recv(&mut a).await, ServerMsg::Reset { rev: 2 });
    assert_eq!(recv(&mut b).await, ServerMsg::Reset { rev: 2 });
    let (_c, snapshot) = join(&url, "room", "compact").await;
    let ServerMsg::Snapshot { state, .. } = snapshot else {
        panic!()
    };
    assert!(state.is_empty());
}

#[tokio::test]
async fn invalid_json_gets_an_error_and_keeps_the_connection() {
    let url = server().await;
    let (mut ws, _) = connect_async(&url).await.unwrap();
    ws.send(Message::text("not json")).await.unwrap();
    assert_eq!(
        recv(&mut ws).await,
        ServerMsg::Error {
            code: ErrorCode::BadMessage
        }
    );
    send(
        &mut ws,
        ClientMsg::Join {
            room: "r".into(),
            name: "n".into(),
        },
    )
    .await;
    assert!(matches!(recv(&mut ws).await, ServerMsg::Snapshot { .. }));
}

mod http {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;
    use tracker_server::site;

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn router(hub: Arc<Hub>) -> (axum::Router, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("tracker-site-{:p}", Arc::as_ptr(&hub)));
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::create_dir_all(root.join("public/sources/compact")).unwrap();
        std::fs::write(root.join("web/index.html"), "the client").unwrap();
        std::fs::write(root.join("public/sources/compact/tracker.json"), "[]").unwrap();
        (site(hub, &root.join("public"), &root.join("web")), root)
    }

    async fn body(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn deep_links_serve_the_client_but_missing_tracker_files_are_404() {
        let (router, root) = router(Arc::new(Hub::default()));
        for uri in ["/", "/compact", "/tracker/compact/room"] {
            let response = router.clone().oneshot(get(uri)).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(body(response).await, "the client", "{uri}");
        }
        let found = get("/sources/compact/tracker.json");
        let response = router.clone().oneshot(found).await.unwrap();
        assert_eq!(body(response).await, "[]");
        let missing = get("/sources/nope/tracker.json");
        let response = router.clone().oneshot(missing).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let traversal = get("/sources/../web/index.html");
        let response = router.oneshot(traversal).await.unwrap();
        assert_ne!(body(response).await, "the client");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn api_lists_and_deletes_rooms() {
        let hub = Arc::new(Hub::default());
        let joined = hub.join("Room", "compact", 1).unwrap();
        let (router, root) = router(hub);

        let response = router.clone().oneshot(get("/api/tracker")).await.unwrap();
        let rooms: serde_json::Value = serde_json::from_str(&body(response).await).unwrap();
        let room = &rooms["room"];
        assert_eq!(room["tracker"], "compact");
        assert_eq!(room["url"], "/tracker/compact/room");
        assert_eq!(room["connectedUsers"], 1);
        assert_eq!(room["isActive"], true);
        assert_eq!(room["createdDelay"], "00:00:00");
        assert!(room["createdAt"].as_str().unwrap().ends_with('Z'));
        drop(joined);

        let delete = |id: &str| {
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/tracker/{id}"))
                .body(Body::empty())
                .unwrap()
        };
        let response = router.clone().oneshot(delete("room")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router.oneshot(delete("room")).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(root).unwrap();
    }
}
