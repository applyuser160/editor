use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub struct FileWatcherManager;

impl FileWatcherManager {
    pub fn start_watching(app_handle: AppHandle, watch_dir: &Path) -> Result<(), String> {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        watcher
            .watch(watch_dir, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch directory: {}", e))?;

        let app_clone = app_handle.clone();
        std::thread::spawn(move || {
            // Keep watcher alive
            let _watcher = watcher;
            while let Ok(res) = rx.recv() {
                match res {
                    Ok(Event { paths, kind, .. }) => {
                        let path_strs: Vec<String> = paths
                            .into_iter()
                            .filter(|p| {
                                let s = p.to_string_lossy();
                                !s.contains(".git")
                                    && !s.contains("target")
                                    && !s.contains("node_modules")
                                    && !s.contains("dist")
                            })
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();

                        if !path_strs.is_empty() {
                            let _ = app_clone.emit(
                                "fs-change",
                                serde_json::json!({
                                    "paths": path_strs,
                                    "kind": format!("{:?}", kind)
                                }),
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("File watch error: {:?}", e);
                    }
                }
            }
        });

        Ok(())
    }
}
