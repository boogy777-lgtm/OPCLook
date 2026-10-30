//! Offline N2.1 mapping for the bounded Softing callback ABI. Not wired to
//! production DA; handle-to-session/registration mapping remains N3 work.
use super::filetime::ticks_to_utc;
use opclook_contract::model::{DataValue, Quality, Value};

/// ABI layout shared with native/softing-da/adapter.h. No SDK pointers escape.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// SDK mode 1 is data change; 0 is read completion.
pub struct Envelope {
    pub mode: u32,
    pub transaction: u32,
    pub master_quality: i32,
    pub master_error: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawFact {
    pub client_handle: u32,
    pub hresult: i32,
    pub filetime_low: u32,
    pub filetime_high: u32,
    pub i4: i32,
    pub quality: u16,
    pub vartype: u16,
}

#[derive(Clone, Debug)]
pub enum FactValue {
    Value(DataValue),
    Failed,
    BatchFailed,
    UnsupportedType,
}

#[derive(Clone, Debug)]
pub struct Fact {
    pub mode: u32,
    pub transaction: u32,
    pub master_quality: i32,
    pub master_error: i32,
    pub client_handle: u32,
    pub hresult: i32,
    pub filetime_ticks: u64,
    pub quality: u16,
    pub vartype: u16,
    pub value: FactValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdaptError {
    InvalidHandle,
    InvalidMode,
}

/// Maps a copied fact without manufacturing a successful value for a failed
/// HRESULT or an unimplemented VARIANT type, including type flags.
pub fn adapt(envelope: Envelope, raw: RawFact) -> Result<Fact, AdaptError> {
    if raw.client_handle == 0 {
        return Err(AdaptError::InvalidHandle);
    }
    if envelope.mode > 1 {
        return Err(AdaptError::InvalidMode);
    }
    let ticks = (u64::from(raw.filetime_high) << 32) | u64::from(raw.filetime_low);
    let value = if envelope.master_error < 0 {
        FactValue::BatchFailed
    } else if raw.hresult < 0 {
        FactValue::Failed
    } else if raw.vartype != 3 {
        // VT_I4 only; BYREF/ARRAY flags are not accepted.
        FactValue::UnsupportedType
    } else {
        FactValue::Value(DataValue {
            value: Value::I64(i64::from(raw.i4)),
            quality: Quality::from_da(raw.quality),
            timestamp: ticks_to_utc(ticks),
            reason: None,
        })
    };
    Ok(Fact {
        mode: envelope.mode,
        transaction: envelope.transaction,
        master_quality: envelope.master_quality,
        master_error: envelope.master_error,
        client_handle: raw.client_handle,
        hresult: raw.hresult,
        filetime_ticks: ticks,
        quality: raw.quality,
        vartype: raw.vartype,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> RawFact {
        RawFact {
            client_handle: 9,
            hresult: 7,
            filetime_low: 1,
            filetime_high: 0,
            i4: -42,
            quality: 0xc0,
            vartype: 3,
        }
    }
    fn envelope() -> Envelope {
        Envelope {
            mode: 0,
            transaction: 42,
            master_quality: 0,
            master_error: 0,
        }
    }
    #[test]
    fn abi_layout_matches_native_header() {
        assert_eq!(std::mem::size_of::<Envelope>(), 16);
        assert_eq!(std::mem::offset_of!(Envelope, master_error), 12);
        assert_eq!(std::mem::size_of::<RawFact>(), 24);
        assert_eq!(std::mem::offset_of!(RawFact, client_handle), 0);
        assert_eq!(std::mem::offset_of!(RawFact, hresult), 4);
        assert_eq!(std::mem::offset_of!(RawFact, filetime_low), 8);
        assert_eq!(std::mem::offset_of!(RawFact, filetime_high), 12);
        assert_eq!(std::mem::offset_of!(RawFact, i4), 16);
        assert_eq!(std::mem::offset_of!(RawFact, quality), 20);
        assert_eq!(std::mem::offset_of!(RawFact, vartype), 22);
    }
    #[test]
    fn maps_only_supported_success_and_retains_raw_facts() {
        let f = adapt(envelope(), raw()).unwrap();
        assert_eq!(
            (
                f.client_handle,
                f.hresult,
                f.filetime_ticks,
                f.quality,
                f.vartype
            ),
            (9, 7, 1, 0xc0, 3)
        );
        let FactValue::Value(value) = f.value else {
            panic!("expected value")
        };
        assert_eq!(value.value, Value::I64(-42));
        assert_eq!(value.quality.code, 0xc0);
        assert_eq!(value.timestamp.unwrap().timestamp_subsec_nanos(), 100);
        for vt in [0, 8, 0x2003, 0x4003] {
            assert!(matches!(
                adapt(
                    envelope(),
                    RawFact {
                        vartype: vt,
                        ..raw()
                    }
                )
                .unwrap()
                .value,
                FactValue::UnsupportedType
            ));
        }
        let failed_batch = adapt(
            Envelope {
                master_error: i32::MIN,
                ..envelope()
            },
            raw(),
        )
        .unwrap();
        assert_eq!(failed_batch.master_error, i32::MIN);
        assert_eq!(failed_batch.hresult, 7);
        assert_eq!(failed_batch.quality, 0xc0);
        assert!(matches!(failed_batch.value, FactValue::BatchFailed));
        assert!(matches!(
            adapt(
                envelope(),
                RawFact {
                    hresult: i32::MIN,
                    ..raw()
                }
            )
            .unwrap()
            .value,
            FactValue::Failed
        ));
        assert!(
            adapt(
                envelope(),
                RawFact {
                    client_handle: 0,
                    ..raw()
                }
            )
            .is_err()
        );
        assert_eq!(
            adapt(
                Envelope {
                    mode: 2,
                    ..envelope()
                },
                raw()
            )
            .unwrap_err(),
            AdaptError::InvalidMode
        );
        let zero = adapt(
            envelope(),
            RawFact {
                filetime_low: 0,
                ..raw()
            },
        )
        .unwrap();
        let FactValue::Value(value) = zero.value else {
            panic!("expected value")
        };
        assert!(value.timestamp.is_none());
        let max = adapt(
            envelope(),
            RawFact {
                filetime_low: u32::MAX,
                filetime_high: u32::MAX,
                ..raw()
            },
        )
        .unwrap();
        assert_eq!(max.filetime_ticks, u64::MAX);
        let FactValue::Value(value) = max.value else {
            panic!("expected value")
        };
        assert!(value.timestamp.is_some());
    }
}
