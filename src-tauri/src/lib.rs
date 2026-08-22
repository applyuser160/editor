pub mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_workspace_files,
            commands::read_file_content,
            commands::write_file_content,
            commands::execute_terminal_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oxide Editor Tauri application");
}
