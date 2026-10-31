//! Mapping for SDK scalars, strings, and bounded one-dimensional UI1/I4 arrays.
//! The SDK retains all VARIANT and SAFEARRAY ownership.
use super::da_convert::{convert_float, convert_int, convert_uint, da_range};
use super::model::{BackendMsg, DataValue, Quality, Value, ValueOrigin, ValueTimes};
use super::softing_runtime::ReadScalar;

/// Map one synchronous SDK read without promoting missing output to a fact.
pub fn decode_read_outcome(outcome: super::softing_runtime::ReadOutcome) -> DataValue {
    use super::softing_runtime::{READ_ITEM, READ_SAMPLE, READ_SERVICE};
    let raw = outcome.value;
    let service = (outcome.available & READ_SERVICE != 0).then_some(raw.service_result);
    let item = (service.is_some_and(|hr| hr >= 0) && outcome.available & READ_ITEM != 0)
        .then_some(raw.item_result);
    let sample = item.is_some_and(|hr| hr >= 0) && outcome.available & READ_SAMPLE != 0;
    let mut cause = None;
    if outcome.adapter_result >= 0 && sample {
        match outcome.payload {
            Ok(payload) => match decode_with_origin((raw, payload), ValueOrigin::Read) {
                Ok(value) => return value,
                Err(error) => cause = Some(Box::new(error)),
            },
            Err(super::softing_runtime::ReadPayloadError::InvalidLength) => {
                cause = Some(Box::new(BackendMsg::DaReadPayloadInvalid));
            }
        }
    }
    let ticks = (u64::from(raw.filetime_high) << 32) | u64::from(raw.filetime_low);
    // The generic Bad quality code is a synthetic display sentinel. Native
    // quality, if any, is carried only by the typed reason below.
    let mut value = DataValue::bad(BackendMsg::DaReadFailure {
        adapter: Some(outcome.adapter_result),
        service,
        item,
        quality: sample.then_some(raw.quality),
        filetime: sample.then_some(ticks),
        vartype: sample.then_some(raw.vartype),
        cause,
    });
    if sample {
        value.timestamp = super::filetime::ticks_to_utc(ticks);
    }
    value.times = ValueTimes::da(value.timestamp, chrono::Utc::now(), ValueOrigin::Read);
    value
}

pub fn decode(value: (ReadScalar, Option<Vec<u8>>)) -> Result<DataValue, BackendMsg> {
    decode_with_origin(value, ValueOrigin::Unknown)
}

pub fn decode_with_origin(
    value: (ReadScalar, Option<Vec<u8>>),
    origin: ValueOrigin,
) -> Result<DataValue, BackendMsg> {
    let mut decoded = decode_inner(value, 0)?;
    decoded.times = ValueTimes::da(decoded.timestamp, chrono::Utc::now(), origin);
    Ok(decoded)
}
fn decode_inner(
    (raw, text): (ReadScalar, Option<Vec<u8>>),
    depth: usize,
) -> Result<DataValue, BackendMsg> {
    if depth >= 32 {
        return Err(BackendMsg::raw("DA SDK nested VARIANT depth"));
    }
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
        1 => Value::Other("NULL".into()),
        10 => Value::Other(format!("HRESULT 0x{:08X}", bits as u32)),
        64 => super::filetime::ticks_to_utc(bits)
            .map(Value::DateTime)
            .unwrap_or(Value::Empty),
        2 | 3 | 16 | 20 | 22 => Value::I64(signed),
        17 | 18 | 19 | 21 | 23 => Value::U64(bits),
        4 | 5 => Value::F64(f64::from_bits(bits)),
        6 => Value::F64(signed as f64 / 10_000.0),
        7 => Value::DateTime(
            super::ole_time::from_ole_date(f64::from_bits(bits)).ok_or(BackendMsg::DaBadDate)?,
        ),
        8 => Value::Str(
            String::from_utf8(text.ok_or_else(|| BackendMsg::raw("DA SDK string missing"))?)
                .map_err(|_| BackendMsg::raw("DA SDK string is not UTF-8"))?,
        ),
        vt if vt & 0x2000 != 0 => decode_primitive_array(vt & !0x2000, text.as_deref(), depth)?,
        11 => Value::Bool(bits != 0),
        vt => return Err(BackendMsg::DaVtUnsupported { vt: vt.into() }),
    };
    let ticks = (u64::from(raw.filetime_high) << 32) | u64::from(raw.filetime_low);
    Ok(DataValue {
        value,
        quality: Quality::from_da(raw.quality),
        timestamp: super::filetime::ticks_to_utc(ticks),
        times: ValueTimes::default(),
        reason: None,
    })
}
fn primitive_array_width(vt: u16) -> Option<usize> {
    match vt {
        16 | 17 => Some(1),
        2 | 11 | 18 => Some(2),
        3 | 4 | 10 | 19 | 22 | 23 => Some(4),
        5 | 6 | 7 | 20 | 21 | 64 => Some(8),
        _ => None,
    }
}
fn decode_primitive_array(vt: u16, data: Option<&[u8]>, depth: usize) -> Result<Value, BackendMsg> {
    if vt == 12 {
        let bytes = data.ok_or_else(|| BackendMsg::raw("DA SDK variant array missing"))?;
        if bytes.len() > 65536 {
            return Err(BackendMsg::raw("DA SDK variant array size"));
        }
        let mut values = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            if values.len() >= 256 {
                return Err(BackendMsg::raw("DA SDK variant array count"));
            }
            let header = bytes
                .get(offset..offset + 6)
                .ok_or_else(|| BackendMsg::raw("DA SDK variant array header"))?;
            let tag = u16::from_le_bytes([header[0], header[1]]);
            let length = u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize;
            offset += 6;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| BackendMsg::raw("DA SDK variant array size"))?;
            let payload = bytes
                .get(offset..end)
                .ok_or_else(|| BackendMsg::raw("DA SDK variant array size"))?;
            let mut raw = ReadScalar {
                vartype: tag,
                ..Default::default()
            };
            let data = if tag == 8 || tag & 0x2000 != 0 {
                Some(payload.to_vec())
            } else {
                if payload.len() != 8 {
                    return Err(BackendMsg::raw("DA SDK variant scalar size"));
                }
                let bits = u64::from_le_bytes(
                    payload
                        .try_into()
                        .map_err(|_| BackendMsg::raw("DA SDK variant scalar"))?,
                );
                raw.bits_low = bits as u32;
                raw.bits_high = (bits >> 32) as u32;
                None
            };
            values.push(decode_inner((raw, data), depth + 1)?.value);
            offset = end;
        }
        return Ok(Value::Array(values));
    }
    if vt == 8 {
        let bytes = data.ok_or_else(|| BackendMsg::raw("DA SDK string array missing"))?;
        if bytes.len() > 65536 {
            return Err(BackendMsg::raw("DA SDK array exceeds 65536 bytes"));
        }
        let mut items = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            if items.len() >= 256 {
                return Err(BackendMsg::raw("DA SDK string array count"));
            }
            let length = bytes
                .get(offset..offset + 4)
                .ok_or_else(|| BackendMsg::raw("DA SDK string array size"))?;
            let length = u32::from_le_bytes([length[0], length[1], length[2], length[3]]) as usize;
            offset += 4;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| BackendMsg::raw("DA SDK string array size"))?;
            let text = bytes
                .get(offset..end)
                .ok_or_else(|| BackendMsg::raw("DA SDK string array size"))?;
            items.push(Value::Str(
                std::str::from_utf8(text)
                    .map_err(|_| BackendMsg::raw("DA SDK string array UTF-8"))?
                    .to_owned(),
            ));
            offset = end;
        }
        return Ok(Value::Array(items));
    }
    let width = primitive_array_width(vt).ok_or(BackendMsg::DaVtUnsupported {
        vt: (vt | 0x2000).into(),
    })?;
    let bytes = data.ok_or_else(|| BackendMsg::raw("DA SDK array missing"))?;
    if bytes.len() > 65536 || bytes.len() % width != 0 {
        return Err(BackendMsg::raw("DA SDK array size"));
    }
    if bytes.len() / width > 256 {
        return Err(BackendMsg::raw("DA SDK array count"));
    }
    let mut items = Vec::with_capacity(bytes.len() / width);
    for chunk in bytes.chunks_exact(width) {
        let mut raw_bytes = [0u8; 8];
        raw_bytes[..width].copy_from_slice(chunk);
        let unsigned = u64::from_le_bytes(raw_bytes);
        let bits = match vt {
            16 => (unsigned as u8 as i8 as i64) as u64,
            2 => (unsigned as u16 as i16 as i64) as u64,
            3 | 22 => (unsigned as u32 as i32 as i64) as u64,
            4 => (f32::from_bits(unsigned as u32) as f64).to_bits(),
            _ => unsigned,
        };
        let value = match vt {
            10 => Value::Other(format!("HRESULT 0x{:08X}", unsigned as u32)),
            64 => super::filetime::ticks_to_utc(unsigned)
                .map(Value::DateTime)
                .unwrap_or(Value::Empty),
            _ => {
                decode((
                    ReadScalar {
                        vartype: vt,
                        bits_low: bits as u32,
                        bits_high: (bits >> 32) as u32,
                        ..Default::default()
                    },
                    None,
                ))?
                .value
            }
        };
        items.push(value);
    }
    Ok(Value::Array(items))
}
// SDK owns the SAFEARRAY. This bounded payload contains POD elements only.
pub fn encode_array(value: &Value, hint: Option<u16>) -> Result<(u16, Vec<u8>), BackendMsg> {
    encode_array_inner(value, hint, 0)
}
fn encode_array_inner(
    value: &Value,
    hint: Option<u16>,
    depth: usize,
) -> Result<(u16, Vec<u8>), BackendMsg> {
    if depth >= 32 {
        return Err(BackendMsg::raw("DA SDK nested VARIANT depth"));
    }
    let (items, vt) = match (value, hint) {
        (Value::Bytes(bytes), None | Some(0x2011)) if bytes.len() <= 256 => {
            return Ok((0x2011, bytes.clone()));
        }
        (Value::Array(items), Some(vt)) => (items.as_slice(), vt & !0x2000),
        (single, Some(vt)) if vt & 0x2000 != 0 => (std::slice::from_ref(single), vt & !0x2000),
        (Value::Array(items), None) => (
            items.as_slice(),
            match items.first() {
                None | Some(Value::Str(_)) | Some(Value::Empty) => 8,
                Some(Value::Array(_)) | Some(Value::Bytes(_)) => 12,
                Some(first) => encode(first, None)?.0,
            },
        ),
        _ => return Err(BackendMsg::raw("DA array needs a supported canonical type")),
    };
    if vt == 12 {
        if items.len() > 256 {
            return Err(BackendMsg::raw("DA SDK variant array count"));
        }
        let mut bytes = Vec::new();
        for item in items {
            if depth + 1 >= 32 {
                return Err(BackendMsg::raw("DA SDK nested VARIANT depth"));
            }
            let (tag, payload) = match item {
                Value::Str(text) => {
                    if text.contains('\0') {
                        return Err(BackendMsg::raw("DA string contains NUL"));
                    }
                    (8, text.as_bytes().to_vec())
                }
                Value::Array(_) | Value::Bytes(_) => encode_array_inner(item, None, depth + 1)?,
                _ => {
                    let (tag, bits) = encode(item, None)?;
                    (tag, bits.to_le_bytes().to_vec())
                }
            };
            if bytes.len().saturating_add(6).saturating_add(payload.len()) > 65536 {
                return Err(BackendMsg::raw("DA SDK variant array exceeds 65536 bytes"));
            }
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&payload);
        }
        return Ok((0x200c, bytes));
    }
    if vt == 8 {
        if items.len() > 256 {
            return Err(BackendMsg::raw("DA SDK string array count"));
        }
        let mut bytes = Vec::new();
        for item in items {
            let text = match item {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            if text.contains('\0')
                || bytes.len().saturating_add(4).saturating_add(text.len()) > 65536
            {
                return Err(BackendMsg::raw(
                    "DA string array contains NUL or exceeds 65536 bytes",
                ));
            }
            bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        return Ok((0x2008, bytes));
    }
    let width = primitive_array_width(vt).ok_or(BackendMsg::DaVtUnsupported {
        vt: (vt | 0x2000).into(),
    })?;
    if items.len() > 256 || items.len() > 65536 / width {
        return Err(BackendMsg::raw("DA SDK array exceeds 65536 bytes"));
    }
    let mut bytes = Vec::with_capacity(items.len() * width);
    for item in items {
        let (_, bits) = encode(item, Some(vt))?;
        let bits = match vt {
            4 => u64::from((f64::from_bits(bits) as f32).to_bits()),
            11 => {
                if bits == 0 {
                    0
                } else {
                    u64::from(u16::MAX)
                }
            }
            _ => bits,
        };
        bytes.extend_from_slice(&bits.to_le_bytes()[..width]);
    }
    Ok((vt | 0x2000, bytes))
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
    let ty = match vt {
        2 => "I2",
        3 => "I4",
        4 => "R4",
        6 => "CY",
        16 => "I1",
        17 => "UI1",
        18 => "UI2",
        19 => "UI4",
        22 => "INT",
        23 => "UINT",
        _ => "DA",
    };
    let bad = || da_range(value, ty);
    let bits = match vt {
        0 => 0,
        2 | 3 | 16 | 20 | 22 => {
            let v = convert_int(value)?;
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
            let v = convert_uint(value)?;
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
            let v = convert_float(value)?;
            if vt == 4 && v.is_finite() && v.abs() > f64::from(f32::MAX) {
                return Err(bad());
            }
            v.to_bits()
        }
        6 => {
            let v = (convert_float(value)? * 10_000.0).round();
            if !v.is_finite() || v < i64::MIN as f64 || v >= 9_223_372_036_854_775_808.0 {
                return Err(bad());
            }
            u64::from_ne_bytes((v as i64).to_ne_bytes())
        }
        7 => {
            let Value::DateTime(value) = value else {
                return Err(BackendMsg::DaDateRequired);
            };
            super::ole_time::to_ole_date(value)
                .ok_or(BackendMsg::DaBadDate)?
                .to_bits()
        }
        11 => u64::from(match value {
            Value::Bool(v) => *v,
            Value::I64(v) => *v != 0,
            Value::U64(v) => *v != 0,
            Value::F64(v) => *v != 0.0,
            Value::Str(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "да"),
            _ => false,
        }),
        _ => return Err(BackendMsg::DaVtUnsupported { vt: vt.into() }),
    };
    Ok((vt, bits))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_failure_keeps_only_attested_facts_and_never_reports_good() {
        use super::super::softing_runtime::{READ_ITEM, READ_SAMPLE, READ_SERVICE, ReadOutcome};
        let ticks = 116_444_736_010_000_000u64;
        let raw = ReadScalar {
            service_result: 0,
            item_result: 0,
            filetime_low: ticks as u32,
            filetime_high: (ticks >> 32) as u32,
            quality: 0xc0,
            vartype: 0x2003,
            ..Default::default()
        };
        let failed = decode_read_outcome(ReadOutcome {
            adapter_result: -2147024774,
            available: READ_SERVICE | READ_ITEM | READ_SAMPLE,
            value: raw,
            payload: Ok(None),
        });
        assert_eq!(failed.quality.kind, super::super::model::QualityKind::Bad);
        assert!(failed.timestamp.is_some());
        assert_eq!(failed.times.da_timestamp, failed.timestamp);
        assert!(
            matches!(failed.reason, Some(BackendMsg::DaReadFailure { adapter: Some(-2147024774), service: Some(0), item: Some(0), quality: Some(0xc0), filetime: Some(t), vartype: Some(0x2003), .. }) if t == ticks)
        );
        let service = decode_read_outcome(ReadOutcome {
            adapter_result: 0,
            available: READ_SERVICE,
            value: ReadScalar {
                service_result: -1,
                ..raw
            },
            payload: Ok(None),
        });
        assert_eq!(service.quality.kind, super::super::model::QualityKind::Bad);
        assert!(service.timestamp.is_none());
        assert_eq!(service.times.origin, ValueOrigin::Read);
        assert!(service.times.received_at.is_some());
        assert!(service.times.da_timestamp.is_none());
        assert!(matches!(
            service.reason,
            Some(BackendMsg::DaReadFailure {
                service: Some(-1),
                item: None,
                quality: None,
                filetime: None,
                vartype: None,
                ..
            })
        ));
        let item = decode_read_outcome(ReadOutcome {
            adapter_result: 0,
            available: READ_SERVICE | READ_ITEM,
            value: ReadScalar {
                item_result: -1,
                ..raw
            },
            payload: Ok(None),
        });
        assert!(matches!(
            item.reason,
            Some(BackendMsg::DaReadFailure {
                service: Some(0),
                item: Some(-1),
                quality: None,
                filetime: None,
                vartype: None,
                ..
            })
        ));
        let malformed = decode_read_outcome(ReadOutcome {
            adapter_result: 0,
            available: READ_SERVICE | READ_ITEM | READ_SAMPLE,
            value: raw,
            payload: Err(super::super::softing_runtime::ReadPayloadError::InvalidLength),
        });
        assert!(
            matches!(malformed.reason, Some(BackendMsg::DaReadFailure { quality: Some(0xc0), cause: Some(cause), .. }) if *cause == BackendMsg::DaReadPayloadInvalid)
        );
        let no_sdk = decode_read_outcome(ReadOutcome {
            adapter_result: -2,
            available: 0,
            value: raw,
            payload: Ok(None),
        });
        assert!(matches!(
            no_sdk.reason,
            Some(BackendMsg::DaReadFailure {
                adapter: Some(-2),
                service: None,
                item: None,
                quality: None,
                filetime: None,
                vartype: None,
                ..
            })
        ));
        let decoded = decode_read_outcome(ReadOutcome {
            adapter_result: 0,
            available: READ_SERVICE | READ_ITEM | READ_SAMPLE,
            value: ReadScalar {
                vartype: 0xffff,
                ..raw
            },
            payload: Ok(None),
        });
        assert_eq!(decoded.quality.kind, super::super::model::QualityKind::Bad);
        assert!(matches!(
            decoded.reason,
            Some(BackendMsg::DaReadFailure {
                quality: Some(0xc0),
                cause: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn da_timestamp_and_origin_are_separate_from_ua_facts() {
        let ticks = 116_444_736_010_000_000u64;
        for origin in [ValueOrigin::Read, ValueOrigin::Subscription] {
            let value = decode_with_origin(
                (
                    ReadScalar {
                        vartype: 3,
                        bits_low: 42,
                        filetime_low: ticks as u32,
                        filetime_high: (ticks >> 32) as u32,
                        ..Default::default()
                    },
                    None,
                ),
                origin,
            )
            .unwrap();
            assert_eq!(value.times.origin, origin);
            assert_eq!(value.times.da_timestamp, value.timestamp);
            assert!(value.timestamp.is_some());
            assert!(value.times.received_at.is_some());
            assert_eq!(value.times.ua_source, None);
            assert_eq!(value.times.ua_server, None);
            assert_eq!(value.times.source_clock, None);
        }
    }
    #[test]
    fn bounded_arrays_are_lossless_and_validate_elements() {
        let data = Value::Array(vec![Value::I64(-2), Value::I64(0), Value::I64(42)]);
        let (vt, bytes) = encode_array(&data, Some(0x2003)).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: vt,
                    ..Default::default()
                },
                Some(bytes)
            ))
            .unwrap()
            .value,
            data
        );
        let bytes = Value::Bytes(vec![0, 127, 255]);
        let (vt, payload) = encode_array(&bytes, None).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: vt,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            Value::Array(vec![Value::U64(0), Value::U64(127), Value::U64(255)])
        );
        for (vt, item) in [
            (16, Value::I64(-3)),
            (2, Value::I64(-32000)),
            (18, Value::U64(65000)),
            (19, Value::U64(4_000_000_000)),
            (20, Value::I64(i64::MIN)),
            (21, Value::U64(u64::MAX)),
            (4, Value::F64(1.5)),
            (5, Value::F64(3.25)),
            (6, Value::F64(-1.25)),
            (11, Value::Bool(true)),
        ] {
            let original = Value::Array(vec![item]);
            let (tag, payload) = encode_array(&original, Some(vt)).unwrap();
            assert_eq!(
                decode((
                    ReadScalar {
                        vartype: tag,
                        ..Default::default()
                    },
                    Some(payload)
                ))
                .unwrap()
                .value,
                original
            );
        }
        let nested = Value::Array(vec![
            Value::I64(-42),
            Value::Str("РУГ 中文".into()),
            Value::Array(vec![Value::U64(2), Value::U64(255)]),
        ]);
        let (tag, payload) = encode_array(&nested, Some(0x200c)).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: tag,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            nested
        );
        assert!(
            decode((
                ReadScalar {
                    vartype: 0x200c,
                    ..Default::default()
                },
                Some(vec![1])
            ))
            .is_err()
        );
        let mut deep = Value::I64(1);
        for _ in 0..33 {
            deep = Value::Array(vec![deep]);
        }
        assert!(encode_array(&deep, Some(0x200c)).is_err());
        let mut wire = Vec::new();
        wire.extend_from_slice(&3u16.to_le_bytes());
        wire.extend_from_slice(&8u32.to_le_bytes());
        wire.extend_from_slice(&1u64.to_le_bytes());
        for _ in 0..33 {
            let mut outer = Vec::new();
            outer.extend_from_slice(&0x200cu16.to_le_bytes());
            outer.extend_from_slice(&(wire.len() as u32).to_le_bytes());
            outer.extend_from_slice(&wire);
            wire = outer;
        }
        assert!(
            decode((
                ReadScalar {
                    vartype: 0x200c,
                    ..Default::default()
                },
                Some(wire)
            ))
            .is_err()
        );

        let strings = Value::Array(vec![
            Value::Str("РУГ 中文".into()),
            Value::Str(String::new()),
        ]);
        let (tag, payload) = encode_array(&strings, None).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: tag,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            strings
        );
        assert!(encode_array(&Value::Array(vec![Value::Str("a\0b".into())]), None).is_err());
        assert!(
            decode((
                ReadScalar {
                    vartype: 0x2008,
                    ..Default::default()
                },
                Some(vec![2, 0, 0, 0, 1])
            ))
            .is_err()
        );
        assert!(encode_array(&Value::Bytes(vec![0; 65_537]), None).is_err());
        assert!(encode_array(&Value::Bytes(vec![0; 257]), None).is_err());
        let (tag, payload) = encode_array(&Value::I64(-5), Some(0x2003)).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: tag,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            Value::Array(vec![Value::I64(-5)])
        );
        let largest = Value::Array(vec![Value::Str("x".repeat(65_532))]);
        let (tag, payload) = encode_array(&largest, Some(8)).unwrap();
        assert_eq!(payload.len(), 65_536);
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: tag,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            largest
        );
        assert!(
            encode_array(&Value::Array(vec![Value::Str("x".repeat(65_533))]), Some(8)).is_err()
        );

        assert!(encode_array(&Value::Array(vec![Value::I64(i64::MAX)]), Some(0x2003)).is_err());
        assert!(
            decode((
                ReadScalar {
                    vartype: 0x2003,
                    ..Default::default()
                },
                Some(vec![1])
            ))
            .is_err()
        );
    }
    #[test]
    fn ole_before_epoch_is_shared_for_scalar_and_array() {
        let value = decode((
            ReadScalar {
                vartype: 7,
                bits_low: (-1.25f64).to_bits() as u32,
                bits_high: ((-1.25f64).to_bits() >> 32) as u32,
                ..Default::default()
            },
            None,
        ))
        .unwrap()
        .value;
        assert!(matches!(&value, Value::DateTime(time) if time.timestamp() == -2_209_226_400));
        assert_eq!(encode(&value, Some(7)).unwrap().1, (-1.25f64).to_bits());
        let array = Value::Array(vec![value]);
        let (tag, payload) = encode_array(&array, Some(7)).unwrap();
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: tag,
                    ..Default::default()
                },
                Some(payload)
            ))
            .unwrap()
            .value,
            array
        );
    }
    #[test]
    fn scalar_boundaries_and_unknown_types_are_explicit() {
        assert!(matches!(
            encode(&Value::Str("not-int".into()), Some(3)),
            Err(BackendMsg::DaNotInteger { .. })
        ));
        assert!(matches!(
            encode(&Value::I64(-1), Some(19)),
            Err(BackendMsg::DaNegativeValue { .. })
        ));
        assert!(matches!(
            encode(&Value::Array(vec![]), Some(5)),
            Err(BackendMsg::DaNotNumberAny)
        ));
        assert_eq!(encode(&Value::Str("да".into()), Some(11)).unwrap(), (11, 1));
        assert_eq!(encode(&Value::F64(-0.25), Some(11)).unwrap(), (11, 1));
        assert_eq!(
            encode(&Value::Str("off".into()), Some(11)).unwrap(),
            (11, 0)
        );
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
        assert_eq!(
            decode((
                ReadScalar {
                    vartype: 8,
                    ..Default::default()
                },
                Some("РУГ 中文".as_bytes().to_vec())
            ))
            .unwrap()
            .value,
            Value::Str("РУГ 中文".into())
        );
        assert!(
            decode((
                ReadScalar {
                    vartype: 0x2003,
                    ..Default::default()
                },
                None
            ))
            .is_err()
        );
        assert!(
            decode((
                ReadScalar {
                    vartype: 3,
                    item_result: -1,
                    ..Default::default()
                },
                None
            ))
            .is_err()
        );
    }
}
