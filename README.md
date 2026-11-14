# RawZLinsoTracker

Randomizer tracker based on LinSoTracker, as a desktop app and a web app, with
shared rooms between players. Written in Rust with [egui](https://github.com/emilk/egui).

| Crate | Role |
| --- | --- |
| `crates/core` | Tracker files, item rules, sync protocol |
| `crates/server` | WebSocket rooms, monitoring API, hosting of the web client |
| `crates/client` | The tracker UI, for desktop and WebAssembly |

Trackers live in `public/sources`, in two families read by different code:

- `restream/`: the restream trackers, which can go online in a shared room
  (`/compact`, `/tracker/compact/<room>`).
- `linso/`: LinSoTracker 2.4's own templates, extracted from their
  `.template` archives and played as LinSoTracker plays them: evolutions,
  hints, sub-menus, conditional items, drag and drop, and the map template
  with its logic and rules (`crates/core/src/linso`). They are local only
  (`/local/oot64`).

Both families are bundled in the desktop executable.

On first launch, the desktop app creates `Documents/RawZLinsoTracker/templates`.
LinSoTracker `.template` files or template folders dropped there appear in the
Local section, and replace a built-in template of the same name. The same
folder keeps the Escape menu's settings and the default saves (in the browser,
they go to local storage).

## Commands

```bash
make server                            # http://127.0.0.1:3000
make client                            # desktop client, opens the menu
make client ARGS="compact --room test" # open a tracker and join a room
make client ARGS="local/oot64"         # open a LinSoTracker template
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
| Ctrl+Shift+L | Go Live: open a shared room (restream trackers) |
| Escape | Options of a LinSoTracker template: zoom, hints, sound, saves |
| Hold the left button on an item | Drag it onto a draggable item or a map check |
| Ctrl+L | Open in a small window (web) |

## Server configuration

| Variable | Default | Role |
| --- | --- | --- |
| `BIND` | `127.0.0.1:3000` | Listening address |
| `PUBLIC_DIR` | `public` | Folder with `sources` and `default` |
| `WEB_DIR` | `dist` | Built web client |

## Releases

Every push to `main` runs the checks, then bumps the minor version, tags it and
creates a GitHub release. Once tagged, the client is attached to that release
as a Windows `.exe`, a Linux `.AppImage` and a macOS `.dmg`, and the server is
deployed to preprod. Nothing happens if the checks fail.

Set the `TRACKER_SERVER_URL` repository variable, for example
`wss://tracker.example.com/ws`, to choose the server the executables connect to
by default. Without it they use the local server.
