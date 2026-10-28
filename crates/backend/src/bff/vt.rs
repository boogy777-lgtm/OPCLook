//! Platform-neutral VARIANT type names (shared by the COM layer and the UI).

use opclook_contract::model::VarType;

pub fn vt_name(vt: VarType) -> &'static str {
    match vt {
        VarType::EMPTY => "VT_EMPTY",
        VarType::NULL => "VT_NULL",
        VarType::I2 => "VT_I2",
        VarType::I4 => "VT_I4",
        VarType::R4 => "VT_R4",
        VarType::R8 => "VT_R8",
        VarType::CY => "VT_CY",
        VarType::DATE => "VT_DATE",
        VarType::BSTR => "VT_BSTR",
        VarType::DISPATCH => "VT_DISPATCH",
        VarType::ERROR => "VT_ERROR",
        VarType::BOOL => "VT_BOOL",
        VarType::VARIANT => "VT_VARIANT",
        VarType::UNKNOWN => "VT_UNKNOWN",
        VarType::DECIMAL => "VT_DECIMAL",
        VarType::I1 => "VT_I1",
        VarType::UI1 => "VT_UI1",
        VarType::UI2 => "VT_UI2",
        VarType::UI4 => "VT_UI4",
        VarType::I8 => "VT_I8",
        VarType::UI8 => "VT_UI8",
        VarType::INT => "VT_INT",
        VarType::UINT => "VT_UINT",
        _ => "VT_UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vt_names_match_typed_values_and_keep_unknown() {
        assert_eq!(vt_name(VarType::BOOL), "VT_BOOL");
        assert_eq!(vt_name(VarType::I4), "VT_I4");
        assert_eq!(vt_name(VarType::UI8), "VT_UI8");
        assert_eq!(vt_name(VarType(0xffff)), "VT_UNKNOWN");
    }
}
