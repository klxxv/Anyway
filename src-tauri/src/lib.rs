pub mod agent_commands;
pub mod agent_host;
pub mod deepseek_client;
pub mod graph_algorithms;
pub mod graph_compiler;
pub mod pdf_pipeline;
mod graph_cmds;
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(agent_commands::AgentHostState(std::sync::Mutex::new(
            agent_host::AgentHost::new(std::env::temp_dir()),
        )))
        .invoke_handler(tauri::generate_handler![
            graph_cmds::compute_graph_layout,
            graph_cmds::layout_project_view,
            deepseek_client::set_deepseek_api_key,
            deepseek_client::has_deepseek_api_key,
            deepseek_client::clear_deepseek_api_key,
            plugins::install_myc_plugin,
            plugins::uninstall_myc_plugin,
            plugins::list_installed_plugins,
            plugins::execute_myc_plugin,
            projects::save_project_file,
            projects::import_project_file,
            workspace_host::save_plugin_artifact,
            workspace_host::scan_project_folder,
            workspace_host::read_git_workspace,
            workspace_host::initialize_git_workspace,
            workspace_host::read_github_account,
            workspace_host::login_github_account,
            workspace_host::generate_github_ssh_key,
            workspace_host::upload_github_ssh_key,
            workspace_host::git_autosave_project,
            agent_commands::start_pdf_job,
            agent_commands::get_job_status,
            agent_commands::review_patch,
            agent_commands::cancel_job
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
