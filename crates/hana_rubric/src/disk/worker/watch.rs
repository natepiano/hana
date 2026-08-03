use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
#[cfg(test)]
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use notify::Event;
use notify::RecursiveMode;
use notify::Watcher;

use super::channels;
use super::runtime::DiskWorker;
use crate::disk::paths::KeymapPaths;

#[derive(Default)]
pub(super) struct WatchNotifications {
    pub(super) dirty:          bool,
    pub(super) health_message: Option<String>,
}

#[cfg(test)]
pub(super) struct TestWatcher {
    pub(super) watcher_notifications: Arc<Mutex<WatchNotifications>>,
    pub(super) watcher_sender:        SyncSender<()>,
}

#[derive(Clone, Copy)]
pub(super) enum WatchMode {
    Native,
    #[cfg(test)]
    PollOnly,
    #[cfg(test)]
    Injected,
    /// [`Self::Injected`], with the worker parked between arming its watcher and its first read
    /// until `DiskWorkerChannels::release_first_read` lets it go.
    #[cfg(test)]
    InjectedHoldingFirstRead,
}

impl WatchMode {
    pub(super) const fn is_native(self) -> bool {
        match self {
            Self::Native => true,
            #[cfg(test)]
            Self::PollOnly => false,
            #[cfg(test)]
            Self::Injected | Self::InjectedHoldingFirstRead => false,
        }
    }

    #[cfg(test)]
    pub(super) const fn is_injected(self) -> bool {
        matches!(self, Self::Injected | Self::InjectedHoldingFirstRead)
    }

    #[cfg(test)]
    pub(super) const fn holds_first_read(self) -> bool {
        matches!(self, Self::InjectedHoldingFirstRead)
    }

    #[cfg(not(test))]
    pub(super) const fn is_injected(self) -> bool {
        match self {
            Self::Native => false,
        }
    }
}

impl DiskWorker {
    pub(super) fn handle_watcher_notifications(&mut self, paths: &KeymapPaths) {
        let Some(watcher_notifications) = &self.watcher_notifications else {
            return;
        };
        let mut watcher_notifications = watcher_notifications
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let health_message = watcher_notifications.health_message.take();
        let dirty = std::mem::take(&mut watcher_notifications.dirty);
        drop(watcher_notifications);

        if let Some(health_message) = health_message {
            self.handle_watcher_failure(paths, health_message);
        } else if dirty {
            self.dirty_at = Some(Instant::now() + self.worker_timings.debounce);
            #[cfg(test)]
            self.status
                .dirty_notifications
                .fetch_add(1, Ordering::Release);
        }
    }

    pub(super) fn handle_watcher_failure(&mut self, paths: &KeymapPaths, message: String) {
        if self.reported_watch_error.as_deref() != Some(message.as_str()) {
            self.report_diagnostics(vec![channels::disk_diagnostic(
                paths.config_directory(),
                &message,
            )]);
            self.reported_watch_error = Some(message);
        }

        self.read_user_keymap(paths, Instant::now());
        self.recreate_watcher(paths);
    }

    pub(super) fn audit(&mut self, paths: &KeymapPaths, now: Instant) {
        let parent_exists = paths.config_directory().is_dir();

        if self.parent_exists != parent_exists {
            self.parent_exists = parent_exists;
            self.handle_watcher_failure(
                paths,
                String::from(
                    "The keymap configuration directory changed and its watch was recreated.",
                ),
            );
        }

        self.read_user_keymap(paths, now);

        if self.watcher.is_none() && parent_exists {
            self.recreate_watcher(paths);
        }
    }

    pub(super) fn recreate_watcher(&mut self, paths: &KeymapPaths) {
        self.watcher = None;
        self.status.watching.store(false, Ordering::Release);

        if self.watch_mode.is_injected() {
            if paths.config_directory().is_dir() {
                self.status.watching.store(true, Ordering::Release);
            } else {
                self.report_watcher_setup_failure(
                    paths.config_directory(),
                    String::from("The keymap configuration directory is unavailable for watching."),
                );
            }
            return;
        }

        self.watcher_notifications = None;
        self.watcher_receiver = None;

        if !self.watch_mode.is_native() {
            return;
        }

        if !paths.config_directory().is_dir() {
            self.report_watcher_setup_failure(
                paths.config_directory(),
                String::from("The keymap configuration directory is unavailable for watching."),
            );
            return;
        }

        let watch_targets = user_keymap_watch_targets(paths);
        let watch_notifications = Arc::new(Mutex::new(WatchNotifications::default()));
        let callback_notifications = Arc::clone(&watch_notifications);
        let (watcher_sender, watcher_receiver) = mpsc::sync_channel(1);
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let mut notifications = callback_notifications
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let notify_worker = match event {
                Ok(event) if event.need_rescan() => {
                    notifications.health_message = Some(String::from(
                        "The keymap watcher requested a complete rescan.",
                    ));
                    true
                },
                Ok(event)
                    if event
                        .paths
                        .iter()
                        .any(|event_path| watch_targets.contains(event_path)) =>
                {
                    notifications.dirty = true;
                    true
                },
                Ok(_) => false,
                Err(error) => {
                    notifications.health_message =
                        Some(format!("The keymap watcher reported an error: {error}"));
                    true
                },
            };
            drop(notifications);

            if notify_worker {
                let _ = watcher_sender.try_send(());
            }
        });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                self.report_watcher_setup_failure(
                    paths.config_directory(),
                    format!("Could not create the keymap watcher: {error}"),
                );
                return;
            },
        };

        if let Err(error) = watcher.watch(paths.config_directory(), RecursiveMode::NonRecursive) {
            self.report_watcher_setup_failure(
                paths.config_directory(),
                format!("Could not watch the keymap configuration directory: {error}"),
            );
            return;
        }

        self.watcher = Some(watcher);
        self.watcher_notifications = Some(watch_notifications);
        self.watcher_receiver = Some(watcher_receiver);
        self.reported_watch_error = None;
        self.status.watching.store(true, Ordering::Release);
    }

    fn report_watcher_setup_failure(&mut self, path: &Path, message: String) {
        if self.reported_watch_error.as_deref() != Some(message.as_str()) {
            self.report_diagnostics(vec![channels::disk_diagnostic(path, &message)]);
            self.reported_watch_error = Some(message);
        }
    }
}

/// Every path form the operating system may use when it reports an event for the user keymap.
///
/// macOS delivers `notify::Event` paths fully resolved, so an event for
/// [`KeymapPaths::user_keymap`] does not equal that path whenever the configuration directory is
/// reached through a symlink — the ordinary case under a dotfile manager, and the form `TMPDIR`
/// always takes. Accepting the resolved form as well keeps the native watch delivering an edit
/// within [`WorkerTimings::debounce`] rather than leaving it to the much later
/// [`WorkerTimings::poll`] audit.
///
/// [`WorkerTimings::debounce`]: super::runtime::WorkerTimings::debounce
/// [`WorkerTimings::poll`]: super::runtime::WorkerTimings::poll
fn user_keymap_watch_targets(paths: &KeymapPaths) -> [PathBuf; 2] {
    let configured_keymap = paths.user_keymap().to_path_buf();
    let resolved_keymap = paths
        .config_directory()
        .canonicalize()
        .ok()
        .zip(paths.user_keymap().file_name())
        .map_or_else(
            || configured_keymap.clone(),
            |(config_directory, file_name)| config_directory.join(file_name),
        );

    [configured_keymap, resolved_keymap]
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests stop when their isolated disk-worker setup fails"
)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;

    use super::WatchMode;
    use crate::disk::KeymapPaths;
    use crate::disk::paths::ENVIRONMENT_LOCK;
    use crate::disk::paths::TestDirectory;
    use crate::disk::paths::XdgConfigHome;
    use crate::disk::worker::channels::DiskWorkerChannels;
    use crate::disk::worker::runtime;
    use crate::disk::worker::runtime::WorkerTimings;

    const DEFAULT_KEYMAP: &[u8] = b"{\"bindings\": []}";
    const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);
    const NATIVE_WATCH_DEADLINE: Duration = Duration::from_millis(500);
    const POLL_AUDIT_INTERVAL: Duration = Duration::from_millis(20);
    const RETRY_INTERVAL: Duration = Duration::from_millis(30);
    const TEST_APP_NAME: &str = "hana-rubric-watch-test";
    const TEST_TIMEOUT: Duration = Duration::from_secs(3);
    const WAIT_INTERVAL: Duration = Duration::from_millis(5);

    fn isolated_paths(temporary_directory: &TestDirectory) -> Result<KeymapPaths, String> {
        let paths = KeymapPaths::new(TEST_APP_NAME)
            .ok_or_else(|| String::from("test keymap paths should resolve"))?;

        if !paths
            .config_directory()
            .starts_with(temporary_directory.path())
        {
            return Err(String::from(
                "test keymap path escaped the temporary directory",
            ));
        }

        Ok(paths)
    }

    fn start_worker(worker_timings: WorkerTimings, watch_mode: WatchMode) -> DiskWorkerChannels {
        let paths = KeymapPaths::new(TEST_APP_NAME).expect("test keymap paths resolve");
        runtime::start_disk_worker_with(
            &paths,
            DEFAULT_KEYMAP.to_vec(),
            Some(b"{}".to_vec()),
            worker_timings,
            watch_mode,
        )
    }

    fn wait_for_message(worker: &DiskWorkerChannels) -> Result<(), String> {
        let deadline = Instant::now() + TEST_TIMEOUT;

        while Instant::now() < deadline {
            if worker.take_message().is_some() {
                return Ok(());
            }
            thread::sleep(WAIT_INTERVAL);
        }

        Err(String::from(
            "disk worker did not publish a message before the test timeout",
        ))
    }

    fn wait_for_snapshot(
        worker: &DiskWorkerChannels,
        path: &Path,
        expected_contents: Option<&[u8]>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + TEST_TIMEOUT;

        while Instant::now() < deadline {
            if let Some(message) = worker.take_message()
                && message.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.source_path == path
                        && snapshot.contents.as_deref() == expected_contents
                })
            {
                return Ok(());
            }
            thread::sleep(WAIT_INTERVAL);
        }

        Err(String::from(
            "disk worker did not publish the requested snapshot",
        ))
    }

    fn wait_for_watcher(worker: &DiskWorkerChannels, expected: bool) -> Result<(), String> {
        let deadline = Instant::now() + TEST_TIMEOUT;

        while Instant::now() < deadline {
            if worker.is_watching() == expected {
                return Ok(());
            }
            thread::sleep(WAIT_INTERVAL);
        }

        Err(format!("disk worker watch state did not become {expected}"))
    }

    fn wait_for_dirty_notifications(
        worker: &DiskWorkerChannels,
        expected: usize,
    ) -> Result<(), String> {
        let deadline = Instant::now() + TEST_TIMEOUT;

        while Instant::now() < deadline {
            if worker.dirty_notifications() >= expected {
                return Ok(());
            }
            thread::sleep(WAIT_INTERVAL);
        }

        Err(String::from(
            "disk worker did not handle the injected watcher notification",
        ))
    }

    #[test]
    fn recreated_parent_directory_reestablishes_the_watch() -> Result<(), String> {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("recreated-parent").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let paths = isolated_paths(&temporary_directory)?;
        let worker_timings = WorkerTimings {
            debounce: DEBOUNCE_INTERVAL,
            poll:     None,
            retry:    RETRY_INTERVAL,
        };
        let mut worker = start_worker(worker_timings, WatchMode::Injected);

        assert!(
            paths
                .config_directory()
                .starts_with(temporary_directory.path())
        );
        wait_for_message(&worker)?;
        wait_for_watcher(&worker, true)?;

        fs::remove_dir_all(paths.config_directory())
            .map_err(|error| format!("configuration directory removal failed: {error}"))?;
        worker.inject_watcher_failure()?;
        wait_for_watcher(&worker, false)?;
        fs::create_dir_all(paths.config_directory())
            .map_err(|error| format!("configuration directory recreation failed: {error}"))?;
        worker.inject_watcher_failure()?;
        wait_for_watcher(&worker, true)?;
        wait_for_snapshot(&worker, paths.user_keymap(), None)?;
        let read_attempts_before_write = worker.read_attempts();
        fs::write(paths.user_keymap(), b"{\"bindings\": []}")
            .map_err(|error| format!("recreated user keymap write failed: {error}"))?;
        worker.inject_watcher_dirty()?;
        wait_for_dirty_notifications(&worker, 1)?;

        wait_for_snapshot(&worker, paths.user_keymap(), Some(b"{\"bindings\": []}"))?;
        if worker.read_attempts() != read_attempts_before_write + 1 {
            return Err(String::from(
                "the recreated watcher snapshot did not come from its injected watcher edge",
            ));
        }

        worker.shutdown();
        drop(xdg_config_home);
        drop(environment_lock);
        Ok(())
    }

    #[test]
    fn a_native_watch_carries_an_edit_without_a_poll_audit() -> Result<(), String> {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("native-watch").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let paths = isolated_paths(&temporary_directory)?;
        // The temporary directory is deliberately left in its symlinked form. A configuration
        // directory reached through a symlink is the ordinary case on macOS and under a dotfile
        // manager, and it is precisely the case a path-equality watch check fails.
        let worker_timings = WorkerTimings {
            debounce: DEBOUNCE_INTERVAL,
            poll:     None,
            retry:    RETRY_INTERVAL,
        };
        let mut worker = start_worker(worker_timings, WatchMode::Native);

        assert!(
            paths
                .config_directory()
                .starts_with(temporary_directory.path())
        );
        // Waiting for the stub proves the first read has already happened, so nothing but the
        // native watch can carry the edit written below.
        wait_for_snapshot(
            &worker,
            paths.user_keymap(),
            Some(runtime::USER_KEYMAP_STUB),
        )?;
        wait_for_watcher(&worker, true)?;

        let edited_contents = b"{\"bindings\": [{\"bindings\": {}}]}";
        let edited_at = Instant::now();
        fs::write(paths.user_keymap(), edited_contents)
            .map_err(|error| format!("native watch keymap write failed: {error}"))?;
        wait_for_snapshot(&worker, paths.user_keymap(), Some(edited_contents))?;
        let latency = edited_at.elapsed();

        if latency > NATIVE_WATCH_DEADLINE {
            return Err(format!(
                "the native watch delivered an edit after {latency:?}, past the {NATIVE_WATCH_DEADLINE:?} deadline",
            ));
        }

        worker.shutdown();
        drop(xdg_config_home);
        drop(environment_lock);
        Ok(())
    }

    #[test]
    fn polling_audit_detects_suppressed_native_events() -> Result<(), String> {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("polling-audit").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let paths = isolated_paths(&temporary_directory)?;
        let worker_timings = WorkerTimings {
            debounce: DEBOUNCE_INTERVAL,
            poll:     Some(POLL_AUDIT_INTERVAL),
            retry:    RETRY_INTERVAL,
        };
        let mut worker = start_worker(worker_timings, WatchMode::PollOnly);

        assert!(
            paths
                .config_directory()
                .starts_with(temporary_directory.path())
        );
        wait_for_message(&worker)?;
        fs::write(paths.user_keymap(), b"{\"bindings\": [{\"bindings\": {}}]}")
            .map_err(|error| format!("polled user keymap write failed: {error}"))?;

        wait_for_snapshot(
            &worker,
            paths.user_keymap(),
            Some(b"{\"bindings\": [{\"bindings\": {}}]}"),
        )?;

        worker.shutdown();
        drop(xdg_config_home);
        drop(environment_lock);
        Ok(())
    }
}
