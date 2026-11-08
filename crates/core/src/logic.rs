//! Item behaviour, ported from `components/items/type/*.vue`.

use crate::model::{Item, Kind};
use crate::protocol::ItemValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Primary,
    Secondary,
    Middle,
    WheelUp,
    WheelDown,
}

/// `value` wrapped into `0..len`. State comes from the network, so nothing here
/// may assume it is in range.
fn wrap(value: i64, len: i64) -> i64 {
    if len <= 0 { 0 } else { value.rem_euclid(len) }
}

fn next(value: i64, len: i64) -> i64 {
    wrap(value.saturating_add(1), len)
}

fn prev(value: i64, len: i64) -> i64 {
    wrap(value.saturating_sub(1), len)
}

impl Item {
    /// `None` for kinds that hold no state.
    pub fn default_value(&self) -> Option<ItemValue> {
        Some(match self.kind {
            Kind::Item | Kind::GoModeItem => ItemValue::Flag(self.is_active),
            Kind::IncrementalItem | Kind::EvolutionItem => ItemValue::Number(self.is_active as i64),
            Kind::CountItem => ItemValue::Number(self.value_start.unwrap_or(0)),
            Kind::CheckItem => ItemValue::Check {
                check_active: false,
                active: self.is_active,
            },
            Kind::LabelItem => ItemValue::Label {
                state: self.label_default.unwrap_or(0),
                active: self.is_active,
            },
            Kind::AlternateEvolutionItem => ItemValue::Label {
                state: 0,
                active: self.is_active,
            },
            Kind::AlternateCountItem => ItemValue::Count {
                count: 0,
                mq: false,
            },
            Kind::Unsupported => return None,
        })
    }

    /// `value` when it has the shape this kind expects, the default otherwise.
    pub fn resolve(&self, value: Option<&ItemValue>) -> Option<ItemValue> {
        let default = self.default_value()?;
        match value {
            Some(v) if std::mem::discriminant(v) == std::mem::discriminant(&default) => {
                Some(v.clone())
            }
            _ => Some(default),
        }
    }

    /// Number of states of an incremental or evolution item.
    pub fn stages(&self) -> i64 {
        match self.kind {
            Kind::IncrementalItem => self.increments.len() as i64 + 2,
            Kind::EvolutionItem => self.next.len() as i64 + 2,
            Kind::AlternateEvolutionItem => self.next.len() as i64 + 1,
            _ => 0,
        }
    }

    pub fn count_max(&self) -> i64 {
        self.value_max.unwrap_or(100)
    }

    pub fn alternate_count_max(&self, mq: bool) -> i64 {
        match (mq, self.max_alternate) {
            (true, Some(alternate)) => alternate,
            _ => self.max.unwrap_or(1),
        }
    }

    /// New value after `action`, or `None` when the action does nothing.
    pub fn apply(&self, current: &ItemValue, action: Action) -> Option<ItemValue> {
        use Action::*;
        let new = match (self.kind, current, action) {
            (Kind::Item | Kind::GoModeItem, ItemValue::Flag(on), Primary | Secondary) => {
                ItemValue::Flag(!on)
            }
            (Kind::IncrementalItem | Kind::EvolutionItem, ItemValue::Number(v), Primary) => {
                ItemValue::Number(next(*v, self.stages()))
            }
            (Kind::IncrementalItem | Kind::EvolutionItem, ItemValue::Number(v), Secondary) => {
                ItemValue::Number(prev(*v, self.stages()))
            }
            (Kind::CountItem, ItemValue::Number(v), Primary | WheelDown) => {
                ItemValue::Number(wrap(
                    v.saturating_add(self.value_step.unwrap_or(1)),
                    self.count_max() + 1,
                ))
            }
            (Kind::CountItem, ItemValue::Number(v), Secondary | WheelUp) => {
                ItemValue::Number(prev(*v, self.count_max() + 1))
            }
            (
                Kind::CheckItem,
                ItemValue::Check {
                    check_active,
                    active,
                },
                Primary,
            ) => ItemValue::Check {
                check_active: *check_active,
                active: !active,
            },
            (
                Kind::CheckItem,
                ItemValue::Check {
                    check_active,
                    active,
                },
                Secondary,
            ) => ItemValue::Check {
                check_active: !check_active,
                active: *active,
            },
            (Kind::LabelItem, ItemValue::Label { state, active }, Primary) => ItemValue::Label {
                state: *state,
                active: !active,
            },
            (Kind::LabelItem, ItemValue::Label { state, active }, Secondary)
                if self.label_default.is_none() =>
            {
                ItemValue::Label {
                    state: next(*state, self.labels.len() as i64),
                    active: *active,
                }
            }
            (Kind::LabelItem, ItemValue::Label { state, active }, WheelDown) => {
                let len = self.labels.len() as i64;
                let state = match self.label_default {
                    // With a default label the list is a bounded scale.
                    Some(_) if state + 1 >= len => return None,
                    Some(_) => state + 1,
                    None => next(*state, len),
                };
                ItemValue::Label {
                    state,
                    active: *active,
                }
            }
            (Kind::LabelItem, ItemValue::Label { state, active }, WheelUp) => {
                let state = match self.label_default {
                    Some(_) if *state <= 0 => return None,
                    Some(_) => state - 1,
                    None => prev(*state, self.labels.len() as i64),
                };
                ItemValue::Label {
                    state,
                    active: *active,
                }
            }
            (Kind::AlternateEvolutionItem, ItemValue::Label { state, active }, Primary) => {
                ItemValue::Label {
                    state: *state,
                    active: !active,
                }
            }
            (Kind::AlternateEvolutionItem, ItemValue::Label { state, active }, Secondary) => {
                ItemValue::Label {
                    state: next(*state, self.stages()),
                    active: *active,
                }
            }
            (Kind::AlternateCountItem, ItemValue::Count { count, mq }, Primary | WheelDown) => {
                ItemValue::Count {
                    count: next(*count, self.alternate_count_max(*mq) + 1),
                    mq: *mq,
                }
            }
            (Kind::AlternateCountItem, ItemValue::Count { count, mq }, Secondary | WheelUp) => {
                ItemValue::Count {
                    count: prev(*count, self.alternate_count_max(*mq) + 1),
                    mq: *mq,
                }
            }
            (Kind::AlternateCountItem, ItemValue::Count { count, mq }, Middle)
                if self.max_alternate.is_some() =>
            {
                let mq = !mq;
                ItemValue::Count {
                    count: (*count).min(self.alternate_count_max(mq)),
                    mq,
                }
            }
            _ => return None,
        };
        Some(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Point, SheetRef};

    fn item(kind: Kind) -> Item {
        Item {
            id: 1,
            kind,
            name: "test".into(),
            position: Point::default(),
            sheet: SheetRef {
                row: 1,
                column: 1,
                sheet: "Normal".into(),
            },
            is_active: false,
            opacity_disabled: 0.5,
            label: None,
            label_size: None,
            label_offset: None,
            labels: vec![],
            label_default: None,
            global_label: None,
            increments: vec![],
            next: vec![],
            check_sheet: None,
            glow: None,
            value_max: None,
            value_step: None,
            value_start: None,
            max: None,
            max_alternate: None,
            label_position: None,
        }
    }

    #[test]
    fn simple_item_toggles_on_both_buttons() {
        let it = item(Kind::Item);
        let on = it.apply(&ItemValue::Flag(false), Action::Primary).unwrap();
        assert_eq!(on, ItemValue::Flag(true));
        assert_eq!(
            it.apply(&on, Action::Secondary),
            Some(ItemValue::Flag(false))
        );
        assert_eq!(it.apply(&on, Action::WheelUp), None);
    }

    #[test]
    fn incremental_cycles_through_all_stages() {
        let mut it = item(Kind::IncrementalItem);
        it.increments = vec!["30".into(), "40".into()];
        assert_eq!(it.stages(), 4);
        let mut v = ItemValue::Number(0);
        for expected in [1, 2, 3, 0] {
            v = it.apply(&v, Action::Primary).unwrap();
            assert_eq!(v, ItemValue::Number(expected));
        }
        assert_eq!(it.apply(&v, Action::Secondary), Some(ItemValue::Number(3)));
    }

    #[test]
    fn count_wraps_with_step_going_up_and_one_going_down() {
        let mut it = item(Kind::CountItem);
        it.value_max = Some(4);
        it.value_step = Some(2);
        assert_eq!(
            it.apply(&ItemValue::Number(4), Action::Primary),
            Some(ItemValue::Number(1))
        );
        assert_eq!(
            it.apply(&ItemValue::Number(0), Action::WheelUp),
            Some(ItemValue::Number(4))
        );
        assert_eq!(
            it.apply(&ItemValue::Number(3), Action::Secondary),
            Some(ItemValue::Number(2))
        );
    }

    #[test]
    fn check_item_uses_one_button_per_field() {
        let it = item(Kind::CheckItem);
        let v = ItemValue::Check {
            check_active: false,
            active: false,
        };
        assert_eq!(
            it.apply(&v, Action::Primary),
            Some(ItemValue::Check {
                check_active: false,
                active: true
            })
        );
        assert_eq!(
            it.apply(&v, Action::Secondary),
            Some(ItemValue::Check {
                check_active: true,
                active: false
            })
        );
    }

    #[test]
    fn label_with_default_is_bounded_and_ignores_right_click() {
        let mut it = item(Kind::LabelItem);
        it.labels = vec!["a".into(), "b".into()];
        it.label_default = Some(0);
        let start = ItemValue::Label {
            state: 0,
            active: false,
        };
        let end = ItemValue::Label {
            state: 1,
            active: false,
        };
        assert_eq!(it.apply(&start, Action::WheelUp), None);
        assert_eq!(it.apply(&start, Action::WheelDown), Some(end.clone()));
        assert_eq!(it.apply(&end, Action::WheelDown), None);
        assert_eq!(it.apply(&start, Action::Secondary), None);
    }

    #[test]
    fn label_without_default_wraps() {
        let mut it = item(Kind::LabelItem);
        it.labels = vec!["a".into(), "b".into()];
        let start = ItemValue::Label {
            state: 0,
            active: true,
        };
        let end = ItemValue::Label {
            state: 1,
            active: true,
        };
        assert_eq!(it.apply(&start, Action::WheelUp), Some(end.clone()));
        assert_eq!(it.apply(&end, Action::Secondary), Some(start));
    }

    #[test]
    fn switching_mq_clamps_count_to_the_new_max() {
        let mut it = item(Kind::AlternateCountItem);
        it.max = Some(5);
        it.max_alternate = Some(3);
        let v = ItemValue::Count {
            count: 5,
            mq: false,
        };
        assert_eq!(
            it.apply(&v, Action::Middle),
            Some(ItemValue::Count { count: 3, mq: true })
        );
    }

    #[test]
    fn middle_click_does_nothing_without_alternate_max() {
        let mut it = item(Kind::AlternateCountItem);
        it.max = Some(5);
        assert_eq!(
            it.apply(
                &ItemValue::Count {
                    count: 1,
                    mq: false
                },
                Action::Middle
            ),
            None
        );
    }

    #[test]
    fn out_of_range_network_values_do_not_panic() {
        let mut it = item(Kind::CountItem);
        it.value_max = Some(-1);
        assert_eq!(
            it.apply(&ItemValue::Number(i64::MAX), Action::Primary),
            Some(ItemValue::Number(0))
        );
        let labels = item(Kind::LabelItem);
        let v = ItemValue::Label {
            state: i64::MIN,
            active: false,
        };
        assert!(labels.apply(&v, Action::WheelUp).is_some());
    }

    #[test]
    fn resolve_replaces_values_of_the_wrong_shape() {
        let it = item(Kind::Item);
        assert_eq!(
            it.resolve(Some(&ItemValue::Number(3))),
            Some(ItemValue::Flag(false))
        );
        assert_eq!(
            it.resolve(Some(&ItemValue::Flag(true))),
            Some(ItemValue::Flag(true))
        );
    }
}
