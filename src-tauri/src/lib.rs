mod plugin_vm;
mod plugins;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            plugins::install_myc_plugin,
            plugins::list_installed_plugins,
            plugins::execute_myc_plugin
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

            tauri::WebviewWindowBuilder::new(app, "main", url)
                .title("Research Canvas")
                .inner_size(1440.0, 900.0)
                .min_inner_size(760.0, 560.0)
                .center()
                .resizable(true)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Research Canvas");
}
