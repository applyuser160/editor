pub mod commands;
pub mod pty_manager;

use pty_manager::PtyState;

pub fn run() {
    let pty_state = PtyState::new();

    tauri::Builder::default()
        .manage(pty_state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
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
