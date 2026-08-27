pub mod commands;
pub mod debug_config;
pub mod extension_host;
pub mod file_watcher;
pub mod lsp_client;
pub mod pty_manager;
pub mod task_runner;

use extension_host::ExtensionHostState;
use file_watcher::FileWatcherManager;
use lsp_client::LspState;
use pty_manager::PtyState;

pub fn run() {
    let pty_state = PtyState::new();
    let ext_state = ExtensionHostState::new();
    let lsp_state = LspState::new();

    tauri::Builder::default()
        .manage(pty_state)
        .manage(ext_state)
        .manage(lsp_state)
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Ok(cur_dir) = std::env::current_dir() {
                let _ = FileWatcherManager::start_watching(app.handle().clone(), &cur_dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::lsp_start_server,
            commands::lsp_send_notification,
            commands::lsp_send_request,
            commands::list_debug_configurations,
            commands::validate_debug_configuration,
            commands::search_openvsx_extensions,
            commands::install_openvsx_extension,
            commands::uninstall_extension,
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
            commands::find_rust_stdlib_definition,
            commands::git_get_status,
            commands::git_commit,
            commands::git_push,
            commands::git_pull,
            commands::git_stage_file,
            commands::git_unstage_file,
            commands::execute_terminal_command,
            commands::run_named_task,
            commands::get_workspace_path,
            commands::reveal_in_os_explorer,
            commands::replace_in_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oxide Editor Tauri application");
}
