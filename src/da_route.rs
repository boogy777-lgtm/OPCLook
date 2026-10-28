//! GUI localization for backend route preparation.
use crate::i18n::{Lang, Tr, tr};
pub use opclook_contract::da_route::{DaRouteError, DaRouteStatus};

pub trait LocalizedDaRouteError {
    fn localized(&self, lang: Lang) -> String;
}
pub trait LocalizedDaRouteStatus {
    fn localized(self, lang: Lang) -> &'static str;
}

impl LocalizedDaRouteError for DaRouteError {
    fn localized(&self, lang: Lang) -> String {
        crate::i18n::da_route_error(lang, self)
    }
}

impl LocalizedDaRouteStatus for DaRouteStatus {
    fn localized(self, lang: Lang) -> &'static str {
        match self {
            Self::Idle => tr(lang, Tr::DaRouteHelperIdle),
            Self::HelperStarting => tr(lang, Tr::DaRouteHelperStarting),
            Self::HelperRunning => tr(lang, Tr::DaRouteHelperRunning),
            Self::HelperUnavailable => tr(lang, Tr::DaRouteHelperUnavailable),
        }
    }
}
