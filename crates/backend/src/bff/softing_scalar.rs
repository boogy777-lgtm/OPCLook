//! Scalar-only mapping for the SDK ABI. Array and string ownership remains with
//! the SDK and is explicitly unsupported until its helpers are wired.
use super::model::{BackendMsg, DataValue, Quality, Value};
use super::softing_runtime::ReadScalar;
use chrono::{NaiveDate, TimeZone, Utc};

pub fn decode((raw, text): (ReadScalar, Option<String>)) -> Result<DataValue, BackendMsg> {
    if raw.service_result < 0 || raw.item_result < 0 {
        return Err(BackendMsg::raw(format!(
            "DA read HRESULT service={:#x}, item={:#x}",
            raw.service_result, raw.item_result
        )));
    }
    let bits = (u64::from(raw.bits_high) << 32) | u64::from(raw.bits_low);
    let signed = i64::from_ne_bytes(bits.to_ne_bytes());
    let value = match raw.vartype {
        0 => Value::Empty,
        2 | 3 | 16 | 20 | 22 => Value::I64(signed),
        17 | 18 | 19 | 21 | 23 => Value::U64(bits),
        4 | 5 => Value::F64(f64::from_bits(bits)),
        6 => Value::F64(signed as f64 / 10_000.0),
        7 => {
            let days = f64::from_bits(bits);
            if !days.is_finite() {
                return Err(BackendMsg::DaBadDate);
            }
            let base = NaiveDate::from_ymd_opt(1899, 12, 30)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .ok_or(BackendMsg::DaBadDate)?;
            let delta =
                chrono::Duration::try_days(days.trunc() as i64).ok_or(BackendMsg::DaBadDate)?;
            let date = base
                .checked_add_signed(delta)
                .and_then(|d| {
                    d.checked_add_signed(chrono::Duration::seconds(
                        (days.fract() * 86_400.0).round() as i64,
                    ))
                })
                .ok_or(BackendMsg::DaBadDate)?;
            Value::DateTime(Utc.from_utc_datetime(&date))
        }
        8 => Value::Str(text.ok_or_else(|| BackendMsg::raw("DA SDK string missing"))?),
        11 => Value::Bool(bits != 0),
        vt => return Err(BackendMsg::DaVtUnsupported { vt: vt.into() }),
    };
    let ticks = (u64::from(raw.filetime_high) << 32) | u64::from(raw.filetime_low);
    Ok(DataValue {
        value,
        quality: Quality::from_da(raw.quality),
        timestamp: super::filetime::ticks_to_utc(ticks),
        reason: None,
    })
}
fn signed(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(v) => Some(i64::from(*v)),
        Value::I64(v) => Some(*v),
        Value::U64(v) => i64::try_from(*v).ok(),
        Value::F64(v)
            if v.is_finite()
                && v.fract() == 0.0
                && *v >= i64::MIN as f64
                && *v < 9_223_372_036_854_775_808.0 =>
        {
            Some(*v as i64)
        }
        Value::Str(v) => v.trim().parse().ok(),
        Value::DateTime(v) => Some(v.timestamp()),
        _ => None,
    }
}
fn unsigned(value: &Value) -> Option<u64> {
    match value {
        Value::Bool(v) => Some(u64::from(*v)),
        Value::I64(v) => u64::try_from(*v).ok(),
        Value::U64(v) => Some(*v),
        Value::F64(v)
            if v.is_finite()
                && v.fract() == 0.0
                && *v >= 0.0
                && *v < 18_446_744_073_709_551_616.0 =>
        {
            Some(*v as u64)
        }
        Value::Str(v) => v.trim().parse().ok(),
        _ => None,
    }
}
pub fn encode(value: &Value, hint: Option<u16>) -> Result<(u16, u64), BackendMsg> {
    let vt = hint.unwrap_or_else(|| match value {
        Value::Empty => 0,
        Value::Bool(_) => 11,
        Value::I64(v) if i32::try_from(*v).is_ok() => 3,
        Value::I64(_) => 20,
        Value::U64(_) => 21,
        Value::F64(_) => 5,
        Value::DateTime(_) => 7,
        Value::Str(_) => 8,
        Value::Bytes(_) => 0x2011,
        Value::Array(_) => 0x2000,
        _ => u16::MAX,
    });
    let bad = || BackendMsg::WriteOutOfRange {
        value: value.to_string(),
        ty: format!("VT {vt}"),
    };
    let bits = match vt {
        0 if matches!(value, Value::Empty) => 0,
        2 | 3 | 16 | 20 | 22 => {
            let v = signed(value).ok_or_else(bad)?;
            let fits = match vt {
                2 => i16::try_from(v).is_ok(),
                3 | 22 => i32::try_from(v).is_ok(),
                16 => i8::try_from(v).is_ok(),
                _ => true,
            };
            if !fits {
                return Err(bad());
            }
            u64::from_ne_bytes(v.to_ne_bytes())
        }
        17 | 18 | 19 | 21 | 23 => {
            let v = unsigned(value).ok_or_else(bad)?;
            let fits = match vt {
                17 => u8::try_from(v).is_ok(),
                18 => u16::try_from(v).is_ok(),
                19 | 23 => u32::try_from(v).is_ok(),
                _ => true,
            };
            if !fits {
                return Err(bad());
            }
            v
        }
        4 | 5 => {
            let v = value.as_f64().ok_or_else(bad)?;
            if vt == 4 && v.is_finite() && v.abs() > f64::from(f32::MAX) {
                return Err(bad());
            }
            v.to_bits()
        }
        6 => {
            let v = (value.as_f64().ok_or_else(bad)? * 10_000.0).round();
            if !v.is_finite() || v < i64::MIN as f64 || v >= 9_223_372_036_854_775_808.0 {
                return Err(bad());
            }
            u64::from_ne_bytes((v as i64).to_ne_bytes())
        }
        7 => {
            let Value::DateTime(v) = value else {
                return Err(bad());
            };
            let base = NaiveDate::from_ymd_opt(1899, 12, 30)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .ok_or(BackendMsg::DaBadDate)?;
            (v.signed_duration_since(Utc.from_utc_datetime(&base))
                .num_seconds() as f64
                / 86_400.0)
                .to_bits()
        }
        11 => match value {
            Value::Bool(v) => u64::from(*v),
            _ => u64::from(signed(value).ok_or_else(bad)? != 0),
        },
        _ => return Err(BackendMsg::DaVtUnsupported { vt: vt.into() }),
    };
    Ok((vt, bits))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_boundaries_and_unknown_types_are_explicit() {
        assert_eq!(
            encode(&Value::I64(-128), Some(16)).unwrap(),
            (16, (-128i64) as u64)
        );
        assert!(encode(&Value::I64(128), Some(16)).is_err());
        assert!(encode(&Value::U64(u64::MAX), Some(20)).is_err());
        assert_eq!(
            encode(&Value::U64(u64::MAX), Some(21)).unwrap(),
            (21, u64::MAX)
        );
        assert!(encode(&Value::F64(1.25), Some(3)).is_err());
        assert!(encode(&Value::Str("text".into()), Some(8)).is_err());
        let raw = ReadScalar {
            vartype: 20,
            bits_low: u32::MAX,
            bits_high: u32::MAX,
            quality: 0xc0,
            ..Default::default()
        };
        assert_eq!(decode((raw, None)).unwrap().value, Value::I64(-1));
        assert_eq!(decode((ReadScalar { vartype: 8, ..Default::default() }, Some("РУГ 中文".into()))).unwrap().value, Value::Str("РУГ 中文".into()));
        assert!(
            decode((ReadScalar {
                vartype: 0x2003,
                ..Default::default()
            }, None))
            .is_err()
        );
        assert!(
            decode((ReadScalar {
                vartype: 3,
                item_result: -1,
                ..Default::default()
            }, None))
            .is_err()
        );
    }
}
