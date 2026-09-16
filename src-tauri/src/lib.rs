mod capture;
#[cfg(windows)]
mod capture_win;
mod commands;
mod connector;
mod credential;
mod discovery;
mod model;
mod protocol;
mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        // 自动更新与安装后重启：产物由 CI 签名，前端不直接暴露下载地址。
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            match state::initialize_state(&handle) {
                Ok(app_state) => {
                    app.manage(app_state);
                    Ok(())
                }
                Err(error) => Err(error.to_string().into()),
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::db_status,
            commands::furnace_snapshot,
            commands::master_list,
            commands::master_detail,
            commands::master_versions,
            commands::master_revert,
            commands::master_flag_unit,
            commands::master_install,
            commands::master_validate,
            commands::coverage_matrix,
            commands::master_domains,
            commands::corpus_list,
            commands::corpus_search,
            commands::seed_packs_install,
            commands::settings_get,
            commands::settings_set,
            commands::networking_get,
            commands::networking_set,
            commands::platform_list,
            commands::platform_upsert,
            commands::platform_enable,
            commands::llm_calls,
            commands::model_probe,
            commands::council_candidates,
            commands::council_create,
            commands::council_select,
            commands::council_rotate,
            commands::council_run,
            commands::council_turns,
            commands::council_retry_seat,
            commands::council_followup,
            commands::council_conclusion,
            commands::council_cancel,
            commands::council_recoverable,
            commands::echo_check,
            commands::principle_revoke,
            commands::council_sessions,
            commands::council_session,
            commands::master_history,
            commands::tuning_get,
            commands::tuning_set,
            commands::connector_list,
            commands::connector_upsert,
            commands::connector_enable,
            commands::connector_test,
            commands::connector_calls,
            commands::council_sources,
            commands::council_search,
            commands::network_upsert_node,
            commands::network_link,
            commands::network_activate,
            commands::network_decay,
            commands::network_node,
            commands::network_graph,
            commands::network_resolve_conflict,
            commands::network_record_session,
            commands::records_list,
            commands::records_compare,
            commands::record_decision,
            commands::consolidate_trigger,
            commands::consolidate_report,
            commands::consolidate_runs,
            commands::companion_settings,
            commands::companion_enable,
            commands::companion_limit,
            commands::companion_rules,
            commands::companion_collide,
            commands::insights_list,
            commands::insight_mark,
            commands::insight_convert,
            commands::topics_list,
            commands::principles_list,
            commands::promote_principles,
            commands::ring_overview,
            commands::distill_start,
            commands::distill_from_intake,
            commands::distill_list,
            commands::distill_detail,
            commands::distill_confirm,
            commands::distill_resume,
            commands::intake_create,
            commands::intake_list,
            commands::intake_preview,
            commands::intake_confirm,
            commands::discovery_settings,
            commands::discovery_enable,
            commands::discovery_schedule,
            commands::discovery_run,
            commands::capture_settings,
            commands::capture_set_capability,
            commands::capture_set_paused,
            commands::capture_set_redaction,
            commands::capture_set_dedup,
            commands::capture_collect,
            commands::capture_events,
            commands::capture_summaries,
            commands::capture_delete_event,
            commands::capture_audit,
            commands::kb_sources,
            commands::kb_add_source,
            commands::kb_remove_source,
            commands::kb_scan,
            commands::kb_documents,
            commands::kb_search,
            commands::kb_overview,
            commands::self_readiness,
            commands::self_draft,
            commands::self_start,
            commands::self_decide,
            commands::self_install,
            commands::self_set_seat,
            commands::data_scope,
            commands::data_export,
            commands::data_purge,
            commands::data_events,
            commands::asset_roots,
            commands::asset_add_root,
            commands::asset_remove_root,
            commands::asset_scan,
            commands::asset_skills,
            commands::asset_skill_detail,
            commands::asset_summary,
            commands::cost_summary,
            commands::cost_estimate,
            commands::backup_create,
            commands::backup_list,
            commands::backup_restore,
            commands::credential_set,
            commands::credential_status,
        ])
        .run(tauri::generate_context!())
        .expect("思想熔炉启动失败");
}
