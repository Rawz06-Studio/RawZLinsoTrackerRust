//! The pages of the app, mirrored in the URL on the web.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // URLs only exist on the web.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.');

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Home,
    Tracker { name: String, room: Option<String> },
}

/// Tracker names become file paths, so only plain folder names are valid.
fn is_tracker_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Route {
    /// `None` when the path matches no page.
    pub fn parse(path: &str) -> Option<Self> {
        let segments: Vec<String> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned())
            .collect();
        let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
        match segments[..] {
            [] => Some(Self::Home),
            [name] if is_tracker_name(name) => Some(Self::Tracker {
                name: name.to_owned(),
                room: None,
            }),
            ["tracker", name, room] if is_tracker_name(name) => Some(Self::Tracker {
                name: name.to_owned(),
                room: Some(room.to_owned()),
            }),
            _ => None,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Home => "/".to_owned(),
            Self::Tracker { name, room: None } => format!("/{name}"),
            Self::Tracker {
                name,
                room: Some(room),
            } => {
                format!("/tracker/{name}/{}", utf8_percent_encode(room, SEGMENT))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(name: &str, room: Option<&str>) -> Route {
        Route::Tracker {
            name: name.into(),
            room: room.map(Into::into),
        }
    }

    #[test]
    fn parses_the_pages_of_the_old_site() {
        assert_eq!(Route::parse("/"), Some(Route::Home));
        assert_eq!(Route::parse("/compact"), Some(tracker("compact", None)));
        assert_eq!(
            Route::parse("/tracker/compact/my-room"),
            Some(tracker("compact", Some("my-room")))
        );
    }

    #[test]
    fn rooms_with_special_characters_round_trip() {
        let route = tracker("compact", Some("salle à côté/2"));
        assert_eq!(Route::parse(&route.path()), Some(route));
    }

    #[test]
    fn unknown_paths_and_unsafe_names_are_rejected() {
        assert_eq!(Route::parse("/a/b"), None);
        assert_eq!(Route::parse("/.."), None);
        assert_eq!(Route::parse("/%2e%2e%2fsecret"), None);
    }
}
