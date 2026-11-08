use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracker_server::{hub::Hub, site};

const ROOM_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    let addr = env("BIND", "127.0.0.1:3000");
    let public = PathBuf::from(env("PUBLIC_DIR", "public"));
    let web = PathBuf::from(env("WEB_DIR", "dist"));
    let hub = Arc::new(Hub::default());

    let sweeper = hub.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            tick.tick().await;
            sweeper.remove_older_than(ROOM_LIFETIME);
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!(
        "tracker-server listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, site(hub, &public, &web)).await
}
