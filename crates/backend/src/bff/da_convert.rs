//! Shared DA write-value conversion; both legacy and SDK adapters use the
//! existing validation rules and typed diagnostics.
use super::model::{BackendMsg, Value};

pub(super) fn da_range(value: &Value, ty: &str) -> BackendMsg {
    BackendMsg::WriteOutOfRange {
        value: value.to_string(),
        ty: ty.to_string(),
    }
}

pub(super) fn convert_int(value: &Value) -> Result<i64, BackendMsg> {
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

pub(super) fn convert_uint(value: &Value) -> Result<u64, BackendMsg> {
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

pub(super) fn convert_float(value: &Value) -> Result<f64, BackendMsg> {
    value.as_f64().ok_or(BackendMsg::DaNotNumberAny)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_numeric_input_retains_typed_diagnostics() {
        assert!(matches!(
            convert_int(&Value::Str("bad".into())),
            Err(BackendMsg::DaNotInteger { .. })
        ));
        assert!(matches!(
            convert_int(&Value::Empty),
            Err(BackendMsg::DaNotIntegerAny)
        ));
        assert!(matches!(
            convert_uint(&Value::Str("-1".into())),
            Err(BackendMsg::DaNotUnsigned { .. })
        ));
        assert!(matches!(
            convert_uint(&Value::I64(-1)),
            Err(BackendMsg::DaNegativeValue { .. })
        ));
        assert!(matches!(
            convert_float(&Value::Empty),
            Err(BackendMsg::DaNotNumberAny)
        ));
    }

    #[test]
    fn wide_integer_and_float_boundaries_are_not_lossy() {
        assert_eq!(convert_int(&Value::I64(i64::MIN)).unwrap(), i64::MIN);
        assert_eq!(convert_uint(&Value::U64(u64::MAX)).unwrap(), u64::MAX);
        assert!(convert_int(&Value::U64(u64::MAX)).is_err());
        assert!(convert_int(&Value::F64(9_223_372_036_854_775_808.0)).is_err());
        assert!(convert_uint(&Value::F64(18_446_744_073_709_551_616.0)).is_err());
        assert!(convert_int(&Value::F64(1.25)).is_err());
        assert!(convert_uint(&Value::F64(f64::NAN)).is_err());
    }
}
