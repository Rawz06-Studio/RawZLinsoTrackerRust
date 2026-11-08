//! JSON messages exchanged over the WebSocket.
//!
//! Unknown items are simply absent from the state: every client derives
//! defaults from its own `tracker.json`, so only changed items travel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemValue {
    Flag(bool),
    Number(i64),
    Check {
        #[serde(rename = "checkActive")]
        check_active: bool,
        active: bool,
    },
    Label {
        state: i64,
        active: bool,
    },
    Count {
        count: i64,
        mq: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Join { room: String, name: String },
    Set { item: u32, value: ItemValue },
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The room already exists for another tracker.
    NameMismatch,
    /// The server reached its room limit.
    ServerFull,
    /// The room reached its item limit.
    RoomFull,
    /// Room or tracker name is empty or too long.
    InvalidRoom,
    NotJoined,
    BadMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Full state, sent on join. `conn` identifies this connection in `origin`.
    Snapshot {
        conn: u64,
        // A list rather than a map: serde's internally tagged enums cannot
        // read integer map keys back from JSON strings.
        state: Vec<(u32, ItemValue)>,
        rev: u64,
    },
    Update {
        item: u32,
        value: ItemValue,
        rev: u64,
        origin: u64,
    },
    Reset {
        rev: u64,
    },
    Error {
        code: ErrorCode,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_keep_the_web_app_json_shape() {
        let cases = [
            (ItemValue::Flag(true), r#"true"#),
            (ItemValue::Number(3), r#"3"#),
            (
                ItemValue::Check {
                    check_active: true,
                    active: false,
                },
                r#"{"checkActive":true,"active":false}"#,
            ),
            (
                ItemValue::Label {
                    state: 2,
                    active: true,
                },
                r#"{"state":2,"active":true}"#,
            ),
            (
                ItemValue::Count { count: 4, mq: true },
                r#"{"count":4,"mq":true}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<ItemValue>(json).unwrap(), value);
        }
    }

    #[test]
    fn snapshot_round_trips() {
        let msg = ServerMsg::Snapshot {
            conn: 7,
            state: vec![(
                12,
                ItemValue::Count {
                    count: 1,
                    mq: false,
                },
            )],
            rev: 3,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(serde_json::from_str::<ServerMsg>(&json).unwrap(), msg);
    }
}
