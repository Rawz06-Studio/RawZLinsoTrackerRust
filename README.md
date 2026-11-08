# RawZLinsoTracker

Randomizer tracker based on LinSoTracker, as a desktop app and a web app, with
shared rooms between players. Written in Rust with [egui](https://github.com/emilk/egui).

| Crate | Role |
| --- | --- |
| `crates/core` | Tracker files, item rules, sync protocol |
| `crates/server` | WebSocket rooms, monitoring API, hosting of the web client |
| `crates/client` | The tracker UI, for desktop and WebAssembly |

Trackers live in `public/sources`. The five of the menu are bundled in the
desktop executable.

## Commands

```bash
make server                            # http://127.0.0.1:3000
make client                            # desktop client, opens the menu
make client ARGS="compact --room test" # open a tracker and join a room
make web                               # build the web client and serve it
make test
make lint
make docker && make docker-run
```

`make web` needs the `wasm32-unknown-unknown` target and
[trunk](https://trunkrs.dev), which it installs if they are missing.

## Controls

| Input | Action |
| --- | --- |
| Left / right click, wheel, middle click | Change an item, depending on its type |
| Ctrl+Shift+R | Reset the tracker |
| Ctrl+Shift+L | Go Live: open a shared room |
| Ctrl+L | Open in a small window (web) |

## Server configuration

| Variable | Default | Role |
| --- | --- | --- |
| `BIND` | `127.0.0.1:3000` | Listening address |
| `PUBLIC_DIR` | `public` | Folder with `sources` and `default` |
| `WEB_DIR` | `dist` | Built web client |

## Releases

Pushing a `v*` tag builds the Windows, Linux and macOS executables and attaches
them to a GitHub release. Set the `TRACKER_SERVER_URL` repository variable, for
example `wss://tracker.example.com/ws`, to choose the server they connect to by
default. Without it they use the local server.
