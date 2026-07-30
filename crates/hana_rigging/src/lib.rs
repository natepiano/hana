//! Device identity and registration policy for providers that report device sets to Bevy.

mod identity;
mod scheme;

pub use identity::DeviceId;
pub use identity::DeviceIdSource;
pub use identity::DeviceKey;
pub use identity::DeviceKind;
pub use scheme::Digest;
pub use scheme::RegisteredSchemes;
pub use scheme::ReportedId;
pub use scheme::ReportedIdError;
pub use scheme::SchemeName;
pub use scheme::SchemeNameError;
pub use scheme::UnregisteredSchemeError;

/// Common identity types used by providers when reporting devices to the kernel.
pub mod prelude {
    pub use crate::DeviceId;
    pub use crate::DeviceIdSource;
    pub use crate::DeviceKey;
    pub use crate::DeviceKind;
    pub use crate::Digest;
    pub use crate::RegisteredSchemes;
    pub use crate::ReportedId;
    pub use crate::ReportedIdError;
    pub use crate::SchemeName;
    pub use crate::SchemeNameError;
    pub use crate::UnregisteredSchemeError;
}
