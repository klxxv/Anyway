pub mod agent_commands;
pub mod agent_host;
pub mod deepseek_client;
mod graph_cmds;
pub mod graph_compiler;
pub mod host_bus;
pub mod kernel;
mod kernel_commands;
pub mod llm_client;
pub mod llm_plugin;
pub mod llm_provider_registry;
pub mod native_plugins;
pub mod pdf_pipeline;
pub mod plugin_manifest_v2;
mod vsix_importer;
mod plugin_settings;
mod plugin_vm;
mod plugins;
mod projects;
mod signing;
#[cfg(windows)]
mod trackpad;
mod workspace_host;

// ── PDF Agent 多阶段提取与 GraphPatch 构建（不合并到 pdf_pipeline）──
pub use graphpatch_gen;
pub use semantic_pipeline;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let kernel_state = kernel_commands::create_kernel_state()
        .expect("kernel Host Bus routes must be valid");
    let agent_gate = kernel_commands::agent_gate_for(&kernel_state);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(kernel_state)
        .manage(agent_commands::AgentHostState::with_gate(
            agent_host::AgentHost::new(std::env::temp_dir()),
            agent_gate,
        ))
        .manage(llm_provider_registry::ProviderRegistryState::default())
        .manage(kernel_commands::CapabilityPolicyState::default())
        .invoke_handler(tauri::generate_handler![
            kernel_commands::kernel_host_call
        ])
        .setup(|app| {
            if let Err(error) = plugins::install_pending_packages(app.handle()) {
                eprintln!("Startup plugin package discovery failed: {error}");
            }

            #[cfg(debug_assertions)]
            let url = tauri::WebviewUrl::External(
                "http://127.0.0.1:5173/"
                    .parse()
                    .expect("valid Research Canvas development URL"),
            );
            #[cfg(not(debug_assertions))]
            let url = tauri::WebviewUrl::App("index.html".into());

            #[cfg_attr(not(windows), allow(unused_variables))]
            let window = tauri::WebviewWindowBuilder::new(app, "main", url)
                .title("Research Canvas")
                .inner_size(1440.0, 900.0)
                .min_inner_size(760.0, 560.0)
                .center()
                .resizable(true)
                .build()?;
            #[cfg(windows)]
            if let Err(error) = trackpad::enable_webview2_pinch_input(&window) {
                eprintln!("WebView2 pinch input unavailable: {error}");
            }
            #[cfg(windows)]
            if let Err(error) = trackpad::install(&window, app.handle().clone()) {
                eprintln!("Precision Touchpad pinch bridge unavailable: {error}");
            }
            #[cfg(debug_assertions)]
            window.open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Research Canvas");
}
