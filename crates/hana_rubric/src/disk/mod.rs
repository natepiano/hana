//! Disk access support for JSONC keymaps.
//!
//! The disk worker creates the user keymap stub after the application first
//! runs. Its relative `$schema` reference lets an editor discover the generated
//! schema without workspace configuration, so completion begins after that
//! first run.

mod companion_files;
mod constants;
mod paths;
mod worker;

pub(crate) use constants::MAX_RETAINED_DIAGNOSTICS;
#[cfg(test)]
pub(crate) use paths::ENVIRONMENT_LOCK;
pub use paths::KeymapPaths;
#[cfg(test)]
pub(crate) use paths::TestDirectory;
#[cfg(test)]
pub(crate) use paths::XdgConfigHome;
#[expect(
    unused_imports,
    reason = "plugin assembly receives disk snapshots after the reload transaction is added"
)]
pub(crate) use worker::DiskSnapshot;
pub(crate) use worker::DiskWorkerChannels;
pub(crate) use worker::DiskWorkerMessage;
pub(crate) use worker::start_disk_worker;

pub(super) fn take_worker_message(
    disk_worker_channels: &DiskWorkerChannels,
) -> Option<DiskWorkerMessage> {
    worker::take_worker_message(disk_worker_channels)
}
