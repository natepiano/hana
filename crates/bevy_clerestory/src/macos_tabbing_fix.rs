//! Disable macOS window tabbing.
//!
//! With the system preference "Prefer tabs when opening documents" at its default
//! ("In Full Screen"), `AppKit` merges a window opened while another of the app's
//! windows is fullscreen into that window as a TAB instead of placing it on its
//! own monitor. Only the front tab is displayed; the vacated display shows an
//! empty black fullscreen Space.
//!
//! The real fix is the app-wide class property
//! `NSWindow.allowsAutomaticWindowTabbing = false`, set in
//! [`disable_automatic_tabbing`] during plugin build — before winit creates any
//! OS window. Per-window `NSWindow.tabbingMode = .disallowed` (the
//! [`disable_tabbing_on_managed`] system) cannot fix automatic tabbing on its
//! own: the tab merge happens at `AppKit` window-creation time, before any ECS
//! system sees the new window. It is kept on `ManagedWindow`s to also block
//! MANUAL tabbing (dragging a window onto another's tab bar, "Merge All
//! Windows").

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowTabbingMode;
use raw_window_handle::HasWindowHandle;
use raw_window_handle::RawWindowHandle;

use super::ManagedWindow;

/// Opt out of macOS automatic window tabbing for the whole app.
///
/// `NSWindow.allowsAutomaticWindowTabbing` is a class property — one
/// process-wide switch, not per-window state. It must be set before any OS
/// window exists; plugin `build()` runs during `add_plugins`, ahead of winit's
/// window creation at event-loop start.
pub(crate) fn disable_automatic_tabbing() {
    let Some(main_thread) = MainThreadMarker::new() else {
        warn!("[macos_tabbing_fix] Not on the main thread; automatic tabbing stays enabled");
        return;
    };
    NSWindow::setAllowsAutomaticWindowTabbing(false, main_thread);
    debug!("[macos_tabbing_fix] Disabled automatic window tabbing app-wide");
}

/// Get the `NSWindow` for a Bevy window entity.
fn get_ns_window(entity: Entity) -> Option<Retained<NSWindow>> {
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let winit_window = winit_windows.get_window(entity)?;
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit_handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: `ns_view` is a valid `NSView` pointer from winit's window handle.
        let ns_view: &NSView = unsafe { appkit_handle.ns_view.cast().as_ref() };
        ns_view.window()
    })
}

/// Disable manual tabbing on newly added `ManagedWindow` entities.
pub(crate) fn disable_tabbing_on_managed(
    new_windows: Query<Entity, Added<ManagedWindow>>,
    _: NonSendMarker,
) {
    for entity in &new_windows {
        let Some(ns_window) = get_ns_window(entity) else {
            debug!("[macos_tabbing_fix] Could not get NSWindow for managed window {entity:?}");
            continue;
        };

        ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);
        debug!("[macos_tabbing_fix] Disabled tabbing on managed window {entity:?}");
    }
}
