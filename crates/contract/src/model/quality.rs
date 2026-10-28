#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityKind {
    Good,
    Uncertain,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quality {
    pub code: u32,
    pub kind: QualityKind,
    pub text: std::borrow::Cow<'static, str>,
}

impl Quality {
    pub fn from_da(code: u16) -> Self {
        let code32 = u32::from(code);
        let kind = match code & 0xC0 {
            0xC0 => QualityKind::Good,
            0x40 => QualityKind::Uncertain,
            _ => QualityKind::Bad,
        };
        Self {
            code: code32,
            kind,
            text: std::borrow::Cow::Borrowed(da_quality_text(code)),
        }
    }

    pub fn from_ua(code: u32, text: impl Into<String>) -> Self {
        // OPC UA severity: 00 = Good, 01 = Uncertain, 10/11 = Bad.
        let kind = match code >> 30 {
            0b00 => QualityKind::Good,
            0b01 => QualityKind::Uncertain,
            _ => QualityKind::Bad,
        };
        Self {
            code,
            kind,
            text: std::borrow::Cow::Owned(text.into()),
        }
    }
}

fn da_quality_text(code: u16) -> &'static str {
    match code & 0xFFFC {
        0x0000 => "BAD",
        0x0004 => "BAD - CONFIG_ERROR",
        0x0008 => "BAD - NOT_CONNECTED",
        0x000C => "BAD - DEVICE_FAILURE",
        0x0010 => "BAD - SENSOR_FAILURE",
        0x0014 => "BAD - LAST_KNOWN",
        0x0018 => "BAD - COMM_FAILURE",
        0x001C => "BAD - OUT_OF_SERVICE",
        0x0020 => "BAD - WAITING_FOR_INITIAL_DATA",
        0x0040 => "UNCERTAIN",
        0x0044 => "UNCERTAIN - LAST_USABLE",
        0x0048 => "UNCERTAIN - SENSOR_NOT_ACCURATE",
        0x004C => "UNCERTAIN - EGU_EXCEEDED",
        0x0050 => "UNCERTAIN - SUB_NORMAL",
        0x0054 => "UNCERTAIN - CONVERT_ERROR",
        0x00C0 => "GOOD",
        0x00C8 => "GOOD - CALCULATED",
        0x00D8 => "GOOD - LOCAL_OVERRIDE",
        _ => "UNKNOWN",
    }
}
