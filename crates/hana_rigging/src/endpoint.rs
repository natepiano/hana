use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatResult;

use bevy::prelude::Reflect;
use bevy::reflect::ReflectDeserialize;
use bevy::reflect::ReflectSerialize;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::de::Error as DeserializeError;
use thiserror::Error;

use crate::DeviceKey;

/// Binding target that identifies one provider-defined endpoint on one durable device.
///
/// `EndpointRef` permits several bindings to address different channels of one audio interface,
/// lighting controller, or HID panel. A display uses the same type with its single `Slot`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Serialize, Deserialize)]
pub struct EndpointRef {
    /// Durable device designation that keeps the endpoint associated with one physical unit.
    pub device: DeviceKey,
    /// Provider-defined endpoint name within `device`, such as `ch/7` or `key/3`.
    pub slot:   Slot,
}

/// Provider-defined name for one endpoint within a device.
///
/// `Slot` retains the provider's vocabulary because the kernel cannot know whether `ch/7` is an
/// audio channel, `key/3` is a control surface key, or `nfc` is a reader. Its checked constructor
/// rejects blank and control-character values before endpoint configuration is retained.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Reflect)]
#[reflect(opaque)]
#[reflect(Serialize, Deserialize)]
pub struct Slot(String);

impl Slot {
    /// Create an endpoint name that can be retained without blank or control-character text.
    ///
    /// # Errors
    ///
    /// Returns `SlotError` when `value` cannot identify an endpoint in diagnostics or persisted
    /// configuration.
    pub fn new(value: impl Into<String>) -> Result<Self, SlotError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SlotError::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(SlotError::ContainsControlCharacter);
        }

        Ok(Self(value))
    }

    /// Borrow the provider-defined endpoint name without assigning device-class meaning to it.
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

impl Display for Slot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult { formatter.write_str(&self.0) }
}

impl<'de> Deserialize<'de> for Slot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(<D::Error as DeserializeError>::custom)
    }
}

/// Reason `Slot::new` rejected text before it could name an endpoint.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SlotError {
    /// An empty value cannot select a device endpoint, even for a one-slot display.
    #[error("endpoint slots must not be empty")]
    Empty,
    /// Control characters make endpoint labels ambiguous in configuration and diagnostics.
    #[error("endpoint slots must not contain control characters")]
    ContainsControlCharacter,
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::EndpointRef;
    use super::Slot;
    use super::SlotError;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::ReportedId;
    use crate::SchemeName;

    #[test]
    fn one_slot_display_endpoint_round_trips_through_slot_constructor() -> Result<(), Box<dyn Error>>
    {
        let endpoint_ref = EndpointRef {
            device: display_key()?,
            slot:   Slot::new("0")?,
        };
        let reconstructed_slot = Slot::new(endpoint_ref.slot.as_str())?;

        assert_eq!(reconstructed_slot, endpoint_ref.slot);

        Ok(())
    }

    #[test]
    fn empty_slot_returns_empty_error() {
        assert_eq!(Slot::new(""), Err(SlotError::Empty));
    }

    #[test]
    fn slot_with_control_character_returns_error() {
        assert_eq!(
            Slot::new("input\nchannel"),
            Err(SlotError::ContainsControlCharacter)
        );
    }

    #[test]
    fn empty_slot_fails_ron_deserialization() {
        assert!(ron::from_str::<Slot>("\"\"").is_err());
    }

    #[test]
    fn slot_with_control_character_fails_ron_deserialization() {
        assert!(ron::from_str::<Slot>(r#""input\nchannel""#).is_err());
    }

    fn display_key() -> Result<DeviceKey, Box<dyn Error>> {
        Ok(DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: SchemeName::new("edid-serial")?,
                value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
            },
        })
    }
}
