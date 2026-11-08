//! Client-side replica with optimistic updates.
//!
//! A local change shows immediately. Until the server has echoed every change
//! this client made to an item, messages about that item are held back: the
//! last echo carries the value the server finally settled on.

use crate::protocol::{ItemValue, ServerMsg};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Default)]
pub struct SyncState {
    conn: Option<u64>,
    rev: u64,
    values: BTreeMap<u32, ItemValue>,
    pending: HashMap<u32, u32>,
}

impl SyncState {
    pub fn get(&self, item: u32) -> Option<&ItemValue> {
        self.values.get(&item)
    }

    /// Applies a change made by this client. `sent` is false while offline, in
    /// which case no echo will ever come.
    pub fn set_local(&mut self, item: u32, value: ItemValue, sent: bool) {
        self.values.insert(item, value);
        if sent {
            *self.pending.entry(item).or_default() += 1;
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.pending.clear();
    }

    pub fn on_disconnect(&mut self) {
        self.conn = None;
        self.pending.clear();
    }

    pub fn apply(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Snapshot { conn, state, rev } => {
                self.conn = Some(conn);
                self.rev = rev;
                self.values = state.into_iter().collect();
                self.pending.clear();
            }
            ServerMsg::Update {
                item,
                value,
                rev,
                origin,
            } => {
                if rev <= self.rev {
                    return;
                }
                self.rev = rev;
                if self.conn == Some(origin)
                    && let Some(count) = self.pending.get_mut(&item)
                {
                    *count -= 1;
                    if *count == 0 {
                        self.pending.remove(&item);
                    }
                }
                if !self.pending.contains_key(&item) {
                    self.values.insert(item, value);
                }
            }
            ServerMsg::Reset { rev } => {
                if rev > self.rev {
                    self.rev = rev;
                    self.clear();
                }
            }
            ServerMsg::Error { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: u64 = 1;
    const OTHER: u64 = 2;

    fn online() -> SyncState {
        let mut s = SyncState::default();
        s.apply(ServerMsg::Snapshot {
            conn: ME,
            state: vec![],
            rev: 0,
        });
        s
    }

    fn update(item: u32, n: i64, rev: u64, origin: u64) -> ServerMsg {
        ServerMsg::Update {
            item,
            value: ItemValue::Number(n),
            rev,
            origin,
        }
    }

    #[test]
    fn stale_echo_does_not_roll_back_fast_clicks() {
        let mut s = online();
        s.set_local(5, ItemValue::Number(1), true);
        s.set_local(5, ItemValue::Number(2), true);
        s.apply(update(5, 1, 1, ME));
        assert_eq!(s.get(5), Some(&ItemValue::Number(2)));
        s.apply(update(5, 2, 2, ME));
        assert_eq!(s.get(5), Some(&ItemValue::Number(2)));
    }

    #[test]
    fn changes_to_different_items_are_all_kept() {
        let mut s = online();
        s.set_local(1, ItemValue::Number(1), true);
        s.apply(update(2, 9, 1, OTHER));
        s.apply(update(1, 1, 2, ME));
        assert_eq!(s.get(1), Some(&ItemValue::Number(1)));
        assert_eq!(s.get(2), Some(&ItemValue::Number(9)));
    }

    #[test]
    fn concurrent_writes_to_one_item_converge_on_server_order() {
        // Server order: my write, then the other client's.
        let mut s = online();
        s.set_local(1, ItemValue::Number(1), true);
        s.apply(update(1, 1, 1, ME));
        s.apply(update(1, 7, 2, OTHER));
        assert_eq!(s.get(1), Some(&ItemValue::Number(7)));

        // Server order: the other client's write, then mine.
        let mut s = online();
        s.set_local(1, ItemValue::Number(1), true);
        s.apply(update(1, 7, 1, OTHER));
        assert_eq!(s.get(1), Some(&ItemValue::Number(1)));
        s.apply(update(1, 1, 2, ME));
        assert_eq!(s.get(1), Some(&ItemValue::Number(1)));
    }

    #[test]
    fn snapshot_replaces_everything_after_a_reconnect() {
        let mut s = online();
        s.set_local(1, ItemValue::Number(1), true);
        s.on_disconnect();
        s.apply(ServerMsg::Snapshot {
            conn: 3,
            state: vec![(2, ItemValue::Flag(true))],
            rev: 9,
        });
        assert_eq!(s.get(1), None);
        assert_eq!(s.get(2), Some(&ItemValue::Flag(true)));
        s.apply(update(1, 4, 10, OTHER));
        assert_eq!(s.get(1), Some(&ItemValue::Number(4)));
    }

    #[test]
    fn already_seen_revisions_are_ignored() {
        let mut s = online();
        s.apply(update(1, 4, 5, OTHER));
        s.apply(update(1, 2, 4, OTHER));
        assert_eq!(s.get(1), Some(&ItemValue::Number(4)));
    }

    #[test]
    fn reset_clears_state_and_pending_changes() {
        let mut s = online();
        s.set_local(1, ItemValue::Number(1), true);
        s.apply(ServerMsg::Reset { rev: 1 });
        assert_eq!(s.get(1), None);
        s.apply(update(1, 3, 2, OTHER));
        assert_eq!(s.get(1), Some(&ItemValue::Number(3)));
    }
}
