#![deny(clippy::undocumented_unsafe_blocks)]

use crate::bff::model::{BackendMsg, Value};
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

pub use super::vt::vt_name;

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
    let dt = base.checked_add_signed(chrono::Duration::try_days(whole)?)?;
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
    // SAFETY: the caller supplies an initialized VARIANT containing a tag.
    let vt = unsafe { variant.Anonymous.Anonymous.vt };
    let raw = vt.0;
    let is_array = raw & 0x2000 != 0;
    let base = raw & 0x0FFF;

    if is_array {
        // SAFETY: the array flag selects the SAFEARRAY member of this VARIANT.
        return unsafe { array_to_value(variant, base) };
    }

    // SAFETY: the scalar tag selects the corresponding initialized member.
    unsafe { scalar_to_value(variant, base) }
}

unsafe fn scalar_to_value(variant: &VARIANT, base: u16) -> Value {
    // SAFETY: the caller supplies a valid VARIANT and base is its active
    // scalar tag; any referenced nested value stays live for this conversion.
    unsafe {
        let body = &variant.Anonymous.Anonymous.Anonymous;
        match base {
            0 => Value::Empty,
            1 => Value::Other("NULL".to_string()),
            2 => Value::I64(i64::from(body.iVal)),
            3 => Value::I64(i64::from(body.lVal)),
            4 => Value::F64(f64::from(body.fltVal)),
            5 => Value::F64(body.dblVal),
            6 => Value::F64(body.cyVal.int64 as f64 / 10_000.0),
            7 => match ole_date_to_utc(body.date) {
                Some(dt) => Value::DateTime(dt),
                None => Value::Other("Invalid DATE".to_string()),
            },
            8 => {
                let bstr = &body.bstrVal;
                Value::Str(bstr.to_string())
            }
            10 => Value::Other(format!("HRESULT 0x{:08X}", body.scode as u32)),
            11 => Value::Bool(body.boolVal.0 != 0),
            12 => {
                let nested = body.pvarVal;
                if nested.is_null() {
                    Value::Empty
                } else {
                    variant_to_value(&*nested)
                }
            }
            16 => Value::I64(i64::from(body.bVal as i8)),
            17 => Value::U64(u64::from(body.bVal)),
            18 => Value::U64(u64::from(body.uiVal)),
            19 => Value::U64(u64::from(body.ulVal)),
            20 => Value::I64(*(&raw const *body).cast::<i64>()),
            21 => Value::U64(*(&raw const *body).cast::<u64>()),
            22 => Value::I64(i64::from(body.intVal)),
            23 => Value::U64(u64::from(body.uintVal)),
            24 | 25 => Value::Empty,
            64 => {
                let ft = *(&raw const *body).cast::<FILETIME>();
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
}

/// Limit in i64 before converting to usize, including on 32-bit Windows.
fn bounded_array_len(lower: i32, upper: i32) -> usize {
    (i64::from(upper) - i64::from(lower) + 1).clamp(0, MAX_ARRAY_ITEMS as i64) as usize
}

unsafe fn array_to_value(variant: &VARIANT, base: u16) -> Value {
    // SAFETY: the caller supplies a valid array VARIANT.
    let parray = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
    if parray.is_null() {
        return Value::Array(Vec::new());
    }
    // SAFETY: the non-null SAFEARRAY is live for this conversion.
    if unsafe { SafeArrayGetDim(parray) } != 1 {
        return Value::Other("Multi-dimensional array".to_string());
    }
    // SAFETY: the live array has dimension 1 as checked above.
    let lb = unsafe { SafeArrayGetLBound(parray, 1) }.unwrap_or(0);
    // SAFETY: the live array has dimension 1 as checked above.
    let ub = unsafe { SafeArrayGetUBound(parray, 1) }.unwrap_or(-1);
    let count = bounded_array_len(lb, ub);
    if count == 0 {
        return Value::Array(Vec::new());
    }
    let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: parray is live and data is valid writable output storage.
    if unsafe { SafeArrayAccessData(parray, &raw mut data) }.is_err() {
        return Value::Array(Vec::new());
    }

    let mut out = Vec::with_capacity(count);
    if base == 12 {
        let len = count;
        // SAFETY: the accessed VT_VARIANT array contains at least len initialized elements.
        let vars = unsafe { std::slice::from_raw_parts(data.cast::<VARIANT>(), len) };
        for v in vars {
            // SAFETY: each borrowed array element is an initialized VARIANT.
            out.push(unsafe { variant_to_value(v) });
        }
    } else {
        // SAFETY: parray remains live and accessed.
        let elem_size = unsafe { SafeArrayGetElemsize(parray) } as usize;
        let len = count;
        for i in 0..len {
            let mut tmp = VARIANT::default();
            // SAFETY: i is within the accessed array; the copy is capped to
            // the temporary VARIANT union's storage, with matching element type.
            unsafe {
                set_vt(&mut tmp, base);
                let src = (data as *const u8).add(i * elem_size);
                let dst = variant_inner(&mut tmp).cast::<u8>();
                std::ptr::copy_nonoverlapping(src, dst, elem_size.min(16));
            }
            // SAFETY: tmp borrows the copied scalar matching base until conversion returns.
            out.push(unsafe { scalar_to_value(&tmp, base) });
        }
    }
    // SAFETY: this balances the successful access above exactly once.
    unsafe {
        let _ = SafeArrayUnaccessData(parray);
    }
    Value::Array(out)
}

fn set_vt(variant: &mut VARIANT, vt: u16) {
    // SAFETY: variant is writable and vt is its tag field.
    unsafe {
        *(&raw mut (*variant.Anonymous.Anonymous).vt) = VARENUM(vt);
    }
}

fn variant_inner(variant: &mut VARIANT) -> *mut VARIANT_0_0_0 {
    // SAFETY: this forms a pointer into the live writable VARIANT union.
    unsafe { &raw mut (*variant.Anonymous.Anonymous).Anonymous }
}

/// Builds a COM VARIANT for a write operation. `hint` is an OPC DA VT_* code.
///
/// Arrays are supported: when `value` is a [`Value::Array`] or the hint carries
/// the `VT_ARRAY` (0x2000) bit, a one-dimensional SAFEARRAY is built whose
/// element type is the scalar VT. An array value with a scalar hint is turned
/// into an array of that element type.
pub fn variant_from_value(value: &Value, hint: Option<u16>) -> Result<VARIANT, BackendMsg> {
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
    // SAFETY: inner points into the initialized, writable variant tagged vt.
    unsafe { write_scalar(value, vt, inner)? };
    Ok(variant)
}

/// Writes one scalar (non-array) value into the VARIANT named union.
unsafe fn write_scalar(
    value: &Value,
    vt: u16,
    inner: *mut VARIANT_0_0_0,
) -> Result<(), BackendMsg> {
    // SAFETY: the caller supplies writable VARIANT union storage; each arm
    // writes the member selected by vt and transfers any allocated BSTR to it.
    unsafe {
        match vt {
            0 => {}
            2 => {
                let v = i16::try_from(convert_int(value)?).map_err(|_| da_range(value, "I2"))?;
                (*inner).iVal = v;
            }
            3 => {
                let v = i32::try_from(convert_int(value)?).map_err(|_| da_range(value, "I4"))?;
                (*inner).lVal = v;
            }
            4 => {
                let source = convert_float(value)?;
                if source.is_finite() && source.abs() > f64::from(f32::MAX) {
                    return Err(da_range(value, "R4"));
                }
                let v = source as f32;
                (*inner).fltVal = v;
            }
            5 => {
                let v = convert_float(value)?;
                (*inner).dblVal = v;
            }
            6 => {
                let v = (convert_float(value)? * 10_000.0).round();
                if !v.is_finite()
                    || v < -9_223_372_036_854_775_808.0
                    || v >= 9_223_372_036_854_775_808.0
                {
                    return Err(da_range(value, "CY"));
                }
                (*inner).cyVal.int64 = v as i64;
            }
            7 => {
                let dt = match value {
                    Value::DateTime(dt) => *dt,
                    _ => return Err(BackendMsg::DaDateRequired),
                };
                let ole = utc_to_ole_date(dt).ok_or(BackendMsg::DaBadDate)?;
                (*inner).date = ole;
            }
            8 => {
                let s = match value {
                    Value::Str(s) => s.clone(),
                    other => other.to_string(),
                };
                std::ptr::write(
                    &raw mut (*inner).bstrVal,
                    std::mem::ManuallyDrop::new(BSTR::from(s)),
                );
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
                (*inner).boolVal = VARIANT_BOOL(if b { -1 } else { 0 });
            }
            16 => {
                (*inner).bVal =
                    i8::try_from(convert_int(value)?).map_err(|_| da_range(value, "I1"))? as u8
            }
            17 => {
                (*inner).bVal =
                    u8::try_from(convert_uint(value)?).map_err(|_| da_range(value, "UI1"))?
            }
            18 => {
                (*inner).uiVal =
                    u16::try_from(convert_uint(value)?).map_err(|_| da_range(value, "UI2"))?
            }
            19 => {
                (*inner).ulVal =
                    u32::try_from(convert_uint(value)?).map_err(|_| da_range(value, "UI4"))?
            }
            20 => std::ptr::write(inner.cast::<i64>(), convert_int(value)?),
            21 => {
                std::ptr::write(inner.cast::<u64>(), convert_uint(value)?);
            }
            22 => {
                (*inner).intVal =
                    i32::try_from(convert_int(value)?).map_err(|_| da_range(value, "INT"))?
            }
            23 => {
                (*inner).uintVal =
                    u32::try_from(convert_uint(value)?).map_err(|_| da_range(value, "UINT"))?
            }
            other => return Err(BackendMsg::DaVtUnsupported { vt: other }),
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Default)]
struct ArrayDestroyObservation {
    calls: usize,
    unlocked: bool,
    first_is_bstr: bool,
    destroyed: bool,
}

#[cfg(test)]
thread_local! {
    static ARRAY_DESTROY_OBSERVATION: std::cell::Cell<Option<ArrayDestroyObservation>> = const { std::cell::Cell::new(None) };
}

/// Owns a newly created SAFEARRAY until ownership is transferred to a VARIANT.
struct SafeArrayBuilder {
    ptr: *mut windows::Win32::System::Com::SAFEARRAY,
    locked: bool,
    accessed: bool,
    #[cfg(test)]
    element_vt: u16,
}

impl SafeArrayBuilder {
    fn finish(mut self) -> *mut windows::Win32::System::Com::SAFEARRAY {
        self.release_access();
        let ptr = self.ptr;
        self.ptr = std::ptr::null_mut();
        ptr
    }

    fn release_access(&mut self) {
        // SAFETY: flags describe successful calls on this live array and each
        // successful access/lock is balanced once before destruction or transfer.
        unsafe {
            if self.accessed {
                let _ = SafeArrayUnaccessData(self.ptr);
                self.accessed = false;
            }
            if self.locked {
                let _ = SafeArrayUnlock(self.ptr);
                self.locked = false;
            }
        }
    }
}

impl Drop for SafeArrayBuilder {
    fn drop(&mut self) {
        self.release_access();
        if !self.ptr.is_null() {
            #[cfg(test)]
            let observed = if self.element_vt == 12 {
                observe_variant_array_before_destroy(self.ptr)
            } else {
                None
            };
            // SAFETY: this guard owns the array and has balanced both locks.
            let destroyed = unsafe { SafeArrayDestroy(self.ptr) };
            #[cfg(test)]
            if let Some((unlocked, first_is_bstr)) = observed {
                ARRAY_DESTROY_OBSERVATION.with(|slot| {
                    if let Some(mut observation) = slot.get() {
                        observation.calls += 1;
                        observation.unlocked = unlocked;
                        observation.first_is_bstr = first_is_bstr;
                        observation.destroyed = destroyed.is_ok();
                        slot.set(Some(observation));
                    }
                });
            }
            #[cfg(not(test))]
            let _ = destroyed;
        }
    }
}

#[cfg(test)]
fn observe_variant_array_before_destroy(
    ptr: *mut windows::Win32::System::Com::SAFEARRAY,
) -> Option<(bool, bool)> {
    if !ARRAY_DESTROY_OBSERVATION.with(|slot| slot.get().is_some()) {
        return None;
    }
    // SAFETY: the live owned array was created as a one-dimensional VARIANT
    // vector; reading its descriptor and first slot is valid until destruction.
    unsafe {
        let unlocked = (*ptr).cLocks == 0;
        let mut data = std::ptr::null_mut();
        let mut first_is_bstr = false;
        if (*ptr).rgsabound[0].cElements > 0 && SafeArrayAccessData(ptr, &raw mut data).is_ok() {
            if !data.is_null() {
                let first = &*data.cast::<VARIANT>();
                first_is_bstr = first.Anonymous.Anonymous.vt.0 == VT_BSTR.0;
            }
            let _ = SafeArrayUnaccessData(ptr);
        }
        Some((unlocked, first_is_bstr))
    }
}

/// Builds a `VT_x | VT_ARRAY` VARIANT holding a single-dimension SAFEARRAY.
fn build_array_variant(
    value: &Value,
    element_vt: u16,
    outer_vt: u16,
) -> Result<VARIANT, BackendMsg> {
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };

    // SAFETY: the type and length describe the requested vector allocation.
    let psa = unsafe {
        SafeArrayCreateVector(
            windows::Win32::System::Variant::VARENUM(element_vt),
            0,
            items.len() as u32,
        )
    };
    if psa.is_null() {
        return Err(BackendMsg::DaArrayCreateFailed { vt: element_vt });
    }

    let mut array = SafeArrayBuilder {
        ptr: psa,
        locked: false,
        accessed: false,
        #[cfg(test)]
        element_vt,
    };
    let element_size: usize = match element_vt {
        16 | 17 => 1,
        2 | 11 | 18 => 2,
        3 | 4 | 19 | 22 | 23 => 4,
        5 | 6 | 7 | 20 | 21 => 8,
        8 => std::mem::size_of::<windows_core::BSTR>(),
        12 => std::mem::size_of::<VARIANT>(),
        other => {
            return Err(BackendMsg::DaArrayVtUnsupported { vt: other });
        }
    };

    // Lock the array and write each element in place.
    // SAFETY: psa is a newly created live array owned by the builder.
    let locked = unsafe { SafeArrayLock(psa) }.is_ok();
    if !locked {
        return Err(BackendMsg::DaArrayLockFailed { vt: element_vt });
    }
    array.locked = true;
    let mut access: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: psa is live and access is a valid writable output pointer.
    if unsafe { SafeArrayAccessData(psa, &raw mut access) }.is_err() {
        return Err(BackendMsg::DaArrayAccessFailed);
    }
    array.accessed = true;
    let data = access as *mut u8;

    let mut build_result: Result<(), BackendMsg> = Ok(());
    for (i, item) in items.iter().enumerate() {
        match element_vt {
            12 => {
                // VT_VARIANT array: build a nested VARIANT for each element.
                let nested = match variant_from_value(item, None) {
                    Ok(nested) => nested,
                    Err(error) => {
                        build_result = Err(error);
                        break;
                    }
                };
                // SAFETY: i indexes a slot in the accessed VT_VARIANT vector.
                let dst = unsafe { (data.cast::<VARIANT>()).add(i) };
                // SAFETY: dst is an initialized empty slot whose ownership is transferred.
                unsafe { std::ptr::write(dst, nested) };
            }
            _ => {
                let mut tmp = VARIANT::default();
                set_vt(&mut tmp, element_vt);
                if let Err(e) = (|| {
                    // SAFETY: tmp is writable and tagged with element_vt.
                    unsafe { write_scalar(item, element_vt, variant_inner(&mut tmp)) }?;
                    Ok::<(), BackendMsg>(())
                })() {
                    build_result = Err(e);
                    break;
                }
                // Copy the element value from the temporary VARIANT into the array data.
                let src = variant_inner(&mut tmp).cast::<u8>();
                // SAFETY: i is within the array and element_size matches its type.
                let dst = unsafe { data.add(i * element_size) };
                // SAFETY: the temporary union and array slot are disjoint and large enough.
                unsafe { std::ptr::copy_nonoverlapping(src, dst, element_size) };
                // The element (e.g. a BSTR) is now owned by the safe array; forget the
                // temporary VARIANT so the element is not released later.
                std::mem::forget(tmp);
            }
        }
    }

    build_result?;
    let psa = array.finish();
    let mut variant = VARIANT::default();
    set_vt(&mut variant, outer_vt);
    let inner = variant_inner(&mut variant);
    // SAFETY: the outer VARIANT is tagged as an array and takes ownership of psa.
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
        Value::Struct { .. } => VT_EMPTY.0,
    }
}

fn da_range(value: &Value, ty: &str) -> BackendMsg {
    BackendMsg::WriteOutOfRange {
        value: value.to_string(),
        ty: ty.to_string(),
    }
}

fn convert_int(value: &Value) -> Result<i64, BackendMsg> {
    match value {
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::I64(v) => Ok(*v),
        Value::U64(v) => i64::try_from(*v).map_err(|_| da_range(value, "I8")),
        Value::F64(v)
            if v.is_finite()
                && v.fract() == 0.0
                && *v >= -9_223_372_036_854_775_808.0
                && *v < 9_223_372_036_854_775_808.0 =>
        {
            Ok(*v as i64)
        }
        Value::Str(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|_| BackendMsg::DaNotInteger { value: s.clone() }),
        Value::DateTime(dt) => Ok(dt.timestamp()),
        _ => Err(BackendMsg::DaNotIntegerAny),
    }
}

fn convert_uint(value: &Value) -> Result<u64, BackendMsg> {
    match value {
        Value::Bool(b) => Ok(u64::from(*b)),
        Value::I64(v) => u64::try_from(*v).map_err(|_| BackendMsg::DaNegativeValue {
            value: v.to_string(),
        }),
        Value::U64(v) => Ok(*v),
        Value::F64(v)
            if v.is_finite()
                && v.fract() == 0.0
                && *v >= 0.0
                && *v < 18_446_744_073_709_551_616.0 =>
        {
            Ok(*v as u64)
        }
        Value::Str(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|_| BackendMsg::DaNotUnsigned { value: s.clone() }),
        _ => Err(BackendMsg::DaNotUnsignedAny),
    }
}

fn convert_float(value: &Value) -> Result<f64, BackendMsg> {
    value.as_f64().ok_or(BackendMsg::DaNotNumberAny)
}

pub fn clear_variant(variant: &mut VARIANT) {
    // SAFETY: the caller supplies a valid initialized VARIANT to clear in place.
    unsafe {
        let _ = windows::Win32::System::Variant::VariantClear(variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn integer_writes_do_not_truncate_or_wrap() {
        for (value, vt) in [
            (Value::F64(3.7), 20),
            (Value::I64(70000), 2),
            (Value::I64(300), 16),
            (Value::U64(u64::MAX), 20),
        ] {
            assert!(variant_from_value(&value, Some(vt)).is_err());
        }
    }

    #[test]
    fn hostile_ole_dates_do_not_panic() {
        assert!(ole_date_to_utc(f64::MAX).is_none());
        assert!(ole_date_to_utc(-f64::MAX).is_none());
    }

    #[test]
    fn bool_and_float_arrays_roundtrip() {
        for (vt, input) in [
            (
                11,
                Value::Array(vec![Value::Bool(true), Value::Bool(false)]),
            ),
            (4, Value::Array(vec![Value::F64(1.5), Value::F64(2.5)])),
        ] {
            let mut variant = build_array_variant(&input, vt, vt | 0x2000).unwrap();
            // SAFETY: the test just built this initialized array VARIANT.
            let output = unsafe { variant_to_value(&variant) };
            assert_eq!(format!("{output:?}"), format!("{input:?}"));
            clear_variant(&mut variant);
        }
    }

    #[test]
    fn variant_array_nested_failure_after_an_owned_string_returns_error() {
        let value = Value::Array(vec![
            Value::Str("allocated first".into()),
            Value::Array(vec![Value::I64(1), Value::Str("not an integer".into())]),
        ]);
        ARRAY_DESTROY_OBSERVATION.with(|slot| slot.set(Some(ArrayDestroyObservation::default())));
        assert!(build_array_variant(&value, 12, 12 | 0x2000).is_err());
        let observation = ARRAY_DESTROY_OBSERVATION.with(|slot| slot.take()).unwrap();
        assert_eq!(observation.calls, 1);
        assert!(observation.unlocked);
        assert!(observation.first_is_bstr);
        assert!(observation.destroyed);
    }

    #[test]
    fn full_signed_bound_range_is_capped_before_usize_conversion() {
        assert_eq!(bounded_array_len(i32::MIN, i32::MAX), MAX_ARRAY_ITEMS);
        assert_eq!(bounded_array_len(i32::MAX, i32::MIN), 0);
        assert_eq!(bounded_array_len(i32::MIN, i32::MIN + 1), 2);
    }

    #[test]
    fn array_values_support_extreme_lower_bounds() {
        for lower in [i32::MIN, i32::MAX - 1] {
            // SAFETY: a two-element I4 array is created and owned by this test.
            let array = unsafe { SafeArrayCreateVector(VARENUM(3), lower, 2) };
            assert!(!array.is_null());
            let mut data = std::ptr::null_mut();
            // SAFETY: array is live and data is a valid output location.
            unsafe { SafeArrayAccessData(array, &raw mut data).unwrap() };
            // SAFETY: the accessed array has exactly two initialized I4 slots.
            unsafe {
                data.cast::<i32>().write(7);
                data.cast::<i32>().add(1).write(-9);
                SafeArrayUnaccessData(array).unwrap();
            }
            let mut variant = VARIANT::default();
            set_vt(&mut variant, 3 | 0x2000);
            // SAFETY: the variant is an array variant and takes ownership of array.
            unsafe { (*variant_inner(&mut variant)).parray = array };
            // SAFETY: the variant holds the initialized array until cleared below.
            let value = unsafe { variant_to_value(&variant) };
            assert_eq!(value, Value::Array(vec![Value::I64(7), Value::I64(-9)]));
            clear_variant(&mut variant);
        }
    }

    /// Round-trips a value through a VARIANT of the given VT and asserts value equality.
    fn roundtrip(vt: u16, value: &Value) {
        let mut variant = variant_from_value(value, Some(vt)).expect("build variant");
        // SAFETY: the test just built this initialized VARIANT.
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
        // SAFETY: v is the initialized scalar VARIANT just built above.
        let converted = unsafe { variant_to_value(&v) };
        assert_eq!(converted, Value::U64(200));
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
        // SAFETY: the test just built this initialized VARIANT.
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(&out, &value);
        clear_variant(&mut variant);
    }

    #[test]
    fn roundtrip_variant_array_without_hint_infers_element_type() {
        let value = Value::Array(vec![Value::I64(7), Value::I64(8)]);
        let mut variant = variant_from_value(&value, None).expect("inferred array");
        // SAFETY: the test just built this initialized VARIANT.
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(&out, &value);
        clear_variant(&mut variant);
    }

    #[test]
    fn roundtrip_scalar_to_array_of_one_via_array_bit() {
        // An array-typed hint turns a scalar value into a one-element array.
        let value = Value::Bool(true);
        let mut variant = variant_from_value(&value, Some(11 | 0x2000)).expect("array bool");
        // SAFETY: the test just built this initialized VARIANT.
        let out = unsafe { variant_to_value(&variant) };
        assert_eq!(out, Value::Array(vec![Value::Bool(true)]));
        clear_variant(&mut variant);
    }

    #[test]
    fn i1_negative_roundtrip_preserves_sign() {
        let value = Value::I64(-42);
        let mut variant = variant_from_value(&value, Some(16)).expect("VT_I1");
        // SAFETY: the initialized VARIANT contains a valid type tag.
        let vt = unsafe { variant.Anonymous.Anonymous.vt.0 };
        assert_eq!(vt & 0x0FFF, 16, "expected VT_I1");
        // SAFETY: the test just built this initialized VARIANT.
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
