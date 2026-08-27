pub mod commands;
pub mod debug_config;
pub mod debug_session;
pub mod extension_host;
pub mod file_watcher;
pub mod lsp_client;
pub mod pty_manager;
pub mod settings_store;
pub mod task_runner;
pub mod test_runner;
pub mod workspace;
pub mod workspace_edit;

use debug_session::DebugSessionState;
use extension_host::ExtensionHostState;
use file_watcher::FileWatcherManager;
use lsp_client::LspState;
use pty_manager::PtyState;
use workspace::WorkspaceState;

pub fn run() {
    let debug_session_state = DebugSessionState::new();
    let pty_state = PtyState::new();
    let ext_state = ExtensionHostState::new();
    let lsp_state = LspState::new();
    let workspace_state = WorkspaceState::new();
    let initial_workspace = workspace_state.root();

    tauri::Builder::default()
        .manage(debug_session_state)
        .manage(pty_state)
        .manage(ext_state)
        .manage(lsp_state)
        .manage(workspace_state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let _ = FileWatcherManager::start_watching(app.handle().clone(), &initial_workspace);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::lsp_start_server,
            commands::lsp_send_notification,
            commands::lsp_stop_all,
            commands::lsp_send_request,
            commands::search_openvsx_extensions,
            commands::install_openvsx_extension,
            commands::uninstall_extension,
            commands::set_extension_enabled,
            commands::git_list_branches,
            commands::git_checkout_branch,
            commands::git_create_branch,
            commands::get_installed_extensions,
            commands::start_extension_sidecar,
            commands::spawn_pty,
            commands::write_pty,
            commands::resize_pty,
            commands::list_workspace_tasks,
            commands::run_workspace_task,
            commands::list_workspace_test_suites,
            commands::run_workspace_test_suite,
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
            commands::git_get_file_comparison,
            commands::git_get_merge_versions,
            commands::execute_terminal_command,
            commands::get_workspace_path,
            commands::get_workspace_folders,
            commands::set_workspace_root,
            commands::add_workspace_folder,
            commands::remove_workspace_folder,
            commands::select_workspace_folder,
            commands::get_workspace_trust,
            commands::set_workspace_trust,
            commands::get_workspace_excludes,
            commands::set_workspace_excludes,
            commands::list_recent_workspaces,
            commands::remove_recent_workspace,
            commands::reveal_in_os_explorer,
            commands::replace_in_workspace,
            commands::load_editor_configuration,
            commands::save_editor_configuration,
            commands::migrate_editor_configuration,
            commands::store_credential,
            commands::has_credential,
            commands::delete_credential,
            commands::debug_list_configurations,
            commands::debug_check_adapter,
            commands::debug_start_session,
            commands::debug_stop_session,
            commands::debug_set_breakpoints,
            commands::debug_continue,
            commands::debug_next,
            commands::debug_step_in,
            commands::debug_step_out,
            commands::debug_pause,
            commands::debug_threads,
            commands::debug_stack_trace,
            commands::debug_scopes,
            commands::debug_variables,
            commands::debug_evaluate,
            commands::apply_workspace_edit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oxide Editor Tauri application");
}
