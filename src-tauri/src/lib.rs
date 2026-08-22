pub mod commands;
pub mod extension_host;
pub mod file_watcher;
pub mod pty_manager;

use extension_host::ExtensionHostState;
use file_watcher::FileWatcherManager;
use pty_manager::PtyState;

pub fn run() {
    let pty_state = PtyState::new();
    let ext_state = ExtensionHostState::new();

    tauri::Builder::default()
        .manage(pty_state)
        .manage(ext_state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            if let Ok(cur_dir) = std::env::current_dir() {
                let _ = FileWatcherManager::start_watching(app.handle().clone(), &cur_dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_openvsx_extensions,
            commands::install_openvsx_extension,
            commands::git_list_branches,
            commands::git_checkout_branch,
            commands::git_create_branch,
            commands::get_installed_extensions,
            commands::start_extension_sidecar,
            commands::spawn_pty,
            commands::write_pty,
            commands::resize_pty,
            commands::list_workspace_files,
            commands::read_file_content,
            commands::write_file_content,
            commands::create_file,
            commands::create_directory,
            commands::delete_file,
            commands::rename_file,
            commands::get_file_stat,
            commands::search_in_workspace,
            commands::git_get_status,
            commands::git_commit,
            commands::execute_terminal_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oxide Editor Tauri application");
}
