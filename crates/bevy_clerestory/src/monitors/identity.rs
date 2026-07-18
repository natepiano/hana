use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use bevy::prelude::*;
use winit::monitor::MonitorHandle;
#[cfg(target_os = "macos")]
use winit::platform::macos::MonitorHandleExtMacOS;
#[cfg(target_os = "windows")]
use winit::platform::windows::MonitorHandleExtWindows;
#[cfg(all(unix, not(target_os = "macos")))]
use winit::platform::x11::MonitorHandleExtX11;

/// Stable, OS-assigned identifier for a display, uniform across platforms.
///
/// Sourced from winit's per-platform native id — `CGDirectDisplayID` on macOS,
/// the `RandR` / `wl_output` id on X11 / Wayland, and a hash of the GDI device
/// name on Windows — normalized to one `u64` so the same value keys the
/// connect/disconnect diff on every platform. Unlike [`crate::MonitorInfo::index`], it
/// survives display rearrangement.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorId(pub u64);

/// Stable 64-bit hash used to normalize a platform key into a [`MonitorId`].
pub(super) fn hash_monitor_key(key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Read winit's per-platform native display id and normalize it to [`MonitorId`].
pub(super) fn native_monitor_id(handle: &MonitorHandle) -> MonitorId {
    #[cfg(target_os = "macos")]
    let raw = { u64::from(handle.native_id()) };
    #[cfg(target_os = "windows")]
    let raw = { hash_monitor_key(handle.native_id()) };
    #[cfg(all(unix, not(target_os = "macos")))]
    let raw = {
        // X11 and Wayland both expose `native_id() -> u32`; winit dispatches to
        // the active backend, so importing either trait resolves the same value.
        u64::from(handle.native_id())
    };
    MonitorId(raw)
}
