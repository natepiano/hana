//! Provider-reported serial evidence and identity verdicts produced during reconciliation.
//!
//! These types retain why a provider lacks evidence and record the kernel's resulting identity
//! decision, keeping raw reports separate from the policy that uses a reconciled device key.

use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

use super::identity::DeviceKey;
use super::scheme::ReportedId;

/// Serial evidence from a provider, retaining why a serial value is unavailable instead of using
/// `Option` and losing the policy-relevant distinction.
///
/// A serial-less display has a permanent hardware limitation, while a webcam scanned through a
/// platform API may have a serial that this platform cannot ask for. Providers retain that
/// difference so reconciliation can apply the appropriate recovery policy.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Reflect)]
pub enum ReportedSerial {
    /// The unit supplied this serial, such as the EDID serial from a display panel or the USB
    /// serial from an audio interface.
    ///
    /// `ReportedId` preserves the provider-defined text so it can participate in the registered
    /// identity scheme without the kernel assigning a different meaning to it.
    Provided(ReportedId),
    /// The unit itself exposes no serial, as with a serial-less webcam or panel whose firmware has
    /// no value to report.
    ///
    /// This limitation is permanent for the hardware and is not a provider failure that a retry
    /// can repair.
    NotExposedByUnit,
    /// The provider's platform API cannot ask the unit for a serial, such as a camera API that
    /// exposes capture devices but omits their USB serials.
    ///
    /// Another provider or operating system may be able to report a serial for the same physical
    /// unit, so this differs from `ReportedSerial::NotExposedByUnit`.
    PlatformCannotReport,
}

/// Reconciliation result that states whether a live unit and its durable `DeviceKey` identify the
/// same physical device.
///
/// The kernel computes `DeviceIdentity` from a reconciliation instead of retaining a raw claim so
/// the verdict cannot contradict the key that produced it. This enum is non-exhaustive because
/// future reconciliation evidence can require another verdict; applications must retain a wildcard
/// arm when matching it.
#[non_exhaustive]
#[derive(Clone, Debug, Component, Reflect)]
#[reflect(Component)]
pub enum DeviceIdentity {
    /// A `crate::DeviceIdSource::Reported` key matched one live unit uniquely, as when a display
    /// panel reports the EDID serial a saved layout was written against.
    ///
    /// `Proven` is the only `DeviceIdentity` that can mint an authorization to arm output because
    /// a value reported by the unit establishes which physical device receives that output.
    Proven,
    /// A `crate::DeviceIdSource::Synthesized` key matched one live unit uniquely.
    ///
    /// This restores a saved configuration, such as returning a window to its monitor, but never
    /// drives output. It is the usual result for webcams and serial-less panels whose location
    /// hints can change after a later scan.
    RestoreOnly,
    /// A human assigned this durable address because the unit reports no usable identity.
    ///
    /// The authored patch is authoritative by construction: for example, a lighting fixture with
    /// no serial uses its human-maintained patch as its identity.
    Authored,
    /// A saved key matched no live unit even though a unit of the required kind occupies the same
    /// transport slot, as when a second camera is plugged into the USB port the saved camera used.
    ///
    /// The saved device is neither confirmed present nor known to be gone, so a human must decide
    /// whether the occupying unit replaces it before any automatic action can use it.
    Displaced {
        /// Durable key from saved configuration, retained instead of a process-local `DeviceId`
        /// so the verdict remains meaningful after the process that issued a handle exits.
        saved: DeviceKey,
    },
    /// A human-authored binding matched its saved key while the unit in that binding reports a
    /// different identity.
    ///
    /// This verdict never authorizes output: sending pan data to a misidentified fixture can send
    /// it to a dimmer instead.
    WrongUnit {
        /// Human-authored durable key that matched the saved binding and identifies the
        /// conflicting assignment without relying on a process-local `DeviceId`.
        authored: DeviceKey,
    },
    /// The unit cannot be identified because its identity is absent, duplicated in this scan, or
    /// unavailable through the platform API, as when two identical webcams report the same USB
    /// location value.
    ///
    /// `UnverifiedReason` retains which observation occurred because each one leads to different
    /// recovery policy. `Unverified` prohibits automatic action.
    Unverified(UnverifiedReason),
}

impl DeviceIdentity {
    /// Report whether this verdict identifies the physical unit and nothing more.
    ///
    /// This is not the output-policy predicate: `Devices::armable` also requires presence and a
    /// claim before it can authorize output. `DeviceIdentity::RestoreOnly` is identified so saved
    /// configuration can return to the matching unit, while it remains ineligible to drive output.
    #[must_use]
    pub const fn identified(&self) -> bool {
        matches!(self, Self::Proven | Self::RestoreOnly | Self::Authored)
    }
}

/// Evidence that prevents reconciliation from identifying a live unit, preserved so recovery
/// policy can distinguish hardware limits from platform limits and duplicate reports.
///
/// `UnverifiedReason::NotExposedByUnit` and `UnverifiedReason::PlatformCannotReport` name the same
/// two limits as the `ReportedSerial` variants of those names. `ReportedSerial` records what a
/// provider observed about one unit; these record that reconciliation could not identify the unit
/// as a result.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub enum UnverifiedReason {
    /// The unit itself exposes no serial, as with a serial-less display panel or webcam firmware
    /// that cannot provide a reported identifier on any platform.
    NotExposedByUnit,
    /// More than one live unit reported the same key in one complete provider scan.
    ///
    /// Reconciliation disarms the duplicate key because exact identity requires one unit, not a
    /// plausible choice among two USB devices with indistinguishable reports.
    NotUniqueInScan,
    /// The provider's platform API cannot ask this unit for a serial, even though another platform
    /// may be able to obtain one from the same physical device, as when a macOS camera API reports
    /// a USB location value in place of the unit's serial.
    PlatformCannotReport,
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::DeviceIdentity;
    use super::UnverifiedReason;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::ReportedId;
    use crate::SchemeName;

    #[test]
    fn identified_is_true_for_confirmed_identity_verdicts() {
        for device_identity in [
            DeviceIdentity::Proven,
            DeviceIdentity::RestoreOnly,
            DeviceIdentity::Authored,
        ] {
            assert!(device_identity.identified());
        }
    }

    #[test]
    fn identified_is_false_for_verdicts_without_a_confirmed_identity() -> Result<(), Box<dyn Error>>
    {
        let saved = reported_display_key()?;
        let displaced = DeviceIdentity::Displaced {
            saved: saved.clone(),
        };
        let wrong_unit = DeviceIdentity::WrongUnit { authored: saved };
        let unverified = DeviceIdentity::Unverified(UnverifiedReason::NotUniqueInScan);

        assert!(!displaced.identified());
        assert!(!wrong_unit.identified());
        assert!(!unverified.identified());

        Ok(())
    }

    #[test]
    fn not_unique_in_scan_prevents_identification() {
        let unverified = DeviceIdentity::Unverified(UnverifiedReason::NotUniqueInScan);

        assert!(!unverified.identified());
    }

    fn reported_display_key() -> Result<DeviceKey, Box<dyn Error>> {
        Ok(DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: SchemeName::new("edid-serial")?,
                value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
            },
        })
    }
}
