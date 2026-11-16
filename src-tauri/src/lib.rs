//! Lume: cliente desktop do Claude com várias contas.
//!
//! Camadas (as dependências apontam sempre para dentro):
//! `presentation` → `application` → `domain`, com `infrastructure`
//! implementando as portas definidas em `application::ports`.

mod application;
mod domain;
mod infrastructure;
mod presentation;

use presentation::{commands, start_background_work, windows, Container};
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(windows::on_window_event)
        .setup(|app| {
            let container = Arc::new(Container::build(app.handle())?);
            // Sessões de versões anteriores, criadas sem projeto, entram no projeto da pasta delas.
            let _ = container.projects.adopt_sessions();
            app.manage(container.clone());
            start_background_work(container);
            windows::setup_tray(app.handle())?;
            windows::setup_shortcut(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::environment,
            commands::store_plugins,
            commands::store_categories,
            commands::store_install_plugin,
            commands::store_uninstall_plugin,
            commands::store_refresh_catalog,
            commands::store_servers,
            commands::store_add_server,
            commands::quick_entry_sent,
            commands::hide_quick_entry,
            commands::save_text_file,
            commands::list_accounts,
            commands::add_account,
            commands::remove_account,
            commands::set_account_enabled,
            commands::rename_account,
            commands::reorder_accounts,
            commands::refresh_account,
            commands::login_account_again,
            commands::list_switches,
            commands::refresh_usage,
            commands::list_providers,
            commands::account_limits,
            commands::list_conversations,
            commands::create_conversation,
            commands::delete_conversation,
            commands::branch_conversation,
            commands::list_messages,
            commands::list_subagent_steps,
            commands::set_conversation_model,
            commands::set_conversation_effort,
            commands::set_conversation_mode,
            commands::send_message,
            commands::copy_image,
            commands::inject_message,
            commands::list_tasks,
            commands::save_task,
            commands::set_task_enabled,
            commands::delete_task,
            commands::run_task,
            commands::cancel_message,
            commands::start_folder_session,
            commands::start_isolated_session,
            commands::decide_permission,
            commands::get_flags,
            commands::set_flag,
            commands::get_project_order,
            commands::set_project_order,
            commands::history_status,
            commands::history_list,
            commands::history_reindex,
            commands::history_open,
            commands::list_skills,
            commands::set_skill,
            commands::list_agents,
            commands::save_agent,
            commands::delete_agent,
            commands::draft_agent,
            commands::set_conversation_agent,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::set_project_versioned,
            commands::pipeline_config,
            commands::save_pipeline_config,
            commands::pipeline_models,
            commands::save_pipeline_models,
            commands::cli_models,
            commands::list_pipelines,
            commands::pipeline_detail,
            commands::start_pipeline,
            commands::decide_pipeline_plan,
            commands::pipeline_images,
            commands::reopen_pipeline,
            commands::cancel_pipeline,
            commands::retry_pipeline,
            commands::commit_pipeline,
            commands::delete_pipeline,
            commands::list_project_files,
            commands::add_project_files,
            commands::remove_project_file,
            commands::set_conversation_project,
            commands::project_memory_list,
            commands::project_memory_read,
            commands::project_memory_write,
            commands::project_memory_remove,
            commands::workspace_entries,
            commands::workspace_read,
            commands::workspace_status,
            commands::workspace_diff,
            commands::workspace_commit,
            commands::terminal_open,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
            commands::list_account_connectors,
            commands::submit_login_code,
            commands::link_connector,
            commands::unlink_connector,
            commands::set_server_enabled,
            commands::connector_icon,
            commands::list_connectors,
            commands::add_connector,
            commands::import_desktop_connectors,
            commands::set_connector_enabled,
            commands::remove_connector,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo")
        .run(|_app, _event| {
            // No macOS, clicar no ícone do Dock reabre a janela principal escondida.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                windows::show_main(_app);
            }
        });
}
