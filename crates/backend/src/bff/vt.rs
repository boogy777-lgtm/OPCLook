//! Platform-neutral VARIANT type names (shared by the COM layer and the UI).

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
