//! OLE Automation DATE conversion shared by DA adapters.
//! DATE has no timezone; OPCLook retains its existing UTC interpretation.
//! Before the epoch, the day is negative but the time-of-day fraction is
//! positive: -1.25 means 1899-12-29 06:00, not 1899-12-28 18:00.
//! Reference: dotnet/runtime DateTime.DoubleDateToTicks and Windows DATE.
use chrono::{DateTime, Datelike, Utc};

const EPOCH_UNIX_SECONDS: i64 = -2_209_161_600;
const SECONDS_PER_DAY: i64 = 86_400;
const MIN_DATE: f64 = -657_435.0;
const MAX_DATE: f64 = 2_958_466.0;

pub fn from_ole_date(serial: f64) -> Option<DateTime<Utc>> {
    if !serial.is_finite() || serial <= MIN_DATE || serial >= MAX_DATE {
        return None;
    }
    let day = serial.trunc() as i64;
    let time = serial.fract().abs() * SECONDS_PER_DAY as f64;
    let whole_seconds = time.trunc() as i64;
    let nanos = (time.fract() * 1_000_000_000.0).round() as i64;
    let seconds = EPOCH_UNIX_SECONDS
        .checked_add(day.checked_mul(SECONDS_PER_DAY)?)?
        .checked_add(whole_seconds)?;
    let value = DateTime::from_timestamp(seconds, 0)?
        .checked_add_signed(chrono::Duration::nanoseconds(nanos))?;
    (100..=9999).contains(&value.year()).then_some(value)
}

pub fn to_ole_date(value: &DateTime<Utc>) -> Option<f64> {
    if !(100..=9999).contains(&value.year()) || value.timestamp_subsec_nanos() >= 1_000_000_000 {
        return None;
    }
    let delta = value.timestamp().checked_sub(EPOCH_UNIX_SECONDS)?;
    let days = delta.div_euclid(SECONDS_PER_DAY);
    let time = delta.rem_euclid(SECONDS_PER_DAY) as f64
        + f64::from(value.timestamp_subsec_nanos()) / 1_000_000_000.0;
    let fraction = time / SECONDS_PER_DAY as f64;
    let serial = if days < 0 {
        days as f64 - fraction
    } else {
        days as f64 + fraction
    };
    (serial > MIN_DATE && serial < MAX_DATE).then_some(serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn negative_fraction_is_positive_time_of_day() {
        assert_eq!(from_ole_date(-1.25), Some(at(1899, 12, 29, 6)));
        assert_eq!(from_ole_date(-1.75), Some(at(1899, 12, 29, 18)));
        assert_eq!(from_ole_date(-0.25), Some(at(1899, 12, 30, 6)));
        assert_eq!(from_ole_date(-0.25), from_ole_date(0.25));
    }

    #[test]
    fn writes_canonical_negative_date_and_roundtrips() {
        for (serial, value) in [
            (-2.75, at(1899, 12, 28, 18)),
            (-1.25, at(1899, 12, 29, 6)),
            (0.25, at(1899, 12, 30, 6)),
            (1.75, at(1899, 12, 31, 18)),
        ] {
            assert_eq!(to_ole_date(&value), Some(serial));
            assert_eq!(from_ole_date(serial), Some(value));
        }
    }

    #[test]
    fn subsecond_precision_survives_floating_date_resolution() {
        let value = at(2026, 10, 30, 6)
            .checked_add_signed(chrono::Duration::nanoseconds(123_456_700))
            .unwrap();
        let decoded = from_ole_date(to_ole_date(&value).unwrap()).unwrap();
        let error = decoded
            .signed_duration_since(value)
            .num_nanoseconds()
            .unwrap();
        assert!(error.abs() <= 1_000, "error={error}ns");
    }

    #[test]
    fn invalid_values_and_automation_range_are_checked() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            -f64::MAX,
            MIN_DATE,
            MAX_DATE,
        ] {
            assert!(from_ole_date(value).is_none());
        }
        assert!(to_ole_date(&at(99, 12, 31, 0)).is_none());
        assert!(to_ole_date(&at(10000, 1, 1, 0)).is_none());
        assert!(from_ole_date(-657_434.5).is_some());
        assert!(from_ole_date(2_958_465.5).is_some());
    }
}
