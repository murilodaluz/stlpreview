mod cache;
mod commands;
mod custom_preview;
mod fsutil;
mod imaging;
mod library;
mod mesh;
mod orca;
mod project;
mod rename;
mod render;
mod scan;
mod settings;
#[cfg(test)]
mod testutil;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result: tauri::Result<()> = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::detect_orca,
            commands::load_library,
            commands::refresh_library,
            commands::open_in_orca,
            commands::open_many_in_orca,
            commands::rename_model,
            commands::set_project,
            commands::set_custom_preview
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("sltpreview failed to run: {error}");
        std::process::exit(1);
    }
}
