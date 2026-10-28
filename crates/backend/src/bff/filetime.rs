//! Checked conversion of Windows FILETIME ticks into UTC.
use chrono::{DateTime, TimeZone, Utc};

const TICKS_PER_SECOND: u64 = 10_000_000;
const UNIX_EPOCH_OFFSET_SECS: i64 = 11_644_473_600;

pub(super) fn ticks_to_utc(ticks: u64) -> Option<DateTime<Utc>> {
    if ticks == 0 {
        return None;
    }
    let seconds = i64::try_from(ticks / TICKS_PER_SECOND)
        .ok()?
        .checked_sub(UNIX_EPOCH_OFFSET_SECS)?;
    let nanos = u32::try_from((ticks % TICKS_PER_SECOND) * 100).ok()?;
    Utc.timestamp_opt(seconds, nanos).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_converts_epoch_fraction_and_pre_epoch_without_overflow() {
        const EPOCH: u64 = 116_444_736_000_000_000;
        assert_eq!(ticks_to_utc(0), None);
        let at_epoch = ticks_to_utc(EPOCH).unwrap();
        assert_eq!(at_epoch.timestamp(), 0);
        assert_eq!(at_epoch.timestamp_subsec_nanos(), 0);
        let fraction = ticks_to_utc(EPOCH + 1_234_567).unwrap();
        assert_eq!(fraction.timestamp(), 0);
        assert_eq!(fraction.timestamp_subsec_nanos(), 123_456_700);
        let previous_tick = ticks_to_utc(EPOCH - 1).unwrap();
        assert_eq!(previous_tick.timestamp(), -1);
        assert_eq!(previous_tick.timestamp_subsec_nanos(), 999_999_900);
        let extreme = ticks_to_utc(u64::MAX);
        assert!(extreme.is_some());
        assert_eq!(extreme.unwrap().timestamp(), 1_833_029_933_770);
    }
}
