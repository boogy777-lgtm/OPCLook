use crate::bff::model::Value;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayCreateVector, SafeArrayDestroy, SafeArrayGetDim,
    SafeArrayGetElemsize, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayLock,
    SafeArrayUnaccessData, SafeArrayUnlock,
};
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_DATE, VT_EMPTY,
};
use windows::core::BSTR;

const MAX_ARRAY_ITEMS: usize = 256;

pub fn filetime_to_utc(ft: FILETIME) -> Option<DateTime<Utc>> {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    if ticks == 0 {
        return None;
    }
    let secs = (ticks / 10_000_000) as i64;
    let nanos = ((ticks % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(11_644_473_600 + secs, nanos).single()
}

fn ole_date_to_utc(days: f64) -> Option<DateTime<Utc>> {
    if !days.is_finite() {
        return None;
    }
    let base = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let whole = days.trunc() as i64;
    let frac = days.fract();
    let secs = (frac * 86_400.0).round() as i64;
    let dt = base.checked_add_signed(chrono::Duration::days(whole))?;
    let dt = dt.checked_add_signed(chrono::Duration::seconds(secs))?;
    Some(Utc.from_utc_datetime(&dt))
}

fn utc_to_ole_date(dt: DateTime<Utc>) -> Option<f64> {
    let base = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let base = Utc.from_utc_datetime(&base);
    let delta = dt.signed_duration_since(base);
    Some(delta.num_seconds() as f64 / 86_400.0)
}

/// Converts a COM VARIANT into the protocol-neutral BFF value.
///
/// # Safety
/// The caller guarantees `variant` contains a valid COM VARIANT owned by the caller.
pub unsafe fn variant_to_value(variant: &VARIANT) -> Value {
    let vt = unsafe { variant.Anonymous.Anonymous.vt };
    let raw = vt.0;
    let is_array = raw & 0x2000 != 0;
    let base = raw & 0x0FFF;

    if is_array {
        return unsafe { array_to_value(variant, base) };
    }

    unsafe { scalar_to_value(variant, base) }
}

unsafe fn scalar_to_value(variant: &VARIANT, base: u16) -> Value {
    let body = unsafe { &variant.Anonymous.Anonymous.Anonymous };
    match base {
        0 => Value::Empty,
        1 => Value::Other("NULL".to_string()),
        2 => Value::I64(i64::from(unsafe { body.iVal })),
        3 => Value::I64(i64::from(unsafe { body.lVal })),
        4 => Value::F64(f64::from(unsafe { body.fltVal })),
        5 => Value::F64(unsafe { body.dblVal }),
        6 => Value::F64(unsafe { body.cyVal.int64 } as f64 / 10_000.0),
        7 => match ole_date_to_utc(unsafe { body.date }) {
            Some(dt) => Value::DateTime(dt),
            None => Value::Other("Invalid DATE".to_string()),
        },
        8 => {
            let bstr = unsafe { &body.bstrVal };
            Value::Str(bstr.to_string())
        }
        10 => Value::Other(format!("HRESULT 0x{:08X}", unsafe { body.scode } as u32)),
        11 => Value::Bool(unsafe { body.boolVal.0 } != 0),
        12 => {
            let nested = unsafe { body.pvarVal };
            if nested.is_null() {
                Value::Empty
            } else {
                unsafe { variant_to_value(&*nested) }
            }
        }
        16 => Value::I64(i64::from(unsafe { body.bVal as i8 })),
        17 => Value::U64(u64::from(unsafe { body.bVal })),
        18 => Value::U64(u64::from(unsafe { body.uiVal })),
        19 => Value::U64(u64::from(unsafe { body.ulVal })),
        20 => Value::I64(unsafe { *(&raw const *body).cast::<i64>() }),
        21 => Value::U64(unsafe { *(&raw const *body).cast::<u64>() }),
        22 => Value::I64(i64::from(unsafe { body.intVal })),
        23 => Value::U64(u64::from(unsafe { body.uintVal })),
        24 | 25 => Value::Empty,
        64 => {
            let ft = unsafe { *(&raw const *body).cast::<FILETIME>() };
            match filetime_to_utc(ft) {
                Some(dt) => Value::DateTime(dt),
                None => Value::Empty,
            }
        }
        30 | 31 => Value::Other(format!("VT {base}")),
        72 => Value::Other("CLSID".to_string()),
        other => Value::Other(format!("VT {other}")),
    }
}

unsafe fn array_to_value(variant: &VARIANT, base: u16) -> Value {
    let parray = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
    if parray.is_null() {
        return Value::Array(Vec::new());
    }
    if unsafe { SafeArrayGetDim(parray) } != 1 {
        return Value::Other("Multi-dimensional array".to_string());
    }
    let lb = unsafe { SafeArrayGetLBound(parray, 1) }.unwrap_or(0);
    let ub = unsafe { SafeArrayGetUBound(parray, 1) }.unwrap_or(-1);
    let count = (ub - lb + 1).max(0) as usize;
    let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
    if unsafe { SafeArrayAccessData(parray, &raw mut data) }.is_err() {
        return Value::Array(Vec::new());
    }

    let mut out = Vec::with_capacity(count.min(MAX_ARRAY_ITEMS));
    if base == 12 {
        let len = count.min(MAX_ARRAY_ITEMS);
        let vars = unsafe { std::slice::from_raw_parts(data.cast::<VARIANT>(), len) };
        for v in vars {
            out.push(unsafe { variant_to_value(v) });
        }
    } else {
        let elem_size = unsafe { SafeArrayGetElemsize(parray) } as usize;
        let len = count.min(MAX_ARRAY_ITEMS);
        for i in 0..len {
            let mut tmp = VARIANT::default();
            unsafe {
                set_vt(&mut tmp, base);
                let src = (data as *const u8).add(i * elem_size);
                let dst = variant_inner(&mut tmp).cast::<u8>();
                std::ptr::copy_nonoverlapping(src, dst, elem_size.min(16));
            }
            out.push(unsafe { scalar_to_value(&tmp, base) });
        }
    }
    unsafe {
        let _ = SafeArrayUnaccessData(parray);
    }
    Value::Array(out)
}

fn set_vt(variant: &mut VARIANT, vt: u16) {
    unsafe {
        *(&raw mut (*variant.Anonymous.Anonymous).vt) = VARENUM(vt);
    }
}

fn variant_inner(variant: &mut VARIANT) -> *mut VARIANT_0_0_0 {
    unsafe { &raw mut (*variant.Anonymous.Anonymous).Anonymous }
}

/// Builds a COM VARIANT for a write operation. `hint` is an OPC DA VT_* code.
///
/// Arrays are supported: when `value` is a [`Value::Array`] or the hint carries
/// the `VT_ARRAY` (0x2000) bit, a one-dimensional SAFEARRAY is built whose
/// element type is the scalar VT. An array value with a scalar hint is turned
/// into an array of that element type.
pub fn variant_from_value(value: &Value, hint: Option<u16>) -> Result<VARIANT, String> {
    let vt = hint.unwrap_or_else(|| default_vt(value));

    // When the value is an array (or the hint marks an array) route to the SAFEARRAY path.
    let is_array_value = matches!(value, Value::Array(_));
    let is_array_vt = vt & 0x2000 != 0;
    if is_array_vt || is_array_value {
        let mut element_vt = vt & 0x0FFF;
        // A bare array value without a type hint / element type cannot build a
        // SAFEARRAY; infer the element type from the first element.
        if element_vt == 0 {
            if let Value::Array(items) = value {
                element_vt = items
                    .first()
                    .map(|item| default_vt(item) & 0x0FFF)
                    .unwrap_or(VT_BSTR.0);
            } else {
                element_vt = VT_BSTR.0;
            }
        }
        if element_vt == 0 {
            element_vt = VT_BSTR.0;
        }
        let outer_vt = element_vt | 0x2000;
        return build_array_variant(value, element_vt, outer_vt);
    }

    let mut variant = VARIANT::default();
    set_vt(&mut variant, vt);
    let inner = variant_inner(&mut variant);
    unsafe { write_scalar(value, vt, inner)? };
    Ok(variant)
}

/// Writes one scalar (non-array) value into the VARIANT named union.
unsafe fn write_scalar(value: &Value, vt: u16, inner: *mut VARIANT_0_0_0) -> Result<(), String> {
    match vt {
        0 => {}
        2 => {
            let v = convert_int(value)?.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
            unsafe { (*inner).iVal = v };
        }
        3 => {
            let v = convert_int(value)?.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            unsafe { (*inner).lVal = v };
        }
        4 => {
            let v = convert_float(value)? as f32;
            unsafe { (*inner).fltVal = v };
        }
        5 => {
            let v = convert_float(value)?;
            unsafe { (*inner).dblVal = v };
        }
        6 => {
            let v = (convert_float(value)? * 10_000.0).round();
            unsafe { (*inner).cyVal.int64 = v as i64 };
        }
        7 => {
            let dt = match value {
                Value::DateTime(dt) => *dt,
                _ => return Err("для типа DATE требуется дата/время".to_string()),
            };
            let ole = utc_to_ole_date(dt).ok_or_else(|| "некорректная дата".to_string())?;
            unsafe { (*inner).date = ole };
        }
        8 => {
            let s = match value {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            unsafe {
                std::ptr::write(
                    &raw mut (*inner).bstrVal,
                    std::mem::ManuallyDrop::new(BSTR::from(s)),
                );
            }
        }
        11 => {
            let b = match value {
                Value::Bool(b) => *b,
                Value::I64(v) => *v != 0,
                Value::U64(v) => *v != 0,
                Value::F64(v) => *v != 0.0,
                Value::Str(s) => {
                    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "да")
                }
                _ => false,
            };
            unsafe {
                (*inner).boolVal = VARIANT_BOOL(if b { -1 } else { 0 });
            }
        }
        16 => unsafe { (*inner).bVal = convert_int(value)? as i8 as u8 },
        17 => unsafe { (*inner).bVal = convert_uint(value)? as u8 },
        18 => unsafe { (*inner).uiVal = convert_uint(value)? as u16 },
        19 => unsafe { (*inner).ulVal = convert_uint(value)? as u32 },
        20 => unsafe { std::ptr::write(inner.cast::<i64>(), convert_int(value)?) },
        21 => unsafe {
            std::ptr::write(inner.cast::<u64>(), convert_uint(value)?);
        },
        22 => unsafe { (*inner).intVal = convert_int(value)? as i32 },
        23 => unsafe { (*inner).uintVal = convert_uint(value)? as u32 },
        other => return Err(format!("запись типа VT {other} не поддерживается")),
    }
    Ok(())
}

/// Builds a `VT_x | VT_ARRAY` VARIANT holding a single-dimension SAFEARRAY.
fn build_array_variant(value: &Value, element_vt: u16, outer_vt: u16) -> Result<VARIANT, String> {
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };

    let psa = unsafe {
        SafeArrayCreateVector(
            windows::Win32::System::Variant::VARENUM(element_vt),
            0,
            items.len() as u32,
        )
    };
    if psa.is_null() {
        return Err(format!("не удалось создать массив VT {element_vt}"));
    }

    let element_size: usize = match element_vt {
        16 | 17 => 1,
        2 | 18 => 2,
        3 | 19 | 11 => 4,
        4 | 5 | 6 | 7 | 20 | 21 => 8,
        8 => std::mem::size_of::<windows_core::BSTR>(),
        12 => std::mem::size_of::<VARIANT>(),
        other => {
            let _ = unsafe { SafeArrayDestroy(psa) };
            return Err(format!("запись массива VT {other} не поддерживается"));
        }
    };

    // Lock the array and write each element in place.
    let locked = unsafe { SafeArrayLock(psa) }.is_ok();
    if !locked {
        let _ = unsafe { SafeArrayDestroy(psa) };
        return Err(format!("не удалось заблокировать массив VT {element_vt}"));
    }
    let mut access: *mut core::ffi::c_void = std::ptr::null_mut();
    if unsafe { SafeArrayAccessData(psa, &raw mut access) }.is_err() {
        unsafe {
            let _ = SafeArrayUnlock(psa);
            let _ = SafeArrayDestroy(psa);
        }
        return Err("не удалось получить доступ к данным массива".to_string());
    }
    let data = access as *mut u8;

    let mut build_result: Result<(), String> = Ok(());
    for (i, item) in items.iter().enumerate() {
        match element_vt {
            12 => {
                // VT_VARIANT array: build a nested VARIANT for each element.
                let nested = variant_from_value(item, None)?;
                let dst = unsafe { (data.cast::<VARIANT>()).add(i) };
                unsafe { std::ptr::write(dst, nested) };
            }
            _ => {
                let mut tmp = VARIANT::default();
                set_vt(&mut tmp, element_vt);
                if let Err(e) = (|| {
                    unsafe { write_scalar(item, element_vt, variant_inner(&mut tmp)) }?;
                    Ok::<(), String>(())
                })() {
                    build_result = Err(e);
                    break;
                }
                // Copy the element value from the temporary VARIANT into the array data.
                let src = variant_inner(&mut tmp).cast::<u8>();
                let dst = unsafe { data.add(i * element_size) };
                unsafe { std::ptr::copy_nonoverlapping(src, dst, element_size) };
                // The element (e.g. a BSTR) is now owned by the safe array; forget the
                // temporary VARIANT so the element is not released later.
                std::mem::forget(tmp);
            }
        }
    }

    unsafe {
        let _ = SafeArrayUnaccessData(psa);
        let _ = SafeArrayUnlock(psa);
    }
    if let Err(e) = build_result {
        let _ = unsafe { SafeArrayDestroy(psa) };
        return Err(e);
    }

    let mut variant = VARIANT::default();
    set_vt(&mut variant, outer_vt);
    let inner = variant_inner(&mut variant);
    unsafe {
        (*inner).parray = psa;
    }
    Ok(variant)
}

pub fn default_vt(value: &Value) -> u16 {
    match value {
        Value::Empty => VT_EMPTY.0,
        Value::Bool(_) => VT_BOOL.0,
        Value::I64(v) if i32::try_from(*v).is_ok() => 3,
        Value::I64(_) => 20,
        Value::U64(_) => 21,
        Value::F64(_) => 5,
        Value::Str(_) => VT_BSTR.0,
        Value::DateTime(_) => VT_DATE.0,
        Value::Bytes(_) => VT_EMPTY.0,
        Value::Array(_) => VT_EMPTY.0,
        Value::NodeId(_) => VT_BSTR.0,
        Value::QualifiedName { .. } => VT_BSTR.0,
        Value::LocalizedText { .. } => VT_BSTR.0,
        Value::Other(_) => VT_EMPTY.0,
    }
}

fn convert_int(value: &Value) -> Result<i64, String> {
    match value {
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::I64(v) => Ok(*v),
        Value::U64(v) => Ok(*v as i64),
        Value::F64(v) => Ok(*v as i64),
        Value::Str(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("значение \"{s}\" не является целым числом")),
        Value::DateTime(dt) => Ok(dt.timestamp()),
        _ => Err("значение не приводится к целому".to_string()),
    }
}

fn convert_uint(value: &Value) -> Result<u64, String> {
    match value {
        Value::Bool(b) => Ok(u64::from(*b)),
        Value::I64(v) => u64::try_from(*v).map_err(|_| format!("значение {v} отрицательное")),
        Value::U64(v) => Ok(*v),
        Value::F64(v) if v.is_finite() && *v >= 0.0 => Ok(*v as u64),
        Value::Str(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("значение \"{s}\" не является целым числом без знака")),
        _ => Err("значение не приводится к целому без знака".to_string()),
    }
}

fn convert_float(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| "значение не приводится к числу".to_string())
}

pub fn vt_name(vt: u16) -> &'static str {
    match vt {
        0 => "VT_EMPTY",
        1 => "VT_NULL",
        2 => "VT_I2",
        3 => "VT_I4",
        4 => "VT_R4",
        5 => "VT_R8",
        6 => "VT_CY",
        7 => "VT_DATE",
        8 => "VT_BSTR",
        9 => "VT_DISPATCH",
        10 => "VT_ERROR",
        11 => "VT_BOOL",
        12 => "VT_VARIANT",
        13 => "VT_UNKNOWN",
        14 => "VT_DECIMAL",
        16 => "VT_I1",
        17 => "VT_UI1",
        18 => "VT_UI2",
        19 => "VT_UI4",
        20 => "VT_I8",
        21 => "VT_UI8",
        22 => "VT_INT",
        23 => "VT_UINT",
        _ => "VT_UNKNOWN",
    }
}

pub fn clear_variant(variant: &mut VARIANT) {
    unsafe {
        let _ = windows::Win32::System::Variant::VariantClear(variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Round-trips a value through a VARIANT of the given VT and asserts value equality.
    fn roundtrip(vt: u16, value: &Value) {
        let mut variant = variant_from_value(value, Some(vt)).expect("build variant");
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(&out, value, "roundtrip mismatch for VT {vt}");
        clear_variant(&mut variant);
    }

    #[test]
    fn roundtrip_vt_bool() {
        roundtrip(11, &Value::Bool(true));
        roundtrip(11, &Value::Bool(false));
    }

    #[test]
    fn roundtrip_vt_i1() {
        roundtrip(16, &Value::I64(-128));
        roundtrip(16, &Value::I64(127));
        roundtrip(16, &Value::I64(0));
    }

    #[test]
    fn roundtrip_vt_i2() {
        roundtrip(2, &Value::I64(-32768));
        roundtrip(2, &Value::I64(32767));
        roundtrip(2, &Value::I64(-5));
    }

    #[test]
    fn roundtrip_vt_i4() {
        roundtrip(3, &Value::I64(-2147483648));
        roundtrip(3, &Value::I64(2147483647));
        roundtrip(3, &Value::I64(42));
    }

    #[test]
    fn roundtrip_vt_i8() {
        roundtrip(20, &Value::I64(i64::MIN));
        roundtrip(20, &Value::I64(i64::MAX));
        roundtrip(20, &Value::I64(-999));
    }

    #[test]
    fn roundtrip_vt_ui1() {
        roundtrip(17, &Value::U64(0));
        roundtrip(17, &Value::U64(255));
        roundtrip(17, &Value::U64(128));
        // i64 input in range is stored as the byte value and read back as U64.
        let mut v = variant_from_value(&Value::I64(200), Some(17)).unwrap();
        assert_eq!(unsafe { variant_to_value(&v) }, Value::U64(200));
        clear_variant(&mut v);
    }

    #[test]
    fn roundtrip_vt_ui2() {
        roundtrip(18, &Value::U64(0));
        roundtrip(18, &Value::U64(65535));
        roundtrip(18, &Value::U64(1024));
    }

    #[test]
    fn roundtrip_vt_ui4() {
        roundtrip(19, &Value::U64(0));
        roundtrip(19, &Value::U64(4294967295));
        roundtrip(19, &Value::U64(123456));
    }

    #[test]
    fn roundtrip_vt_ui8() {
        roundtrip(21, &Value::U64(u64::MAX));
        roundtrip(21, &Value::U64(999));
    }

    #[test]
    fn roundtrip_vt_r4() {
        roundtrip(4, &Value::F64(1.5));
        roundtrip(4, &Value::F64(-273.25));
        roundtrip(4, &Value::F64(0.0));
    }

    #[test]
    fn roundtrip_vt_r8() {
        roundtrip(5, &Value::F64(3.14159));
        roundtrip(5, &Value::F64(-1.0e-9));
    }

    #[test]
    fn roundtrip_vt_cy() {
        roundtrip(6, &Value::F64(12.34));
        roundtrip(6, &Value::F64(0.0));
        roundtrip(6, &Value::F64(-5.6789));
    }

    #[test]
    fn roundtrip_vt_bstr() {
        roundtrip(8, &Value::Str("hello".to_string()));
        roundtrip(8, &Value::Str("".to_string()));
        roundtrip(8, &Value::Str("with пробелы и 漢字".to_string()));
    }

    #[test]
    fn roundtrip_vt_date() {
        let dt = Utc.with_ymd_and_hms(2026, 1, 31, 12, 30, 45).unwrap();
        roundtrip(7, &Value::DateTime(dt));
        let dt0 = Utc.timestamp_opt(0, 0).unwrap();
        roundtrip(7, &Value::DateTime(dt0));
    }

    #[test]
    fn roundtrip_array_of_i4() {
        roundtrip(
            3 | 0x2000,
            &Value::Array(vec![Value::I64(1), Value::I64(-2), Value::I64(300)]),
        );
    }

    #[test]
    fn roundtrip_array_of_r8() {
        roundtrip(
            5 | 0x2000,
            &Value::Array(vec![Value::F64(1.5), Value::F64(-2.25)]),
        );
    }

    #[test]
    fn roundtrip_array_of_bool() {
        roundtrip(
            11 | 0x2000,
            &Value::Array(vec![Value::Bool(true), Value::Bool(false)]),
        );
    }

    #[test]
    fn roundtrip_array_of_i8() {
        roundtrip(
            20 | 0x2000,
            &Value::Array(vec![Value::I64(-1), Value::I64(0), Value::I64(1)]),
        );
    }

    #[test]
    fn roundtrip_array_of_bstr() {
        let value = Value::Array(vec![
            Value::Str("a".to_string()),
            Value::Str("b".to_string()),
        ]);
        let mut variant = variant_from_value(&value, Some(8 | 0x2000)).expect("bstr array");
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(&out, &value);
        clear_variant(&mut variant);
    }

    #[test]
    fn roundtrip_variant_array_without_hint_infers_element_type() {
        let value = Value::Array(vec![Value::I64(7), Value::I64(8)]);
        let mut variant = variant_from_value(&value, None).expect("inferred array");
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(&out, &value);
        clear_variant(&mut variant);
    }

    #[test]
    fn roundtrip_scalar_to_array_of_one_via_array_bit() {
        // An array-typed hint turns a scalar value into a one-element array.
        let value = Value::Bool(true);
        let mut variant = variant_from_value(&value, Some(11 | 0x2000)).expect("array bool");
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(out, Value::Array(vec![Value::Bool(true)]));
        clear_variant(&mut variant);
    }

    #[test]
    fn i1_negative_roundtrip_preserves_sign() {
        let value = Value::I64(-42);
        let mut variant = variant_from_value(&value, Some(16)).expect("VT_I1");
        let vt = unsafe { variant.Anonymous.Anonymous.vt.0 };
        assert_eq!(vt & 0x0FFF, 16, "expected VT_I1");
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(out, Value::I64(-42));
        clear_variant(&mut variant);
    }

    #[test]
    fn unsupported_vt_returns_error() {
        let err = variant_from_value(&Value::I64(1), Some(13));
        assert!(err.is_err(), "VT_UNKNOWN should be rejected");
        let err = variant_from_value(&Value::I64(1), Some(9));
        assert!(err.is_err(), "VT_DISPATCH should be rejected");
    }
}
