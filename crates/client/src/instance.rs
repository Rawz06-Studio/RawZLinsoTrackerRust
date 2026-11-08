//! Starts another copy of the app, which gives one more tracker window.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Inside an AppImage the executable sits on a mount that disappears with the
/// first instance, so the AppImage itself has to be started.
fn program() -> std::io::Result<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(path) => Ok(path.into()),
        None => std::env::current_exe(),
    }
}

fn arguments(server_url: &str, sources_dir: Option<&Path>) -> Vec<OsString> {
    let mut arguments = vec!["--server".into(), server_url.into()];
    if let Some(dir) = sources_dir {
        arguments.push("--sources".into());
        arguments.push(dir.into());
    }
    arguments
}

/// Opens the tracker menu in a new process using the same server and trackers.
pub fn spawn(server_url: &str, sources_dir: Option<&Path>) {
    let child = program().and_then(|program| {
        Command::new(program)
            .args(arguments(server_url, sources_dir))
            .spawn()
    });
    match child {
        Ok(mut child) => {
            // Collects the exit status, so a closed window leaves no zombie.
            std::thread::spawn(move || child.wait());
        }
        Err(error) => eprintln!("could not open a new window: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_instance_keeps_the_server_and_the_trackers() {
        assert_eq!(arguments("ws://a/ws", None), ["--server", "ws://a/ws"]);
        assert_eq!(
            arguments("ws://a/ws", Some(Path::new("/trackers"))),
            ["--server", "ws://a/ws", "--sources", "/trackers"]
        );
    }
}
