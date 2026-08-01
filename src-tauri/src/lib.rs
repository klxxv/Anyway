mod plugin_vm;
mod plugins;
mod projects;
#[cfg(windows)]
mod trackpad;
mod workspace_host;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            plugins::install_myc_plugin,
            plugins::list_installed_plugins,
            plugins::execute_myc_plugin,
            projects::save_project_file,
            projects::import_project_file,
            workspace_host::save_plugin_artifact,
            workspace_host::scan_project_folder,
            workspace_host::read_git_workspace,
            workspace_host::git_autosave_project
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            let url = tauri::WebviewUrl::External(
                "http://localhost:3000/"
                    .parse()
                    .expect("valid Research Canvas development URL"),
            );
            #[cfg(not(debug_assertions))]
            let url = tauri::WebviewUrl::App("index.html".into());

            let window = tauri::WebviewWindowBuilder::new(app, "main", url)
                .title("Research Canvas")
                .inner_size(1440.0, 900.0)
                .min_inner_size(760.0, 560.0)
                .center()
                .resizable(true)
                .build()?;
            #[cfg(windows)]
            if let Err(error) = trackpad::install(&window, app.handle().clone()) {
                eprintln!("Precision Touchpad observer unavailable: {error}");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Research Canvas");
}
