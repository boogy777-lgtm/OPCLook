//! Pure, transactional conversion of legacy endpoint acquisition profiles.
use crate::bff::model::{
    ACQUISITION_INTERVAL_MAX_MS, ACQUISITION_INTERVAL_MIN_MS, AcquisitionConfig, EndpointSpec,
    Protocol,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Migrated {
    pub ua: usize,
    pub da: usize,
    pub interval_ms: u64,
}

pub(super) fn migrate(
    endpoints: &mut [EndpointSpec],
    seconds: u64,
) -> Result<Option<Migrated>, ()> {
    if !endpoints
        .iter()
        .any(|endpoint| endpoint.acquisition.is_none())
    {
        return Ok(None);
    }
    let interval_ms = seconds
        .checked_mul(1000)
        .filter(|ms| (ACQUISITION_INTERVAL_MIN_MS..=ACQUISITION_INTERVAL_MAX_MS).contains(ms))
        .ok_or(())?;
    let (mut ua, mut da) = (0, 0);
    for endpoint in endpoints {
        if endpoint.acquisition.is_some() {
            continue;
        }
        let mut config = match endpoint.protocol {
            Protocol::OpcUa => {
                ua += 1;
                let mut config = AcquisitionConfig::ua_default();
                // Legacy connections did not automatically read after writes.
                config.read_after_write = false;
                config
            }
            Protocol::OpcDa => {
                da += 1;
                AcquisitionConfig::da_available()
            }
        };
        config.interval_ms = interval_ms;
        endpoint.acquisition = Some(config);
    }
    Ok(Some(Migrated {
        ua,
        da,
        interval_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{PollFreshness, UpdateMode};

    #[test]
    fn mixed_migration_preserves_explicit_and_is_idempotent() {
        let mut explicit = EndpointSpec::ua("opc.tcp://localhost:4842");
        let mut original = AcquisitionConfig::ua_default();
        original.interval_ms = 17;
        explicit.acquisition = Some(original.clone());
        let mut endpoints = [
            EndpointSpec::ua("opc.tcp://localhost:4840"),
            EndpointSpec::da("", "Vendor.Server"),
            explicit,
        ];
        assert_eq!(
            migrate(&mut endpoints, 5),
            Ok(Some(Migrated {
                ua: 1,
                da: 1,
                interval_ms: 5000
            }))
        );
        let ua = endpoints[0].acquisition.as_ref().unwrap();
        assert_eq!(
            (
                ua.update_mode,
                ua.interval_ms,
                ua.poll_freshness,
                ua.read_after_write
            ),
            (
                UpdateMode::Subscription,
                5000,
                PollFreshness::FreshSource,
                false
            )
        );
        let da = endpoints[1].acquisition.as_ref().unwrap();
        assert_eq!(
            (
                da.update_mode,
                da.interval_ms,
                da.poll_freshness,
                da.read_after_write
            ),
            (UpdateMode::Poll, 5000, PollFreshness::CacheAllowed, false)
        );
        assert_eq!(endpoints[2].acquisition.as_ref(), Some(&original));
        assert_eq!(migrate(&mut endpoints, 99), Ok(None));
    }

    #[test]
    fn invalid_interval_leaves_every_profile_unchanged() {
        for seconds in [0, 3601, u64::MAX] {
            let mut endpoints = [
                EndpointSpec::ua("opc.tcp://localhost:4840"),
                EndpointSpec::da("", "Vendor.Server"),
            ];
            assert_eq!(migrate(&mut endpoints, seconds), Err(()));
            assert!(
                endpoints
                    .iter()
                    .all(|endpoint| endpoint.acquisition.is_none())
            );
        }
    }

    #[test]
    fn explicit_profiles_do_not_depend_on_global_interval() {
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        endpoint.acquisition = Some(AcquisitionConfig::ua_default());
        assert_eq!(
            migrate(std::slice::from_mut(&mut endpoint), u64::MAX),
            Ok(None)
        );
        assert_eq!(endpoint.acquisition, Some(AcquisitionConfig::ua_default()));
    }
}
