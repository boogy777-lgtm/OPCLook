/// Native per-item write status, retained without changing its bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeWriteStatus {
    Ua(u32),
    Da(i32),
    UaService(u32),
    DaService(i32),
}

/// What the server's per-item response says about the write request.
/// Even an accepted result does not prove a physical action occurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteDisposition {
    Accepted,
    AcceptedAsynchronously,
    Uncertain,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOutcome {
    pub disposition: WriteDisposition,
    pub raw: Option<NativeWriteStatus>,
    pub detail: String,
    pub reason: Option<super::BackendMsg>,
}

impl WriteOutcome {
    pub fn from_ua(code: u32, detail: impl Into<String>) -> Self {
        let disposition = match code >> 30 {
            0 if code & 0xffff_0000 == 0x002e_0000 => WriteDisposition::AcceptedAsynchronously,
            0 => WriteDisposition::Accepted,
            1 => WriteDisposition::Uncertain,
            _ => WriteDisposition::Rejected,
        };
        Self {
            disposition,
            raw: Some(NativeWriteStatus::Ua(code)),
            detail: detail.into(),
            reason: None,
        }
    }

    pub fn from_da(code: i32, detail: impl Into<String>) -> Self {
        let disposition = if code < 0 {
            WriteDisposition::Rejected
        } else {
            WriteDisposition::Accepted
        };
        Self {
            disposition,
            raw: Some(NativeWriteStatus::Da(code)),
            detail: detail.into(),
            reason: None,
        }
    }

    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            disposition: WriteDisposition::Unknown,
            raw: None,
            detail: detail.into(),
            reason: None,
        }
    }

    pub fn unknown_with_status(raw: NativeWriteStatus, detail: impl Into<String>) -> Self {
        Self {
            disposition: WriteDisposition::Unknown,
            raw: Some(raw),
            detail: detail.into(),
            reason: None,
        }
    }

    pub fn unknown_because(reason: super::BackendMsg) -> Self {
        Self {
            disposition: WriteDisposition::Unknown,
            raw: None,
            detail: String::new(),
            reason: Some(reason),
        }
    }

    /// Normalize a correlated routing/timeout error after dispatch. Only NoConnection
    /// proves the worker did not invoke the native write.
    pub fn from_dispatch_error(reason: super::BackendMsg) -> Result<Self, super::BackendMsg> {
        if matches!(reason, super::BackendMsg::NoConnection) {
            Err(reason)
        } else {
            Ok(Self::unknown_because(reason))
        }
    }

    pub fn accepted(&self) -> bool {
        matches!(
            self.disposition,
            WriteDisposition::Accepted | WriteDisposition::AcceptedAsynchronously
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_error_preserves_submission_uncertainty() {
        use super::super::BackendMsg;
        assert!(matches!(
            WriteOutcome::from_dispatch_error(BackendMsg::NoConnection),
            Err(BackendMsg::NoConnection)
        ));
        let outcome =
            WriteOutcome::from_dispatch_error(BackendMsg::RequestTimeout { seconds: 5 }).unwrap();
        assert_eq!(outcome.disposition, WriteDisposition::Unknown);
        assert!(matches!(
            outcome.reason,
            Some(BackendMsg::RequestTimeout { seconds: 5 })
        ));
    }

    #[test]
    fn ua_status_classification_preserves_raw_bits() {
        for (code, disposition) in [
            (0, WriteDisposition::Accepted),
            (0x002e_0480, WriteDisposition::AcceptedAsynchronously),
            (0x4000_1234, WriteDisposition::Uncertain),
            (0x8000_0042, WriteDisposition::Rejected),
            (0xc000_0000, WriteDisposition::Rejected),
        ] {
            let outcome = WriteOutcome::from_ua(code, "server");
            assert_eq!(outcome.disposition, disposition);
            assert_eq!(outcome.raw, Some(NativeWriteStatus::Ua(code)));
        }
        assert_eq!(
            WriteOutcome::unknown("missing").disposition,
            WriteDisposition::Unknown
        );
        assert_eq!(
            WriteOutcome::from_da(1, "info").raw,
            Some(NativeWriteStatus::Da(1))
        );
        assert_eq!(
            WriteOutcome::from_da(-1, "error").disposition,
            WriteDisposition::Rejected
        );
    }
}
