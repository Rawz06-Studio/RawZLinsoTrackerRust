//! In-memory rooms. Each change is applied and queued for broadcast under one
//! lock, so every client observes the same order.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use tracker_core::{ErrorCode, ItemValue, ServerMsg};

const MAX_ROOMS: usize = 1000;
const MAX_ITEMS_PER_ROOM: usize = 4096;
const MAX_ID_LEN: usize = 64;
const BROADCAST_BACKLOG: usize = 256;

pub type Broadcast = Arc<str>;

struct Room {
    name: String,
    state: BTreeMap<u32, ItemValue>,
    rev: u64,
    tx: broadcast::Sender<Broadcast>,
    created_at: SystemTime,
    last_activity: SystemTime,
}

impl Room {
    fn snapshot(&self, conn: u64) -> String {
        let msg = ServerMsg::Snapshot {
            conn,
            state: self.state.iter().map(|(k, v)| (*k, v.clone())).collect(),
            rev: self.rev,
        };
        encode(&msg)
    }

    fn publish(&self, msg: &ServerMsg) {
        // Err only means nobody is listening right now.
        let _ = self.tx.send(encode(msg).into());
    }
}

pub fn encode(msg: &ServerMsg) -> String {
    serde_json::to_string(msg).expect("server messages always serialize")
}

#[derive(Default)]
pub struct Hub {
    rooms: Mutex<HashMap<String, Room>>,
    next_conn: AtomicU64,
}

pub struct RoomSummary {
    pub tracker: String,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub connected_users: usize,
}

pub struct Joined {
    pub room: String,
    pub snapshot: String,
    pub rx: broadcast::Receiver<Broadcast>,
}

impl Hub {
    pub fn next_conn(&self) -> u64 {
        self.next_conn.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn join(&self, room: &str, name: &str, conn: u64) -> Result<Joined, ErrorCode> {
        let valid = |s: &str| !s.is_empty() && s.len() <= MAX_ID_LEN;
        if !valid(room) || !valid(name) {
            return Err(ErrorCode::InvalidRoom);
        }
        let room = room.to_lowercase();
        let mut rooms = self.rooms.lock().unwrap();
        if !rooms.contains_key(&room) && rooms.len() >= MAX_ROOMS {
            return Err(ErrorCode::ServerFull);
        }
        let entry = rooms.entry(room.clone()).or_insert_with(|| Room {
            name: name.to_owned(),
            state: BTreeMap::new(),
            rev: 0,
            tx: broadcast::channel(BROADCAST_BACKLOG).0,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
        });
        if entry.name != name {
            return Err(ErrorCode::NameMismatch);
        }
        entry.last_activity = SystemTime::now();
        Ok(Joined {
            snapshot: entry.snapshot(conn),
            rx: entry.tx.subscribe(),
            room,
        })
    }

    pub fn set(&self, room: &str, conn: u64, item: u32, value: ItemValue) -> Result<(), ErrorCode> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.get_mut(room).ok_or(ErrorCode::NotJoined)?;
        if !room.state.contains_key(&item) && room.state.len() >= MAX_ITEMS_PER_ROOM {
            return Err(ErrorCode::RoomFull);
        }
        room.state.insert(item, value.clone());
        room.last_activity = SystemTime::now();
        room.rev += 1;
        room.publish(&ServerMsg::Update {
            item,
            value,
            rev: room.rev,
            origin: conn,
        });
        Ok(())
    }

    pub fn reset(&self, room: &str) -> Result<(), ErrorCode> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.get_mut(room).ok_or(ErrorCode::NotJoined)?;
        room.state.clear();
        room.rev += 1;
        room.publish(&ServerMsg::Reset { rev: room.rev });
        Ok(())
    }

    /// Full state for a client that fell too far behind the broadcast.
    pub fn snapshot(&self, room: &str, conn: u64) -> Option<String> {
        self.rooms
            .lock()
            .unwrap()
            .get(room)
            .map(|r| r.snapshot(conn))
    }

    pub fn touch(&self, room: &str) {
        if let Some(room) = self.rooms.lock().unwrap().get_mut(room) {
            room.last_activity = SystemTime::now();
        }
    }

    pub fn list(&self) -> BTreeMap<String, RoomSummary> {
        let rooms = self.rooms.lock().unwrap();
        rooms
            .iter()
            .map(|(id, room)| {
                let summary = RoomSummary {
                    tracker: room.name.clone(),
                    created_at: room.created_at,
                    last_activity: room.last_activity,
                    connected_users: room.tx.receiver_count(),
                };
                (id.clone(), summary)
            })
            .collect()
    }

    /// See [`Hub::remove_older_than`] for what happens to connected clients.
    pub fn delete(&self, room: &str) -> bool {
        self.rooms.lock().unwrap().remove(room).is_some()
    }

    /// Dropping a room closes its channel, which disconnects its clients.
    pub fn remove_older_than(&self, age: Duration) {
        self.rooms
            .lock()
            .unwrap()
            .retain(|_, room| room.created_at.elapsed().unwrap_or_default() < age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_ids_are_case_insensitive() {
        let hub = Hub::default();
        let joined = hub.join("MyRoom", "compact", 1).unwrap();
        assert_eq!(joined.room, "myroom");
        assert!(hub.join("MYROOM", "compact", 2).is_ok());
    }

    #[test]
    fn joining_with_another_tracker_is_refused() {
        let hub = Hub::default();
        hub.join("a", "compact", 1).unwrap();
        assert_eq!(
            hub.join("a", "compactfr", 2).err(),
            Some(ErrorCode::NameMismatch)
        );
    }

    #[test]
    fn limits_are_enforced() {
        let hub = Hub::default();
        assert_eq!(hub.join("", "x", 1).err(), Some(ErrorCode::InvalidRoom));
        for i in 0..MAX_ROOMS {
            hub.join(&format!("r{i}"), "x", 1).unwrap();
        }
        assert_eq!(hub.join("extra", "x", 1).err(), Some(ErrorCode::ServerFull));
        assert!(
            hub.join("r0", "x", 2).is_ok(),
            "existing rooms stay joinable"
        );

        for i in 0..MAX_ITEMS_PER_ROOM as u32 {
            hub.set("r0", 1, i, ItemValue::Flag(true)).unwrap();
        }
        let extra = hub.set("r0", 1, u32::MAX, ItemValue::Flag(true));
        assert_eq!(extra, Err(ErrorCode::RoomFull));
    }

    #[test]
    fn expired_rooms_are_removed_and_their_clients_notified() {
        let hub = Hub::default();
        let mut joined = hub.join("a", "x", 1).unwrap();
        hub.remove_older_than(Duration::ZERO);
        assert_eq!(hub.snapshot("a", 1), None);
        assert!(matches!(
            joined.rx.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
    }
}
