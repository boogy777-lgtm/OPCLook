//! GUI localization for backend route preparation.
use crate::i18n::{Lang, Tr, fmt, tr};
pub use opclook_backend::da_route::*;

pub trait LocalizedDaRouteError {
    fn localized(&self, lang: Lang) -> String;
}
pub trait LocalizedDaRouteStatus {
    fn localized(self, lang: Lang) -> &'static str;
}

impl LocalizedDaRouteError for DaRouteError {
    fn localized(&self, lang: Lang) -> String {
        let detail = match self {
            Self::NoAdapter => tr(lang, Tr::DaRouteNoAdapter).to_string(),
            Self::AdapterUnavailable => tr(lang, Tr::SelectAdapterRequired).to_string(),
            Self::Resolve(host) => fmt(lang, Tr::DaRouteResolve, &[host]),
            Self::InvalidTarget(ip) => fmt(lang, Tr::DaRouteResolve, &[&ip.to_string()]),
            Self::NoGateway(ip) => fmt(lang, Tr::DaRouteNoGateway, &[&ip.to_string()]),
            Self::Conflict(ip) => fmt(lang, Tr::DaRouteConflict, &[&ip.to_string()]),
            Self::HelperStarting => tr(lang, Tr::DaRouteHelperStarting).to_string(),
            Self::HelperUnavailable(_) | Self::UacDeclined => {
                tr(lang, Tr::DaRouteHelperUnavailable).to_string()
            }
            Self::Add(ip, detail) => fmt(lang, Tr::DaRouteAdd, &[&ip.to_string(), detail]),
            Self::Verify(ip) => fmt(lang, Tr::DaRouteVerify, &[&ip.to_string()]),
        };
        fmt(lang, Tr::DaRouteError, &[&detail])
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
