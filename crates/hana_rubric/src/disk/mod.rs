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

pub use paths::KeymapPaths;
#[expect(
    unused_imports,
    reason = "plugin assembly receives disk snapshots after the reload transaction is added"
)]
pub(crate) use worker::DiskSnapshot;
#[expect(
    unused_imports,
    reason = "plugin assembly stores disk worker channels after the reload transaction is added"
)]
pub(crate) use worker::DiskWorkerChannels;
#[expect(
    unused_imports,
    reason = "plugin assembly consumes disk worker messages after the reload transaction is added"
)]
pub(crate) use worker::DiskWorkerMessage;
#[expect(
    unused_imports,
    reason = "plugin assembly starts the disk worker after companion generation is added"
)]
pub(crate) use worker::start_disk_worker;
