//! Protocol workers and platform networking.
pub mod bff;
pub mod constants;
pub mod net;
pub mod opc;
#[cfg(test)]
pub use opclook_locale as i18n;

pub mod da_route;
pub mod route_resolver;

pub mod route_service;
#[cfg(windows)]
mod winutil;

/// Current OPCLook implementation abilities, not negotiated server capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcquisitionCapabilities {
    pub subscription: bool,
    pub cache_allowed: bool,
    pub fresh_source: bool,
    pub read_after_write: bool,
}
pub fn acquisition_capabilities(
    protocol: opclook_contract::model::Protocol,
) -> AcquisitionCapabilities {
    use opclook_contract::model::Protocol;
    match protocol {
        Protocol::OpcUa => AcquisitionCapabilities {
            subscription: true,
            cache_allowed: true,
            fresh_source: true,
            read_after_write: true,
        },
        // Preview exposes SDK advise; the default legacy adapter remains Cache-only.
        Protocol::OpcDa => AcquisitionCapabilities {
            subscription: cfg!(all(windows, feature = "native-da-preview")),
            cache_allowed: true,
            fresh_source: cfg!(all(windows, feature = "native-da-preview")),
            read_after_write: cfg!(all(windows, feature = "native-da-preview")),
        },
    }
}
/// Defaults for a new GUI connection; migration and CLI compatibility use
/// their explicit baseline constructors instead.
pub fn recommended_acquisition(
    protocol: opclook_contract::model::Protocol,
) -> opclook_contract::model::AcquisitionConfig {
    use opclook_contract::model::{AcquisitionConfig, PollFreshness, Protocol};
    match protocol {
        Protocol::OpcUa => AcquisitionConfig::ua_default(),
        Protocol::OpcDa => {
            let mut config = AcquisitionConfig::da_available();
            if acquisition_capabilities(protocol).fresh_source {
                config.poll_freshness = PollFreshness::FreshSource;
                config.read_after_write = true;
            }
            config
        }
    }
}

pub fn supports_fresh_source(protocol: opclook_contract::model::Protocol) -> bool {
    acquisition_capabilities(protocol).fresh_source
}

/// Capabilities of the currently shipped adapters; not server negotiation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquisitionError {
    Interval,
    StreamUnsupported,
    FreshnessUnsupported,
    ReadbackUnsupported,
    DaAsyncUnsupported,
}
pub fn validate_acquisition(
    endpoint: &opclook_contract::model::EndpointSpec,
) -> Result<(), AcquisitionError> {
    use opclook_contract::model::{DaReadMethod, PollFreshness, UpdateMode};
    let Some(config) = &endpoint.acquisition else {
        return Ok(());
    };
    if !(opclook_contract::model::ACQUISITION_INTERVAL_MIN_MS
        ..=opclook_contract::model::ACQUISITION_INTERVAL_MAX_MS)
        .contains(&config.interval_ms)
    {
        return Err(AcquisitionError::Interval);
    }
    if config.da_read_method == DaReadMethod::Async {
        return Err(AcquisitionError::DaAsyncUnsupported);
    }
    let capabilities = acquisition_capabilities(endpoint.protocol);
    if !capabilities.subscription && config.update_mode == UpdateMode::Subscription {
        return Err(AcquisitionError::StreamUnsupported);
    }
    if config.read_after_write && !capabilities.read_after_write {
        return Err(AcquisitionError::ReadbackUnsupported);
    }
    if match config.poll_freshness {
        PollFreshness::CacheAllowed => !capabilities.cache_allowed,
        PollFreshness::FreshSource => !capabilities.fresh_source,
    } {
        return Err(AcquisitionError::FreshnessUnsupported);
    }

    Ok(())
}

#[cfg(test)]
mod acquisition_tests {
    use super::*;
    use opclook_contract::model::{
        AcquisitionConfig, EndpointSpec, PollFreshness, Protocol, UpdateMode,
    };
    #[test]
    fn recommended_da_is_capability_gated_without_changing_legacy_profile() {
        let legacy = AcquisitionConfig::da_available();
        let recommended = recommended_acquisition(Protocol::OpcDa);
        assert_eq!(legacy.poll_freshness, PollFreshness::CacheAllowed);
        assert!(!legacy.read_after_write);
        let capable = acquisition_capabilities(Protocol::OpcDa).fresh_source;
        assert_eq!(recommended.read_after_write, capable);
        assert_eq!(
            recommended.poll_freshness,
            if capable {
                PollFreshness::FreshSource
            } else {
                PollFreshness::CacheAllowed
            }
        );
        let mut restored = EndpointSpec::da("", "test");
        restored.acquisition = Some(legacy.clone());
        assert_eq!(validate_acquisition(&restored), Ok(()));
        assert_eq!(restored.acquisition, Some(legacy));
    }
    #[test]
    fn async_da_method_is_not_advertised_before_live_proof() {
        let mut endpoint = EndpointSpec::da("", "test");
        let mut config = AcquisitionConfig::da_available();
        config.da_read_method = opclook_contract::model::DaReadMethod::Async;
        endpoint.acquisition = Some(config);
        assert_eq!(
            validate_acquisition(&endpoint),
            Err(AcquisitionError::DaAsyncUnsupported)
        );
    }

    #[test]
    fn validates_known_adapter_capabilities_and_interval_boundaries() {
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let mut config = AcquisitionConfig::ua_default();
        for interval in [1, 3_600_000] {
            config.interval_ms = interval;
            endpoint.acquisition = Some(config.clone());
            assert_eq!(validate_acquisition(&endpoint), Ok(()));
        }
        for interval in [0, 3_600_001] {
            config.interval_ms = interval;
            endpoint.acquisition = Some(config.clone());
            assert_eq!(
                validate_acquisition(&endpoint),
                Err(AcquisitionError::Interval)
            );
        }
        config.interval_ms = 1000;
        config.poll_freshness = PollFreshness::CacheAllowed;
        endpoint.acquisition = Some(config);
        assert_eq!(validate_acquisition(&endpoint), Ok(()));
        let mut da = EndpointSpec::da("", "test");
        let mut config = AcquisitionConfig::da_available();
        da.acquisition = Some(config.clone());
        assert_eq!(validate_acquisition(&da), Ok(()));
        config.update_mode = UpdateMode::Subscription;
        da.acquisition = Some(config.clone());
        assert_eq!(
            validate_acquisition(&da),
            if cfg!(all(windows, feature = "native-da-preview")) {
                Ok(())
            } else {
                Err(AcquisitionError::StreamUnsupported)
            }
        );
        config.update_mode = UpdateMode::Poll;
        config.read_after_write = true;
        da.acquisition = Some(config);
        assert_eq!(
            validate_acquisition(&da),
            if cfg!(all(windows, feature = "native-da-preview")) {
                Ok(())
            } else {
                Err(AcquisitionError::ReadbackUnsupported)
            }
        );
    }
}
