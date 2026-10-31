//! Conservative selection among samples with explicitly attested clock domains.
use crate::bff::model::{DataValue, QualityKind, SourceClockId, Value, ValueTimes};
use std::time::{Duration, Instant};

/// Product scheduling policy, not a claim that physical clock resets can be detected.
const MAX_OLDER_HOLD: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeOrder {
    #[default]
    Unknown,
    Comparable,
    OlderHeld,
    EqualConflict,
    Discontinuity,
    Diagnostic,
}

#[derive(Clone, Debug, Default)]
pub struct TimeSelection {
    pub order: TimeOrder,
    older_since: Option<Instant>,
    disabled_clock: Option<SourceClockId>,
}

impl TimeSelection {
    /// Decide whether to apply this sample. Only a trusted same source clock
    /// permits an older Good sample to be held. The bounded hold prevents an
    /// unknown clock discontinuity from freezing the row forever.
    pub fn select(
        &mut self,
        previous_times: &ValueTimes,
        previous_value: &Value,
        previous_good: bool,
        incoming: &DataValue,
        now: Instant,
    ) -> bool {
        let clock = incoming.times.source_clock;
        if clock.is_some() && self.disabled_clock.is_some() && clock != self.disabled_clock {
            self.disabled_clock = None;
        }
        if incoming.quality.kind != QualityKind::Good || incoming.reason.is_some() {
            self.order = TimeOrder::Diagnostic;
            self.older_since = None;
            return true;
        }
        let comparable = previous_good
            && clock.is_some()
            && clock == previous_times.source_clock
            && clock != self.disabled_clock;
        if comparable {
            if let (Some((old_kind, old)), Some((new_kind, new))) = (
                previous_times.comparable_measurement(),
                incoming.times.comparable_measurement(),
            ) && old_kind == new_kind
            {
                if new < old {
                    let first_older = *self.older_since.get_or_insert(now);
                    if now.saturating_duration_since(first_older) < MAX_OLDER_HOLD {
                        self.order = TimeOrder::OlderHeld;
                        return false;
                    }
                    self.disabled_clock = clock;
                    self.order = TimeOrder::Discontinuity;
                    self.older_since = None;
                    return true;
                }
                self.order = if new == old && incoming.value != *previous_value {
                    TimeOrder::EqualConflict
                } else {
                    TimeOrder::Comparable
                };
                self.older_since = None;
                return true;
            }
        }
        let equal_conflict = previous_good
            && incoming.value != *previous_value
            && matches!(
                (previous_times.comparable_measurement(), incoming.times.comparable_measurement()),
                (Some((old_kind, old)), Some((new_kind, new))) if old_kind == new_kind && old == new
            );
        self.order = if equal_conflict {
            TimeOrder::EqualConflict
        } else if clock.is_some() && clock == self.disabled_clock {
            TimeOrder::Discontinuity
        } else {
            TimeOrder::Unknown
        };
        self.older_since = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{BackendMsg, Quality, ValueOrigin};
    use chrono::{TimeZone, Utc};
    fn value(ts: i64, clock: Option<SourceClockId>, n: i64) -> DataValue {
        let mut times = ValueTimes::ua(
            Utc.timestamp_opt(ts, 0).single(),
            None,
            Utc.timestamp_opt(100, 0).single().unwrap(),
            ValueOrigin::Read,
        );
        times.source_clock = clock;
        DataValue {
            value: Value::I64(n),
            quality: Quality::from_ua(0, "Good"),
            timestamp: times.primary(),
            times,
            reason: None,
        }
    }
    #[test]
    fn known_clock_holds_older_only_until_bounded_ambiguity() {
        let now = Instant::now();
        let clock = Some(SourceClockId(7));
        let previous = value(11, clock, 11);
        let old = value(10, clock, 10);
        let mut selection = TimeSelection::default();
        assert!(selection.select(&ValueTimes::default(), &Value::Empty, false, &previous, now));
        assert!(!selection.select(
            &previous.times,
            &previous.value,
            true,
            &old,
            now + Duration::from_secs(4)
        ));
        assert_eq!(selection.order, TimeOrder::OlderHeld);
        assert!(selection.select(
            &previous.times,
            &previous.value,
            true,
            &old,
            now + Duration::from_secs(9)
        ));
        assert_eq!(selection.order, TimeOrder::Discontinuity);
        assert!(selection.select(
            &old.times,
            &old.value,
            true,
            &value(9, clock, 9),
            now + Duration::from_secs(10)
        ));
        assert_eq!(selection.order, TimeOrder::Discontinuity);
    }
    #[test]
    fn unknown_equal_conflict_and_bad_are_visible() {
        let now = Instant::now();
        let mut selection = TimeSelection::default();
        let prev = value(11, None, 1);
        let old = value(10, None, 2);
        assert!(selection.select(&prev.times, &prev.value, true, &old, now));
        assert_eq!(selection.order, TimeOrder::Unknown);
        let equal_unknown = value(11, None, 2);
        assert!(selection.select(&prev.times, &prev.value, true, &equal_unknown, now));
        assert_eq!(selection.order, TimeOrder::EqualConflict);
        let prev = value(11, Some(SourceClockId(7)), 1);
        let equal = value(11, Some(SourceClockId(7)), 2);
        assert!(selection.select(&prev.times, &prev.value, true, &equal, now));
        assert_eq!(selection.order, TimeOrder::EqualConflict);
        let mut bad = value(1, Some(SourceClockId(7)), 3);
        bad.reason = Some(BackendMsg::NoConnection);
        assert!(selection.select(&prev.times, &prev.value, true, &bad, now));
        assert_eq!(selection.order, TimeOrder::Diagnostic);
    }
    #[test]
    fn changing_attested_identity_allows_comparison_again() {
        let now = Instant::now();
        let mut state = TimeSelection::default();
        let old = value(20, Some(SourceClockId(1)), 1);
        let reset = value(1, Some(SourceClockId(1)), 2);
        state.select(&ValueTimes::default(), &Value::Empty, false, &old, now);
        assert!(!state.select(
            &old.times,
            &old.value,
            true,
            &reset,
            now + Duration::from_secs(6)
        ));
        assert!(state.select(
            &old.times,
            &old.value,
            true,
            &reset,
            now + Duration::from_secs(11)
        ));
        let replacement = value(100, Some(SourceClockId(2)), 3);
        assert!(state.select(
            &reset.times,
            &reset.value,
            true,
            &replacement,
            now + Duration::from_secs(12)
        ));
        let slightly_old = value(99, Some(SourceClockId(2)), 4);
        assert!(!state.select(
            &replacement.times,
            &replacement.value,
            true,
            &slightly_old,
            now + Duration::from_secs(13)
        ));
    }
    #[test]
    fn first_old_sample_after_long_idle_is_held_and_accepted_sample_resets_streak() {
        let now = Instant::now();
        let clock = Some(SourceClockId(9));
        let previous = value(11, clock, 11);
        let older = value(10, clock, 10);
        let mut state = TimeSelection::default();
        assert!(state.select(&ValueTimes::default(), &Value::Empty, false, &previous, now));
        assert!(!state.select(
            &previous.times,
            &previous.value,
            true,
            &older,
            now + Duration::from_secs(100)
        ));
        assert!(state.select(
            &previous.times,
            &previous.value,
            true,
            &previous,
            now + Duration::from_secs(101)
        ));
        assert!(!state.select(
            &previous.times,
            &previous.value,
            true,
            &older,
            now + Duration::from_secs(110)
        ));
        assert!(state.select(
            &previous.times,
            &previous.value,
            true,
            &older,
            now + Duration::from_secs(115)
        ));
        assert_eq!(state.order, TimeOrder::Discontinuity);
    }
    #[test]
    fn da_time_is_comparable_only_with_same_kind_and_attested_clock() {
        let now = Instant::now();
        let clock = Some(SourceClockId(7));
        let mut previous = value(20, clock, 20);
        previous.times.da_timestamp = previous.times.ua_source.take();
        let mut incoming = value(19, clock, 19);
        incoming.times.da_timestamp = incoming.times.ua_source.take();
        let mut selection = TimeSelection::default();
        selection.select(&ValueTimes::default(), &Value::Empty, false, &previous, now);
        assert!(!selection.select(&previous.times, &previous.value, true, &incoming, now));
        let ua = value(18, clock, 18);
        assert!(selection.select(&previous.times, &previous.value, true, &ua, now));
        assert_eq!(selection.order, TimeOrder::Unknown);
        incoming.times.source_clock = None;
        assert!(selection.select(&previous.times, &previous.value, true, &incoming, now));
        assert_eq!(selection.order, TimeOrder::Unknown);
    }
}
