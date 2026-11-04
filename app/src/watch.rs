//! Live reload: watches the loaded keymap file and reloads it when it changes.
//! Editors often save by writing a new file and renaming it, so the folder is
//! watched and events are matched by file name.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::hub::Hub;

pub fn spawn(hub: Hub) {
    std::thread::spawn(move || {
        let (tx, rx) = channel::<notify::Result<Event>>();
        let mut watcher: Option<RecommendedWatcher> = None;
        let mut watched: Option<PathBuf> = None;

        loop {
            // Follow the loaded file
            let current = hub.lock().file_path.clone().map(PathBuf::from);
            if current != watched {
                watcher = None;
                if let Some(path) = &current {
                    let tx = tx.clone();
                    match notify::recommended_watcher(move |res| {
                        let _ = tx.send(res);
                    }) {
                        Ok(mut w) => {
                            let dir = path.parent().unwrap_or(Path::new("."));
                            if let Err(e) = w.watch(dir, RecursiveMode::NonRecursive) {
                                eprintln!("[FileMonitor] Could not watch {}: {e}", dir.display());
                            } else {
                                watcher = Some(w);
                            }
                        }
                        Err(e) => eprintln!("[FileMonitor] Could not start: {e}"),
                    }
                }
                watched = current;
            }
            let _keep_alive = &watcher;

            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    let Some(file) = &watched else { continue };
                    let relevant = event.paths.iter().any(|p| p.file_name() == file.file_name());
                    if !relevant || event.kind.is_access() {
                        continue;
                    }
                    // One save can fire several events; let them settle
                    std::thread::sleep(Duration::from_millis(200));
                    while rx.try_recv().is_ok() {}
                    eprintln!("[FileMonitor] File changed, reloading...");
                    hub.reload();
                }
                Ok(Err(e)) => eprintln!("[FileMonitor] {e}"),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}
