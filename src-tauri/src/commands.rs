use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use thought_forge_core::asset::{
    service as asset_service, AssetRootView, AssetScanOutcome, AssetSummary, SkillDetail,
    SkillFilter, SkillView,
};
use thought_forge_core::backup::{self, BackupOutcome, BackupView};
use thought_forge_core::companion::{
    collide as companion_collide_mod, growth as companion_growth, repo as companion_repo,
    CollisionOutcome, CollisionSignal, CompanionRules, CompanionSettings, Insight, InsightFilter,
    InsightKind, PrincipleSeal, RingOverview, TopicView,
};
use thought_forge_core::capture::{
    pipeline as capture_pipeline, repo as capture_repo, CaptureAuditView, CaptureEventView,
    CaptureFilter, CaptureOutcome, CaptureSettingsView, CaptureSummaryView,
    RedactionRules,
};
use thought_forge_core::council::{
    conclusion as council_conclusion_mod, control as council_control, divergence,
    echo as council_echo, followup as council_followup_mod, orchestrator, pool,
    repo as council_repo, select, speech as council_speech, tuning as council_tuning,
    CandidatePool, ConclusionView, CouncilOutcome, FollowUpAnchor, MasterHistoryEntry,
    SeatSpeech, Selection, SessionDetail, SessionView, Strategy,
};
use thought_forge_core::council::echo::EchoHit;
use thought_forge_core::council::tuning::TuningItem;
use thought_forge_core::connector::{
    guard, repo as connector_repo, service as connector_service, ConnectorInput, ConnectorView,
    PageReader, SearchHit, SearchProvider,
};
use thought_forge_core::connector::repo::ConnectorCallView;
use thought_forge_core::corpus::{repo as corpus_repo, CorpusItemView, CorpusSearchHit};
use thought_forge_core::cost::{self, CostEstimate, CostSummary};
use thought_forge_core::credential::{self, CredentialRefView};
use thought_forge_core::data::service as data_service;
use thought_forge_core::data::{DataEventView, DataScope, ExportOutcome, PurgeOutcome};
use thought_forge_core::distill::intake::{self as intake_service, ManualIntakeInput};
use thought_forge_core::distill::{
    pipeline as distill_pipeline, repo as distill_repo, DiscoveryOutcome, DiscoverySettings,
    DistillDetail, DistillJobView, IntakeJobView, IntakeMaterial, SignalView,
};
use thought_forge_core::llm::platform::{self as platform_repo, PlatformInput, PlatformView};
use thought_forge_core::llm::probe::{self, ProbeOutcome};
use thought_forge_core::llm::{recent_calls, CallView, GatedClient, ModelClient, RetryPolicy};
use thought_forge_core::kb::{
    service as kb_service, KbDocumentView, KbFilter, KbScanOutcome, KbSearchHit, KbSourceView,
    KnowledgeOverview,
};
use thought_forge_core::master::{
    repo as master_repo, CoverageMatrix, InstallOutcome, Layer, MasterDetail, MasterSummary,
    VersionView, LAYER_ORDER,
};
use thought_forge_core::network::{
    consolidate, recorder as network_recorder, repo as network_repo, ActivationOutcome,
    ConsolidationReport, ConsolidationRunView, EdgeUpsert, GraphFilter, GraphView, NodeDetail,
    NodeKind, NodeUpsert, RecordOutcome, Relation, ThoughtRecordView, MAX_CONSOLIDATION_BATCH,
};
use thought_forge_core::self_distill;
use thought_forge_core::self_distill::service as self_service;
use thought_forge_core::self_distill::{SelfDraftDetail, SelfReadiness};
use thought_forge_core::{db, furnace, CoreError, CoreResult};

use crate::model::{self, BlockedClient};
use crate::protocol::CommandResult;
use crate::state::AppState;

use crate::connector::ShellConnector;
use crate::discovery;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    pub name: &'static str,
    pub version: &'static str,
    pub schema_version: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatusDto {
    pub path: String,
    pub schema_version: i64,
    pub journal_mode: String,
    pub foreign_keys: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FurnaceSnapshotDto {
    pub active_nodes: i64,
    pub total_nodes: i64,
    pub recent_captures: i64,
    pub recent_councils: i64,
    pub computed_at: String,
}

/// 检索预演结果：待确认内容、发送串指纹与已取得的结果。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutcomeDto {
    /// 预演开启且尚未确认时为真，此时不发起任何对外请求。
    pub pending: bool,
    pub fingerprint: String,
    pub prepared: Option<thought_forge_core::connector::guard::PreparedQuery>,
    pub hits: Vec<SearchHit>,
}

/// 连通测试结果：同样支持两阶段确认。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorTestDto {
    pub pending: bool,
    pub fingerprint: String,
    pub prepared: Option<thought_forge_core::connector::guard::PreparedQuery>,
    pub call: Option<ConnectorCallView>,
}

fn lock<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, rusqlite::Connection> {
    // 锁中毒意味着上一次持锁线程 panic；此处沿用内部连接以避免整个应用不可用。
    state
        .conn
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> CommandResult<AppInfoDto> {
    let conn = lock(&state);
    let result: CoreResult<AppInfoDto> = thought_forge_core::app_info(&conn).map(|info| AppInfoDto {
        name: info.name,
        version: info.version,
        schema_version: info.schema_version,
    });
    result.into()
}

#[tauri::command]
pub fn db_status(state: State<'_, AppState>) -> CommandResult<DbStatusDto> {
    let conn = lock(&state);
    let result: CoreResult<DbStatusDto> = (|| {
        Ok(DbStatusDto {
            path: state.db_path.to_string_lossy().to_string(),
            schema_version: db::migrations::current_version(&conn)?,
            journal_mode: db::journal_mode(&conn)?,
            foreign_keys: db::foreign_keys_enabled(&conn)?,
        })
    })();
    result.into()
}

#[tauri::command]
pub fn furnace_snapshot(state: State<'_, AppState>) -> CommandResult<FurnaceSnapshotDto> {
    let conn = lock(&state);
    let result: CoreResult<FurnaceSnapshotDto> = furnace::snapshot(&conn).map(|snapshot| {
        FurnaceSnapshotDto {
            active_nodes: snapshot.active_nodes,
            total_nodes: snapshot.total_nodes,
            recent_captures: snapshot.recent_captures,
            recent_councils: snapshot.recent_councils,
            computed_at: snapshot.computed_at,
        }
    });
    result.into()
}

#[tauri::command]
pub fn master_list(
    state: State<'_, AppState>,
    domain: Option<String>,
    layer: Option<String>,
) -> CommandResult<Vec<MasterSummary>> {
    let parsed = match layer.as_deref() {
        None => None,
        Some(value) => match Layer::parse(value) {
            Some(layer) => Some(layer),
            None => return CommandResult::failure("E_INVALID_INPUT", format!("未知层次：{value}")),
        },
    };
    let conn = lock(&state);
    master_repo::list(&conn, domain.as_deref(), parsed).into()
}

#[tauri::command]
pub fn master_detail(state: State<'_, AppState>, master_id: String) -> CommandResult<MasterDetail> {
    let conn = lock(&state);
    master_repo::detail(&conn, &master_id).into()
}

#[tauri::command]
pub fn master_versions(
    state: State<'_, AppState>,
    master_id: String,
) -> CommandResult<Vec<VersionView>> {
    let conn = lock(&state);
    master_repo::versions(&conn, &master_id).into()
}

#[tauri::command]
pub fn master_revert(
    state: State<'_, AppState>,
    master_id: String,
    version: i64,
) -> CommandResult<i64> {
    let conn = lock(&state);
    match master_repo::revert(&conn, &master_id, version) {
        Ok(version) => CommandResult::ok(version),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn master_flag_unit(
    state: State<'_, AppState>,
    unit_id: String,
    reason: String,
) -> CommandResult<Option<()>> {
    let conn = lock(&state);
    match master_repo::flag_unit(&conn, &unit_id, &reason) {
        Ok(()) => CommandResult::ok(None),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn master_install(state: State<'_, AppState>, pack_path: String) -> CommandResult<InstallOutcome> {
    let mut conn = lock(&state);
    match master_repo::install(&mut conn, &PathBuf::from(pack_path)) {
        Ok(outcome) => CommandResult::ok(outcome),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn master_validate(state: State<'_, AppState>, pack_path: String) -> CommandResult<serde_json::Value> {
    let _ = &state;
    match master_repo::validate_only(&PathBuf::from(pack_path)) {
        Ok(value) => CommandResult::ok(value),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn coverage_matrix(state: State<'_, AppState>) -> CommandResult<CoverageMatrix> {
    let conn = lock(&state);
    master_repo::coverage_matrix(&conn).into()
}

#[tauri::command]
pub fn master_domains(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let conn = lock(&state);
    master_repo::domains(&conn).into()
}

#[tauri::command]
pub fn corpus_list(
    state: State<'_, AppState>,
    master_id: Option<String>,
) -> CommandResult<Vec<CorpusItemView>> {
    let conn = lock(&state);
    corpus_repo::list(&conn, master_id.as_deref()).into()
}

#[tauri::command]
pub fn corpus_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> CommandResult<Vec<CorpusSearchHit>> {
    let conn = lock(&state);
    corpus_repo::search(&conn, &query, limit).into()
}

/// 安装随应用分发的种子大师包，返回成功与失败清单。
#[tauri::command]
pub fn seed_packs_install(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<SeedInstallReport> {
    let root = match seed_root(&app) {
        Some(root) => root,
        None => {
            return CommandResult::failure("E_NOT_FOUND", "未找到种子大师包目录");
        }
    };

    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            return CommandResult::failure("E_IO", format!("读取种子目录失败：{error}"));
        }
    };

    let mut packs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("master.json").is_file())
        .collect();
    packs.sort();

    let mut installed: Vec<InstallOutcome> = Vec::new();
    let mut failures: Vec<SeedFailure> = Vec::new();
    let mut conn = lock(&state);
    for path in packs {
        match master_repo::install(&mut conn, &path) {
            Ok(outcome) => installed.push(outcome),
            Err(error) => failures.push(SeedFailure {
                pack: path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                message: error.to_string(),
            }),
        }
    }

    CommandResult::ok(SeedInstallReport {
        root: root.to_string_lossy().to_string(),
        installed,
        failures,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedFailure {
    pub pack: String,
    pub message: String,
}

/// 扫描种子目录并逐个安装。不依赖 Tauri 类型，启动期与命令共用。
pub fn install_all(
    root: &std::path::Path,
    conn: &mut rusqlite::Connection,
) -> thought_forge_core::CoreResult<SeedInstallReport> {
    let entries = std::fs::read_dir(root).map_err(|error| {
        thought_forge_core::CoreError::InvalidInput(format!("读取种子目录失败：{error}"))
    })?;

    let mut packs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("master.json").is_file())
        .collect();
    packs.sort();

    let mut installed: Vec<InstallOutcome> = Vec::new();
    let mut failures: Vec<SeedFailure> = Vec::new();
    for path in packs {
        match master_repo::install(conn, &path) {
            Ok(outcome) => installed.push(outcome),
            Err(error) => failures.push(SeedFailure {
                pack: path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                message: error.to_string(),
            }),
        }
    }

    Ok(SeedInstallReport {
        root: root.to_string_lossy().to_string(),
        installed,
        failures,
    })
}

/// 一次性安装标记的设置键。
pub const SEED_INSTALLED_KEY: &str = "seed_packs_installed";

/// 首启自动装种子包：装成功后落一次性标记，之后不再自动装，
/// 免得用户删掉某位大师后每次启动又被塞回来。
pub fn seed_install_once(app: &tauri::AppHandle) -> thought_forge_core::CoreResult<()> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let mut conn = lock(&state);
    if db::settings::get(&conn, SEED_INSTALLED_KEY)?.as_deref() == Some("1") {
        return Ok(());
    }
    // 名册非空说明不是首启：只落标记，不重装，免得现有安装被塞出版本堆积。
    if !master_repo::list(&conn, None, None)?.is_empty() {
        db::settings::set(&conn, SEED_INSTALLED_KEY, "1")?;
        return Ok(());
    }
    let Some(root) = seed_root(app) else {
        return Ok(());
    };
    let report = install_all(&root, &mut conn)?;
    // 有失败就不落标记，下次启动再试一次。
    if report.failures.is_empty() {
        db::settings::set(&conn, SEED_INSTALLED_KEY, "1")?;
        eprintln!("已自动安装 {} 个种子大师包", report.installed.len());
    } else {
        eprintln!("种子大师包部分安装失败：{} 个", report.failures.len());
    }
    Ok(())
}

#[cfg(test)]
mod seed_install_tests {
    use super::*;

    /// 种子目录里的每个包都能装上，且没有失败项（名册现为二十位）。
    #[test]
    fn install_all_installs_every_seed_pack() {
        let dir = std::env::temp_dir().join(format!("forge-seed-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let (mut conn, _version) =
            thought_forge_core::db::initialize(dir.join("forge.db")).expect("初始化库");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓库根")
            .join("seed-packs");
        let report = install_all(&root, &mut conn).expect("安装种子包");
        assert_eq!(report.installed.len(), 20);
        assert!(report.failures.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedInstallReport {
    pub root: String,
    pub installed: Vec<InstallOutcome>,
    pub failures: Vec<SeedFailure>,
}

/// 种子包优先从打包资源目录读取，开发期退回源码目录。
fn seed_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    if let Ok(resource) = app.path().resource_dir() {
        let candidate = resource.join("seed-packs");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("seed-packs"))?;
    if dev.is_dir() {
        Some(dev)
    } else {
        None
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>, key: String) -> CommandResult<Option<String>> {
    let conn = lock(&state);
    db::settings::get(&conn, &key).into()
}

#[tauri::command]
pub fn settings_set(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> CommandResult<Option<()>> {
    let conn = lock(&state);
    match db::settings::set(&conn, &key, &value) {
        Ok(()) => CommandResult::ok(None),
        Err(error) => error.into(),
    }
}

/// 联网能力开关的持久化键。默认关闭，逐项开启。
const NETWORKING_KEY: &str = "networking_enabled";

fn networking_enabled(conn: &rusqlite::Connection) -> CoreResult<bool> {
    let value = db::settings::get(conn, NETWORKING_KEY)?;
    Ok(matches!(value.as_deref(), Some("true") | Some("1")))
}

#[tauri::command]
pub fn networking_get(state: State<'_, AppState>) -> CommandResult<bool> {
    let conn = lock(&state);
    networking_enabled(&conn).into()
}

#[tauri::command]
pub fn networking_set(state: State<'_, AppState>, enabled: bool) -> CommandResult<bool> {
    let conn = lock(&state);
    let value = if enabled { "true" } else { "false" };
    match db::settings::set(&conn, NETWORKING_KEY, value) {
        Ok(()) => CommandResult::ok(enabled),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn platform_list(state: State<'_, AppState>) -> CommandResult<Vec<PlatformView>> {
    let conn = lock(&state);
    platform_repo::list(&conn).into()
}

#[tauri::command]
// 参数即前端 `platform_upsert` 的请求字段，合并成结构体会改变 IPC 形状。
#[allow(clippy::too_many_arguments)]
pub fn platform_upsert(
    state: State<'_, AppState>,
    code: String,
    display_name: String,
    endpoint: String,
    model_name: String,
    input_price_micros_per_1k: Option<i64>,
    output_price_micros_per_1k: Option<i64>,
    currency: Option<String>,
) -> CommandResult<PlatformView> {
    let conn = lock(&state);
    let input = PlatformInput {
        code,
        display_name,
        endpoint,
        model_name,
        input_price_micros_per_1k: input_price_micros_per_1k.unwrap_or(0),
        output_price_micros_per_1k: output_price_micros_per_1k.unwrap_or(0),
        currency: currency.unwrap_or_else(|| "CNY".to_string()),
    };
    match platform_repo::upsert(&conn, &input) {
        Ok(view) => CommandResult::ok(view),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn platform_enable(
    state: State<'_, AppState>,
    code: String,
    enabled: bool,
) -> CommandResult<PlatformView> {
    let conn = lock(&state);
    match platform_repo::set_enabled(&conn, &code, enabled) {
        Ok(view) => CommandResult::ok(view),
        Err(error) => error.into(),
    }
}

#[tauri::command]
pub fn llm_calls(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<Vec<CallView>> {
    let conn = lock(&state);
    recent_calls(&conn, limit.unwrap_or(50)).into()
}

/// 外壳自检：用一次最小模型调用判定链路是否可用。
///
/// 结果自带退出码（0 成功、1 模型不可用、2 联网关闭、3 平台未配置），
/// 命令行自检与界面提示共用同一份判定。
#[tauri::command]
pub fn model_probe(state: State<'_, AppState>) -> CommandResult<ProbeOutcome> {
    let conn = lock(&state);
    let result = (|| {
        let enabled = networking_enabled(&conn)?;
        let platform = if enabled {
            platform_repo::enabled(&conn)?
        } else {
            None
        };
        let key_present = platform
            .as_ref()
            .map(|view| crate::credential::has_api_key(&view.code))
            .unwrap_or(false);
        let inner: Box<dyn ModelClient> = match platform.as_ref() {
            Some(view) => match model::build_client(view) {
                Ok(client) => Box::new(client),
                Err(error) => Box::new(BlockedClient::unavailable(error.to_string())),
            },
            None => Box::new(BlockedClient::unavailable("尚未启用任何模型平台")),
        };
        let client = GatedClient {
            enabled,
            inner: inner.as_ref(),
        };
        probe::probe(
            &conn,
            enabled,
            platform.as_ref(),
            &client,
            &RetryPolicy::default(),
            key_present,
        )
    })();
    result.into()
}

/// 构建候选池，供圆桌界面展示三项评分与层次空缺。
#[tauri::command]
pub fn council_candidates(
    state: State<'_, AppState>,
    question: Option<String>,
    domains: Option<Vec<String>>,
) -> CommandResult<CandidatePool> {
    let question = question.unwrap_or_default();
    let owned = domains.unwrap_or_default();
    let conn = lock(&state);
    pool::build(
        &conn,
        &pool::TopicInput {
            question: &question,
            domains: &owned,
        },
    )
    .into()
}

fn parse_strategy(value: Option<&str>) -> CoreResult<Strategy> {
    match value {
        None => Ok(Strategy::Steady),
        Some(raw) => Strategy::parse(raw)
            .ok_or_else(|| CoreError::InvalidInput(format!("未知选角策略：{raw}"))),
    }
}

fn parse_strategy_selection(value: &str) -> CoreResult<Strategy> {
    Strategy::parse(value)
        .ok_or_else(|| CoreError::InvalidInput(format!("未知选角策略：{value}")))
}

/// 新建一次会诊，返回草稿会话。
#[tauri::command]
pub fn council_create(
    state: State<'_, AppState>,
    question: String,
    strategy: Option<String>,
    domains: Option<Vec<String>>,
    include_self: Option<bool>,
) -> CommandResult<SessionView> {
    let parsed = match parse_strategy(strategy.as_deref()) {
        Ok(strategy) => strategy,
        Err(error) => return error.into(),
    };
    let include_self = include_self.unwrap_or(true);
    let owned = domains.unwrap_or_default();
    let layers: Vec<Layer> = LAYER_ORDER.to_vec();
    let conn = lock(&state);
    let result = (|| {
        let id = council_repo::create_session(&conn, &question, &owned, &layers, parsed)?;
        council_repo::set_self_seat_included(&conn, &id, include_self)?;
        council_repo::get_session(&conn, &id)
    })();
    result.into()
}

/// 首次选角并记录阵容轮次。
#[tauri::command]
pub fn council_select(
    state: State<'_, AppState>,
    session_id: String,
    strategy: Option<String>,
    size: Option<usize>,
    pinned: Option<Vec<String>>,
) -> CommandResult<Selection> {
    let parsed = match parse_strategy_selection(strategy.as_deref().unwrap_or("steady")) {
        Ok(strategy) => strategy,
        Err(error) => return error.into(),
    };
    let pinned = pinned.unwrap_or_default();
    let size = size.unwrap_or(thought_forge_core::council::DEFAULT_PANEL_SIZE);
    let conn = lock(&state);
    let result = (|| {
        let session = council_repo::get_session(&conn, &session_id)?;
        let (pinned, size) = reserve_self_seat(&conn, pinned, size, session.self_seat_included)?;
        let exclude = self_exclusion(&session);
        let previous = council_repo::latest_panel(&conn, &session_id)?
            .map(|panel| panel.seats)
            .unwrap_or_default();
        let diverged = diverged_layers(&session);
        let request = select::SelectionRequest {
            strategy: parsed,
            size,
            pinned: &pinned,
            exclude: &exclude,
            previous: &previous,
            diverged: &diverged,
        };
        let selection = select_selection(&conn, &session.question, &request)?;
        let rotation = council_repo::latest_rotation(&conn, &session_id)?.unwrap_or(-1) + 1;
        council_repo::record_panel(&conn, &session_id, rotation, &selection, &pinned)?;
        Ok(selection)
    })();
    result.into()
}

/// 换批：排除上一阵容的非保留席位，并把新阵容记为下一轮。
#[tauri::command]
pub fn council_rotate(
    state: State<'_, AppState>,
    session_id: String,
    strategy: Option<String>,
    size: Option<usize>,
    pinned: Option<Vec<String>>,
) -> CommandResult<Selection> {
    let parsed = match parse_strategy_selection(strategy.as_deref().unwrap_or("clash")) {
        Ok(strategy) => strategy,
        Err(error) => return error.into(),
    };
    let conn = lock(&state);
    let result = (|| {
        let session = council_repo::get_session(&conn, &session_id)?;
        let previous = council_repo::latest_panel(&conn, &session_id)?;
        let pinned = pinned.unwrap_or_else(|| {
            previous
                .as_ref()
                .map(|panel| panel.pinned_ids.clone())
                .unwrap_or_default()
        });
        let mut exclude: Vec<String> = previous
            .as_ref()
            .map(|panel| {
                panel
                    .master_ids
                    .iter()
                    .filter(|id| !pinned.contains(id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        exclude.extend(self_exclusion(&session));
        let (pinned, size) = reserve_self_seat(
            &conn,
            pinned,
            size.unwrap_or(thought_forge_core::council::DEFAULT_PANEL_SIZE),
            session.self_seat_included,
        )?;
        let diverged = diverged_layers(&session);
        let request = select::SelectionRequest {
            strategy: parsed,
            size,
            pinned: &pinned,
            exclude: &exclude,
            previous: previous.as_ref().map(|panel| panel.seats.as_slice()).unwrap_or(&[]),
            diverged: &diverged,
        };
        let selection = select_selection(&conn, &session.question, &request)?;
        let rotation = council_repo::latest_rotation(&conn, &session_id)?.unwrap_or(-1) + 1;
        council_repo::record_panel(&conn, &session_id, rotation, &selection, &pinned)?;
        Ok(selection)
    })();
    result.into()
}

/// 上一轮没能谈拢的题，按题序去重，供换批时优先补人。
fn diverged_layers(session: &SessionView) -> Vec<Layer> {
    let mut layers: Vec<Layer> = session.divergences.iter().map(|item| item.layer).collect();
    layers.sort();
    layers.dedup();
    layers
}

fn select_selection(
    conn: &rusqlite::Connection,
    question: &str,
    request: &select::SelectionRequest<'_>,
) -> CoreResult<Selection> {
    let candidate_pool = pool::build(
        conn,
        &pool::TopicInput {
            question,
            domains: &[],
        },
    )?;
    select::select_panel(conn, &candidate_pool, request)
}

/// 已安装「你」且席位开启时，把「你」作为保留席位加入，并把阵容扩到七席。
/// 未安装或已关闭时原样返回，不影响普通会诊。
fn reserve_self_seat(
    conn: &rusqlite::Connection,
    mut pinned: Vec<String>,
    size: usize,
    include_self: bool,
) -> CoreResult<(Vec<String>, usize)> {
    if !include_self {
        return Ok((pinned, size));
    }
    if let Some(seat) = self_service::seat_master_id(conn)? {
        if !pinned.contains(&seat) {
            pinned.push(seat);
        }
        let expanded = (size + 1).min(thought_forge_core::council::MAX_PANEL_SIZE);
        return Ok((pinned, expanded));
    }
    Ok((pinned, size))
}

/// 「这一场不带我」：单场关闭自我席位时，选角显式排除该席位。
fn self_exclusion(session: &SessionView) -> Vec<String> {
    if session.self_seat_included {
        Vec::new()
    } else {
        vec![self_distill::SELF_MASTER_ID.to_string()]
    }
}

/// 执行会诊：两轮发言加收敛裁决。
#[tauri::command]
pub fn council_run(state: State<'_, AppState>, session_id: String) -> CommandResult<CouncilOutcome> {
    let conn = lock(&state);
    // 连接器受全局联网开关约束：关闭时不装配任何外部能力，编排按「未获得外部背景」继续。
    let enabled = match networking_enabled(&conn) {
        Ok(enabled) => enabled,
        Err(error) => return error.into(),
    };
    let shell = match ShellConnector::from_db(&conn, enabled) {
        Ok(shell) => shell,
        Err(error) => return error.into(),
    };
    let retrieval = shell.retrieval();
    with_model_client(&conn, &retrieval, |client, retrieval| {
        let policy = RetryPolicy::default();
        // 极性判定复用同一次联网装配；不可用时内核自行回退词面判定并留痕。
        let judge = divergence::ModelPolarityJudge {
            conn: &conn,
            client,
            policy,
        };
        orchestrator::run_council_with_judge(
            &conn,
            client,
            retrieval,
            &session_id,
            &policy,
            Some(&judge),
        )
    })
}

/// 按联网开关装配一次模型客户端，供会诊与席位重试共用。
fn with_model_client<T>(
    conn: &rusqlite::Connection,
    retrieval: &thought_forge_core::connector::service::Retrieval<'_>,
    run: impl FnOnce(&dyn ModelClient, &thought_forge_core::connector::service::Retrieval<'_>) -> CoreResult<T>,
) -> CommandResult<T> {
    let enabled = match networking_enabled(conn) {
        Ok(enabled) => enabled,
        Err(error) => return error.into(),
    };

    let inner: Box<dyn ModelClient> = if enabled {
        match platform_repo::enabled(conn) {
            Ok(Some(view)) => match model::build_client(&view) {
                Ok(client) => Box::new(client),
                Err(error) => return error.into(),
            },
            Ok(None) => Box::new(BlockedClient::unavailable("尚未启用任何模型平台")),
            Err(error) => return error.into(),
        }
    } else {
        Box::new(BlockedClient::network_off(
            "联网能力已关闭，先在设置页开启模型平台",
        ))
    };

    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    run(&client, retrieval).into()
}

/// 读取某个阵容轮次下每个席位的发言与状态。未指定轮次时取最后一次阵容。
#[tauri::command]
pub fn council_turns(
    state: State<'_, AppState>,
    session_id: String,
    rotation: Option<i64>,
) -> CommandResult<Vec<SeatSpeech>> {
    let conn = lock(&state);
    let result = (|| {
        let rotation = match rotation {
            Some(rotation) => rotation,
            None => council_repo::latest_rotation(&conn, &session_id)?.unwrap_or(0),
        };
        council_speech::seat_speech(&conn, &session_id, rotation)
    })();
    result.into()
}

/// 重试某个席位在某一轮的发言，只重跑该席位该轮。
#[tauri::command]
pub fn council_retry_seat(
    state: State<'_, AppState>,
    session_id: String,
    rotation: i64,
    master_id: String,
    round: i64,
) -> CommandResult<Vec<SeatSpeech>> {
    let conn = lock(&state);
    let retrieval = thought_forge_core::connector::service::Retrieval::none();
    with_model_client(&conn, &retrieval, |client, _retrieval| {
        council_speech::retry_seat(
            &conn,
            client,
            &session_id,
            rotation,
            &master_id,
            round,
            &RetryPolicy::default(),
        )
    })
}

/// 围绕母会话里的一段判断新建追问会话，母会话不被改写。
#[tauri::command]
pub fn council_followup(
    state: State<'_, AppState>,
    parent_session_id: String,
    anchor: FollowUpAnchor,
    question: String,
    inherit_panel: Option<bool>,
) -> CommandResult<SessionView> {
    let conn = lock(&state);
    council_followup_mod::create_followup(
        &conn,
        &parent_session_id,
        &anchor,
        &question,
        inherit_panel.unwrap_or(true),
    )
    .into()
}

/// 会诊结论详情页的六段数据。
#[tauri::command]
pub fn council_conclusion(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<ConclusionView> {
    let conn = lock(&state);
    council_conclusion_mod::conclusion_view(&conn, &session_id).into()
}

/// 请求取消：取消在当前轮次结束时生效，已完成轮次全部保留。
#[tauri::command]
pub fn council_cancel(state: State<'_, AppState>, session_id: String) -> CommandResult<SessionView> {
    let conn = lock(&state);
    let result = (|| {
        council_control::request_cancel(&conn, &session_id)?;
        council_repo::get_session(&conn, &session_id)
    })();
    result.into()
}

/// 中断会话列表：状态为运行中且心跳早于调参间隔的会话。
#[tauri::command]
pub fn council_recoverable(state: State<'_, AppState>) -> CommandResult<Vec<SessionView>> {
    let conn = lock(&state);
    let stale = council_tuning::int_of(&conn, "council.stale_heartbeat_seconds").unwrap_or(120);
    let result = council_control::recoverable(&conn, stale);
    result.into()
}

/// 回音检测：结论与既有原则的重合度，达阈值的命中会留下 `echo` 提示。
#[tauri::command]
pub fn echo_check(state: State<'_, AppState>, session_id: String) -> CommandResult<Vec<EchoHit>> {
    let conn = lock(&state);
    let result = (|| {
        let Some(record_id) = network_repo::record_by_session(&conn, &session_id)? else {
            return Ok(Vec::new());
        };
        let Some(node_id) =
            network_repo::find_by_source(&conn, network_recorder::SOURCE_RECORD, &record_id)?
        else {
            return Ok(Vec::new());
        };
        council_echo::detect_echo(&conn, &node_id)
    })();
    result.into()
}

/// 撤销一条个人原则：只改状态，内容与撤销原因仍可读。
#[tauri::command]
pub fn principle_revoke(
    state: State<'_, AppState>,
    node_id: String,
    reason: Option<String>,
) -> CommandResult<bool> {
    let conn = lock(&state);
    let result = (|| {
        council_echo::revoke_principle(&conn, &node_id, reason.as_deref().unwrap_or(""))?;
        Ok(true)
    })();
    result.into()
}

/// 列出全部连接器配置。
#[tauri::command]
pub fn connector_list(state: State<'_, AppState>) -> CommandResult<Vec<ConnectorView>> {
    let conn = lock(&state);
    connector_repo::list(&conn).into()
}

/// 新增或覆盖连接器配置。MCP 服务器必须能返回工具能力声明才接受写入。
#[tauri::command]
pub fn connector_upsert(
    state: State<'_, AppState>,
    id: Option<String>,
    kind: String,
    display_name: String,
    endpoint: String,
    config: Option<serde_json::Value>,
) -> CommandResult<ConnectorView> {
    let conn = lock(&state);
    let input = ConnectorInput {
        id,
        kind,
        display_name,
        endpoint,
        config: config.unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
    };
    let result = (|| -> CoreResult<ConnectorView> {
        if input.kind.trim() == thought_forge_core::connector::KIND_MCP {
            // 只有能返回工具能力声明的服务器才准写入，拒绝把第三方运行时当工具服务器接入。
            let provider = crate::connector::tool_provider(
                &input.endpoint,
                input.id.as_deref(),
            )?;
            let tools = thought_forge_core::connector::validate_tools(&provider)?;
            let mut input = input.clone();
            input.config = serde_json::json!({ "tools": tools });
            connector_repo::upsert(&conn, &input)
        } else if input.kind.trim() == thought_forge_core::connector::KIND_SEARCH {
            let timeout_secs = crate::connector::timeout_from_db(&conn)?;
            ensure_provider_ready(&input.endpoint, |endpoint| {
                let provider = crate::connector::search_provider(
                    endpoint,
                    input.id.as_deref(),
                    timeout_secs,
                )?;
                provider.search("__probe__", 1).map(|_| ())
            })?;
            connector_repo::upsert(&conn, &input)
        } else {
            connector_repo::upsert(&conn, &input)
        }
    })();
    result.into()
}

/// 开启或关闭某个连接器。
#[tauri::command]
pub fn connector_enable(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CommandResult<ConnectorView> {
    let conn = lock(&state);
    connector_repo::set_enabled(&conn, &id, enabled).into()
}

/// 连通测试：按类型调用对应能力，并把结果写入调用审计。
///
/// 预演开启时首次调用返回待确认内容与指纹，不发起对外请求；带上同一指纹
/// 再次调用才真正测试。指纹不符返回 `E_INVALID_INPUT`，避免看过 A 却发出 B。
#[tauri::command]
pub fn connector_test(
    state: State<'_, AppState>,
    id: String,
    confirm: Option<String>,
) -> CommandResult<ConnectorTestDto> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<ConnectorTestDto> {
        let view = connector_repo::get(&conn, &id)?;
        // 检索类测试先备好要发送的内容；网页类发送的是地址本身。
        let prepared = match view.kind.as_str() {
            thought_forge_core::connector::KIND_SEARCH => {
                let mode = council_tuning::value_of(&conn, "connector.query_mode")?;
                Some(guard::prepare_query(&conn, "连接测试 检索", &mode)?)
            }
            thought_forge_core::connector::KIND_PAGE => {
                Some(guard::PreparedQuery {
                    original: view.endpoint.clone(),
                    sent: view.endpoint.clone(),
                    redacted: false,
                    mode: guard::MODE_QUESTION.to_string(),
                })
            }
            _ => None,
        };
        let fingerprint = prepared
            .as_ref()
            .map(guard::PreparedQuery::fingerprint)
            .unwrap_or_default();
        if let Some(prepared) = prepared.as_ref() {
            guard::check_confirmation(prepared, confirm.as_deref())?;
        }
        if confirm.is_none() && prepared.is_some() && council_tuning::bool_of(&conn, "connector.preflight")? {
            return Ok(ConnectorTestDto {
                pending: true,
                fingerprint,
                prepared,
                call: None,
            });
        }

        let timeout_secs = crate::connector::timeout_from_db(&conn)?;
        let started = std::time::Instant::now();
        let outcome = match view.kind.as_str() {
            thought_forge_core::connector::KIND_SEARCH => {
                let provider = crate::connector::search_provider(
                    &view.endpoint,
                    Some(&view.id),
                    timeout_secs,
                )?;
                let sent = prepared
                    .as_ref()
                    .map(|item| item.sent.clone())
                    .unwrap_or_default();
                provider
                    .search(&sent, 1)
                    .map(|hits| format!("检索可用，返回 {} 条结果", hits.len()))
            }
            thought_forge_core::connector::KIND_PAGE => {
                let provider = crate::connector::page_reader(timeout_secs)?;
                provider.read(&view.endpoint).map(|page| {
                    format!("网页可读，正文 {} 字", page.text.chars().count())
                })
            }
            thought_forge_core::connector::KIND_MCP => {
                let provider =
                    crate::connector::tool_provider(&view.endpoint, Some(&view.id))?;
                thought_forge_core::connector::validate_tools(&provider)
                    .map(|tools| format!("工具服务器可用，声明 {} 个工具", tools.len()))
            }
            other => Err(thought_forge_core::CoreError::InvalidInput(format!(
                "未知连接器类型：{other}"
            ))),
        };
        let latency = started.elapsed().as_millis() as i64;
        let (status, error_code) = match &outcome {
            Ok(_) => ("ok".to_string(), None),
            Err(error) => ("failed".to_string(), Some(error.code().to_string())),
        };
        connector_repo::record_call(
            &conn,
            &connector_repo::ConnectorCallRecord {
                connector_id: Some(view.id.clone()),
                kind: view.kind.clone(),
                purpose: thought_forge_core::connector::PURPOSE_TEST.to_string(),
                session_id: None,
                query: prepared
                    .as_ref()
                    .map(|item| item.original.clone())
                    .unwrap_or_else(|| view.endpoint.clone()),
                query_sent: prepared
                    .as_ref()
                    .map(|item| item.sent.clone())
                    .unwrap_or_default(),
                redacted: prepared.as_ref().map(|item| item.redacted).unwrap_or(false),
                result_count: if outcome.is_ok() { 1 } else { 0 },
                cost_micros: if outcome.is_ok() {
                    thought_forge_core::cost::record_connector_cost(&conn, Some(&view.id))?
                } else {
                    0
                },
                latency_ms: latency,
                status,
                error_code,
            },
        )?;
        outcome?;

        let call = connector_repo::recent_calls(&conn, 1)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                thought_forge_core::CoreError::InvalidInput(
                    "连通测试审计未能写入".to_string(),
                )
            })?;
        Ok(ConnectorTestDto {
            pending: false,
            fingerprint,
            prepared,
            call: Some(call),
        })
    })();
    result.into()
}

/// 最近的连接器调用审计。
#[tauri::command]
pub fn connector_calls(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<ConnectorCallView>> {
    let conn = lock(&state);
    connector_repo::recent_calls(&conn, limit.unwrap_or(50)).into()
}

/// 某次会诊的检索快照。
#[tauri::command]
pub fn council_sources(
    state: State<'_, AppState>,
    session_id: String,
    rotation: Option<i64>,
) -> CommandResult<Vec<thought_forge_core::council::SourceView>> {
    let conn = lock(&state);
    let result = (|| {
        let rotation = match rotation {
            Some(rotation) => rotation,
            None => council_repo::latest_rotation(&conn, &session_id)?.unwrap_or(0),
        };
        connector_service::sources(&conn, &session_id, rotation)
    })();
    result.into()
}

/// 手动检索：按当前启用的搜索连接器取一次结果，并写入调用审计。
///
/// 与连通测试一致，预演开启时首次调用只返回待确认内容与指纹，不发起对外请求。
#[tauri::command]
pub fn council_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
    confirm: Option<String>,
) -> CommandResult<SearchOutcomeDto> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<SearchOutcomeDto> {
        let connector = connector_repo::enabled_of_kind(
            &conn,
            thought_forge_core::connector::KIND_SEARCH,
        )?;
        let Some(connector) = connector else {
            return Err(thought_forge_core::CoreError::NetworkOff(
                "尚未启用任何搜索连接器".to_string(),
            ));
        };
        let mode = council_tuning::value_of(&conn, "connector.query_mode")?;
        let prepared = guard::prepare_query(&conn, &query, &mode)?;
        let fingerprint = prepared.fingerprint();
        guard::check_confirmation(&prepared, confirm.as_deref())?;
        if confirm.is_none() && council_tuning::bool_of(&conn, "connector.preflight")? {
            return Ok(SearchOutcomeDto {
                pending: true,
                fingerprint,
                prepared: Some(prepared),
                hits: Vec::new(),
            });
        }
        let timeout_secs = crate::connector::timeout_from_db(&conn)?;
        let provider = crate::connector::search_provider(
            &connector.endpoint,
            Some(&connector.id),
            timeout_secs,
        )?;
        let max = limit.unwrap_or_else(|| {
            council_tuning::int_of(&conn, "connector.max_results").unwrap_or(6)
        });
        let max = max.clamp(1, 20) as usize;
        let started = std::time::Instant::now();
        let outcome = provider.search(&prepared.sent, max);
        let latency = started.elapsed().as_millis() as i64;
        let (status, error_code, count) = match &outcome {
            Ok(hits) => ("ok", None, hits.len() as i64),
            Err(error) => ("failed", Some(error.code().to_string()), 0),
        };
        connector_repo::record_call(
            &conn,
            &connector_repo::ConnectorCallRecord {
                connector_id: Some(connector.id.clone()),
                kind: thought_forge_core::connector::KIND_SEARCH.to_string(),
                purpose: thought_forge_core::connector::PURPOSE_TEST.to_string(),
                session_id: None,
                query: prepared.original.clone(),
                query_sent: prepared.sent.clone(),
                redacted: prepared.redacted,
                result_count: count,
                cost_micros: if outcome.is_ok() {
                    thought_forge_core::cost::record_connector_cost(&conn, Some(&connector.id))?
                } else {
                    0
                },
                latency_ms: latency,
                status: status.to_string(),
                error_code,
            },
        )?;
        Ok(SearchOutcomeDto {
            pending: false,
            fingerprint,
            prepared: Some(prepared),
            hits: outcome?,
        })
    })();
    result.into()
}

/// 连接器地址预检：搜索类连接器在写入前先探测一次。
fn ensure_provider_ready(
    endpoint: &str,
    probe: impl FnOnce(&str) -> CoreResult<()>,
) -> CoreResult<()> {
    if endpoint.trim().is_empty() {
        return Err(thought_forge_core::CoreError::InvalidInput(
            "连接器地址不能为空".to_string(),
        ));
    }
    probe(endpoint)
}

#[tauri::command]
pub fn council_sessions(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<SessionView>> {
    let conn = lock(&state);
    council_repo::list_sessions(&conn, limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn council_session(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<SessionDetail> {
    let conn = lock(&state);
    council_repo::session_detail(&conn, &session_id).into()
}

#[tauri::command]
pub fn master_history(
    state: State<'_, AppState>,
    master_id: String,
    limit: Option<i64>,
) -> CommandResult<Vec<MasterHistoryEntry>> {
    let conn = lock(&state);
    council_repo::master_history(&conn, &master_id, limit.unwrap_or(50)).into()
}

/// 读取全部调参项及其当前值、默认值与允许范围。
#[tauri::command]
pub fn tuning_get(state: State<'_, AppState>) -> CommandResult<Vec<TuningItem>> {
    let conn = lock(&state);
    council_tuning::list(&conn).into()
}

/// 批量写入调参。任一项越界或标识未知时整批不落库。
#[tauri::command]
pub fn tuning_set(
    state: State<'_, AppState>,
    values: Vec<(String, String)>,
) -> CommandResult<Vec<TuningItem>> {
    let conn = lock(&state);
    council_tuning::set(&conn, &values).into()
}

fn parse_node_kind(value: &str) -> Result<NodeKind, CommandResult<NodeUpsert>> {
    NodeKind::parse(value).ok_or_else(|| {
        thought_forge_core::CoreError::InvalidInput(format!(
            "未知节点类型：{value}，只支持 idea/judgment/framework/principle/question/evidence"
        ))
        .into()
    })
}

fn parse_relation(value: &str) -> Result<Relation, CommandResult<EdgeUpsert>> {
    Relation::parse(value).ok_or_else(|| {
        thought_forge_core::CoreError::InvalidInput(format!(
            "未知关系类型：{value}，只支持 supports/conflicts/derives/analogous/applies"
        ))
        .into()
    })
}

/// 写入或合并一个认知节点。
#[tauri::command]
pub fn network_upsert_node(
    state: State<'_, AppState>,
    kind: String,
    content: String,
    source_ref: String,
    domains: Option<Vec<String>>,
    layers: Option<Vec<String>>,
) -> CommandResult<NodeUpsert> {
    let node_kind = match parse_node_kind(&kind) {
        Ok(node_kind) => node_kind,
        Err(error) => return error,
    };
    let conn = lock(&state);
    let domains = domains.unwrap_or_default();
    let layers: Vec<Layer> = layers
        .unwrap_or_default()
        .iter()
        .filter_map(|name| Layer::parse(name))
        .collect();
    network_repo::upsert_node(
        &conn,
        &network_repo::NewNode {
            kind: node_kind,
            content: &content,
            source_kind: "manual",
            source_ref: &source_ref,
            domains: &domains,
            layers: &layers,
        },
    )
    .into()
}

/// 建立或增强一条认知连线。
#[tauri::command]
pub fn network_link(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    relation: String,
    weight: Option<f64>,
) -> CommandResult<EdgeUpsert> {
    let relation = match parse_relation(&relation) {
        Ok(relation) => relation,
        Err(error) => return error,
    };
    let conn = lock(&state);
    network_repo::link_nodes(&conn, &from_id, &to_id, relation, weight.unwrap_or(0.5)).into()
}

/// 唤醒节点及其多跳邻居，跳数与衰减由调参项决定。
#[tauri::command]
pub fn network_activate(
    state: State<'_, AppState>,
    node_ids: Vec<String>,
    increment: Option<f64>,
    session_id: Option<String>,
) -> CommandResult<ActivationOutcome> {
    let conn = lock(&state);
    network_repo::activate(
        &conn,
        &node_ids,
        increment.unwrap_or(1.0),
        session_id.as_deref(),
    )
    .into()
}

/// 按半衰期衰减激活度，返回被改动的节点数。
#[tauri::command]
pub fn network_decay(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<i64> {
    let conn = lock(&state);
    network_repo::decay_all(&conn, limit.unwrap_or(MAX_CONSOLIDATION_BATCH)).into()
}

#[tauri::command]
pub fn network_node(state: State<'_, AppState>, node_id: String) -> CommandResult<NodeDetail> {
    let conn = lock(&state);
    network_repo::get_node(&conn, &node_id).into()
}

/// 按领域、层次、类型与激活度过滤图谱。
#[tauri::command]
pub fn network_graph(
    state: State<'_, AppState>,
    domain: Option<String>,
    layer: Option<String>,
    kind: Option<String>,
    cluster_id: Option<String>,
    min_activation: Option<f64>,
    limit: Option<i64>,
) -> CommandResult<GraphView> {
    let conn = lock(&state);
    let filter = GraphFilter {
        domain,
        layer: layer.as_deref().and_then(Layer::parse),
        kind: kind.as_deref().and_then(NodeKind::parse),
        cluster_id,
        min_activation,
        limit,
    };
    network_repo::get_graph(&conn, &filter).into()
}

/// 裁决冲突连线并留档。
#[tauri::command]
pub fn network_resolve_conflict(
    state: State<'_, AppState>,
    edge_id: String,
    decision: String,
    reason: Option<String>,
) -> CommandResult<String> {
    let conn = lock(&state);
    network_repo::resolve_conflict(&conn, &edge_id, &decision, reason.as_deref().unwrap_or("")).into()
}

/// 把一次已收敛的会诊写入思维网络。
#[tauri::command]
pub fn network_record_session(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<RecordOutcome> {
    let conn = lock(&state);
    network_recorder::record_session(&conn, &session_id).into()
}

#[tauri::command]
pub fn records_list(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<Vec<ThoughtRecordView>> {
    let conn = lock(&state);
    network_repo::list_records(&conn, limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn records_compare(state: State<'_, AppState>, topic_key: String) -> CommandResult<Vec<ThoughtRecordView>> {
    let conn = lock(&state);
    network_repo::compare_records(&conn, &topic_key).into()
}

#[tauri::command]
pub fn record_decision(
    state: State<'_, AppState>,
    record_id: String,
    adopted: bool,
    reason: Option<String>,
) -> CommandResult<bool> {
    let conn = lock(&state);
    match network_repo::mark_record_decision(&conn, &record_id, adopted, reason.as_deref().unwrap_or("")) {
        Ok(()) => CommandResult::ok(true),
        Err(error) => error.into(),
    }
}

/// 触发一次固化：强化共激活连线、衰减陈旧连线、合并相似节点、识别冲突。
#[tauri::command]
pub fn consolidate_trigger(
    state: State<'_, AppState>,
    mode: Option<String>,
) -> CommandResult<ConsolidationReport> {
    let conn = lock(&state);
    consolidate::trigger_consolidation(&conn, mode.as_deref().unwrap_or("manual")).into()
}

#[tauri::command]
pub fn consolidate_report(
    state: State<'_, AppState>,
    run_id: String,
) -> CommandResult<ConsolidationReport> {
    let conn = lock(&state);
    consolidate::get_consolidation_report(&conn, &run_id).into()
}

#[tauri::command]
pub fn consolidate_runs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<ConsolidationRunView>> {
    let conn = lock(&state);
    consolidate::list_consolidation_runs(&conn, limit.unwrap_or(20)).into()
}

// ---------- P5 主动助理 ----------

#[tauri::command]
pub fn companion_settings(state: State<'_, AppState>) -> CommandResult<CompanionSettings> {
    let conn = lock(&state);
    companion_repo::get_settings(&conn).into()
}

/// 主动助学总开关。关闭后不产生任何新的主动推送。
#[tauri::command]
pub fn companion_enable(
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<CompanionSettings> {
    let conn = lock(&state);
    companion_repo::set_enabled(&conn, enabled).into()
}

#[tauri::command]
pub fn companion_limit(
    state: State<'_, AppState>,
    limit: i64,
) -> CommandResult<CompanionSettings> {
    let conn = lock(&state);
    companion_repo::set_daily_limit(&conn, limit).into()
}

#[tauri::command]
pub fn companion_rules(
    state: State<'_, AppState>,
    rules: CompanionRules,
) -> CommandResult<CompanionSettings> {
    let conn = lock(&state);
    companion_repo::set_rules(&conn, &rules).into()
}

/// 对一条新信号做一次轻量碰撞。关闭、规则未命中或超限时静默跳过。
#[tauri::command]
pub fn companion_collide(
    state: State<'_, AppState>,
    signal: CollisionSignal,
) -> CommandResult<CollisionOutcome> {
    let conn = lock(&state);
    let enabled = match networking_enabled(&conn) {
        Ok(enabled) => enabled,
        Err(error) => return error.into(),
    };
    let inner: Box<dyn ModelClient> = if enabled {
        match platform_repo::enabled(&conn) {
            Ok(Some(view)) => match model::build_client(&view) {
                Ok(client) => Box::new(client),
                Err(error) => Box::new(BlockedClient::unavailable(error.to_string())),
            },
            Ok(None) => Box::new(BlockedClient::unavailable("尚未启用任何模型平台")),
            Err(error) => return error.into(),
        }
    } else {
        Box::new(BlockedClient::network_off("联网能力已关闭"))
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    companion_collide_mod::collide(&conn, &client, &signal, &RetryPolicy::default()).into()
}

#[tauri::command]
pub fn insights_list(
    state: State<'_, AppState>,
    kind: Option<String>,
    status: Option<String>,
    source: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<Insight>> {
    let conn = lock(&state);
    let filter = InsightFilter {
        kind: kind.as_deref().and_then(InsightKind::parse),
        status,
        source,
    };
    companion_repo::list_insights(&conn, &filter, limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn insight_mark(
    state: State<'_, AppState>,
    insight_id: String,
    action: String,
    reason: Option<String>,
) -> CommandResult<Insight> {
    let conn = lock(&state);
    companion_repo::mark_insight(&conn, &insight_id, &action, reason.as_deref().unwrap_or("")).into()
}

/// 以洞察为议题新建一次会诊，并把洞察标记为已转会诊。
#[tauri::command]
pub fn insight_convert(
    state: State<'_, AppState>,
    insight_id: String,
) -> CommandResult<SessionView> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<SessionView> {
        let insight = companion_repo::get_insight(&conn, &insight_id)?;
        let question = if insight.summary.trim().is_empty() {
            insight.title.clone()
        } else {
            format!("{}：{}", insight.title, insight.summary)
        };

        let mut domains: Vec<String> = Vec::new();
        let mut layers: Vec<Layer> = Vec::new();
        for node_id in &insight.related_node_ids {
            if let Ok(detail) = network_repo::get_node(&conn, node_id) {
                for domain in detail.node.domains {
                    if !domains.contains(&domain) {
                        domains.push(domain);
                    }
                }
                for layer in detail.node.layers {
                    if !layers.contains(&layer) {
                        layers.push(layer);
                    }
                }
            }
        }
        if layers.is_empty() {
            layers = LAYER_ORDER.to_vec();
        }

        let id = council_repo::create_session(&conn, &question, &domains, &layers, Strategy::Steady)?;
        companion_repo::mark_insight(&conn, &insight_id, "convert", "已转为会诊")?;
        council_repo::get_session(&conn, &id)
    })();
    result.into()
}

#[tauri::command]
pub fn topics_list(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<Vec<TopicView>> {
    let conn = lock(&state);
    companion_growth::list_topics(&conn, limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn principles_list(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<PrincipleSeal>> {
    let conn = lock(&state);
    companion_growth::list_principles(&conn, limit.unwrap_or(50)).into()
}

/// 把连续采纳达到阈值的主题提升为个人原则。
#[tauri::command]
pub fn promote_principles(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let conn = lock(&state);
    companion_growth::promote_principles(&conn).into()
}

#[tauri::command]
pub fn ring_overview(state: State<'_, AppState>) -> CommandResult<RingOverview> {
    let conn = lock(&state);
    companion_growth::ring_overview(&conn).into()
}

// ---------- P6 蒸馏流水线 ----------

/// 统一按联网与平台配置组装受控模型客户端。联网关闭时返回拦截客户端。
fn gated_model_client(
    conn: &rusqlite::Connection,
) -> CoreResult<(bool, Box<dyn ModelClient>)> {
    let enabled = networking_enabled(conn)?;
    let inner: Box<dyn ModelClient> = if enabled {
        match platform_repo::enabled(conn)? {
            Some(view) => match model::build_client(&view) {
                Ok(client) => Box::new(client),
                Err(error) => Box::new(BlockedClient::unavailable(error.to_string())),
            },
            None => Box::new(BlockedClient::unavailable("尚未启用任何模型平台")),
        }
    } else {
        Box::new(BlockedClient::network_off("联网能力已关闭"))
    };
    Ok((enabled, inner))
}

fn default_pack_dir(state: &State<'_, AppState>) -> PathBuf {
    state
        .db_path
        .parent()
        .map(|dir| dir.join("packs"))
        .unwrap_or_else(|| PathBuf::from("packs"))
}

/// 直接投喂材料并启动蒸馏。骨架确认后会停在阶段0之后。
#[tauri::command]
pub fn distill_start(
    state: State<'_, AppState>,
    master_id: String,
    master_name: String,
    domain: String,
    materials: Vec<IntakeMaterial>,
    output_dir: Option<String>,
) -> CommandResult<DistillJobView> {
    let mut conn = lock(&state);
    let (enabled, inner) = match gated_model_client(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    let negative = match intake_service::negative_evidence(&conn, &master_id) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let output = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_pack_dir(&state));
    let input = distill_pipeline::DistillInput {
        master_id,
        master_name,
        domain,
        source_kind: "manual".to_string(),
        source_ref: "manual-intake".to_string(),
        materials,
        output_dir: output,
        negative,
    };
    distill_pipeline::start(&mut conn, &client, &RetryPolicy::default(), &input).into()
}

/// 从已确认的入库任务启动蒸馏，只使用被确认的材料。
#[tauri::command]
pub fn distill_from_intake(
    state: State<'_, AppState>,
    intake_job_id: String,
    output_dir: Option<String>,
) -> CommandResult<DistillJobView> {
    let mut conn = lock(&state);
    let (enabled, inner) = match gated_model_client(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    let result = (|| -> CoreResult<DistillJobView> {
        let intake_job = distill_repo::get_intake(&conn, &intake_job_id)?;
        let materials = intake_service::accepted_materials(&conn, &intake_job_id)?;
        if materials.is_empty() {
            return Err(thought_forge_core::CoreError::InvalidInput(
                "该入库任务没有已确认的材料".to_string(),
            ));
        }
        let negative = intake_service::negative_evidence(&conn, &intake_job.master_ref)?;
        let output = output_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| default_pack_dir(&state));
        let input = distill_pipeline::DistillInput {
            master_id: intake_job.master_ref,
            master_name: intake_job.master_name,
            domain: intake_job.domain,
            source_kind: intake_job.mode,
            source_ref: intake_job_id,
            materials,
            output_dir: output,
            negative,
        };
        distill_pipeline::start(&mut conn, &client, &RetryPolicy::default(), &input)
    })();
    result.into()
}

#[tauri::command]
pub fn distill_list(
    state: State<'_, AppState>,
    status: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<DistillJobView>> {
    let conn = lock(&state);
    distill_repo::list_jobs(&conn, status.as_deref(), limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn distill_detail(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<DistillDetail> {
    let conn = lock(&state);
    distill_repo::get_detail(&conn, &job_id).into()
}

/// 确认骨架，继续五路提取。
#[tauri::command]
pub fn distill_confirm(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<DistillJobView> {
    let mut conn = lock(&state);
    let (enabled, inner) = match gated_model_client(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    distill_pipeline::confirm_skeleton(&mut conn, &client, &RetryPolicy::default(), &job_id).into()
}

/// 从失败的检查点续跑，已完成阶段不重复执行。
#[tauri::command]
pub fn distill_resume(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<DistillJobView> {
    let mut conn = lock(&state);
    let (enabled, inner) = match gated_model_client(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    distill_pipeline::resume(&mut conn, &client, &RetryPolicy::default(), &job_id).into()
}

// ---------- P6 入库通道 ----------

/// 手动投喂：材料直接进入队列并标记为已确认。
#[tauri::command]
pub fn intake_create(
    state: State<'_, AppState>,
    master_id: String,
    master_name: String,
    domain: String,
    materials: Vec<IntakeMaterial>,
) -> CommandResult<IntakeJobView> {
    let conn = lock(&state);
    intake_service::create_manual(
        &conn,
        &ManualIntakeInput {
            master_id,
            master_name,
            domain,
            materials,
        },
    )
    .into()
}

#[tauri::command]
pub fn intake_list(
    state: State<'_, AppState>,
    status: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<IntakeJobView>> {
    let conn = lock(&state);
    distill_repo::list_intake(&conn, status.as_deref(), limit.unwrap_or(50)).into()
}

#[tauri::command]
pub fn intake_preview(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<Vec<SignalView>> {
    let conn = lock(&state);
    intake_service::preview_materials(&conn, &job_id).into()
}

/// 逐批确认主动搜集到的资料。只有被确认的材料才会进入蒸馏。
#[tauri::command]
pub fn intake_confirm(
    state: State<'_, AppState>,
    job_id: String,
    accepted_ids: Vec<String>,
    rejected_ids: Vec<String>,
    reason: Option<String>,
) -> CommandResult<IntakeJobView> {
    let conn = lock(&state);
    intake_service::confirm_materials(
        &conn,
        &job_id,
        &accepted_ids,
        &rejected_ids,
        reason.as_deref().unwrap_or(""),
    )
    .into()
}

#[tauri::command]
pub fn discovery_settings(state: State<'_, AppState>) -> CommandResult<DiscoverySettings> {
    let conn = lock(&state);
    distill_repo::get_discovery_settings(&conn).into()
}

/// 主动搜集总开关。默认关闭，关闭时不发起任何外部检索。
#[tauri::command]
pub fn discovery_enable(
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<DiscoverySettings> {
    let conn = lock(&state);
    distill_repo::set_discovery_enabled(&conn, enabled).into()
}

#[tauri::command]
pub fn discovery_schedule(
    state: State<'_, AppState>,
    schedule: serde_json::Value,
) -> CommandResult<DiscoverySettings> {
    let conn = lock(&state);
    distill_repo::set_discovery_schedule(&conn, &schedule).into()
}

/// 触发一次主动搜集。关闭时直接返回未触发，不产生外部请求。
#[tauri::command]
pub fn discovery_run(
    state: State<'_, AppState>,
    master_id: String,
    master_name: String,
    domain: String,
    query: Option<String>,
) -> CommandResult<DiscoveryOutcome> {
    let conn = lock(&state);
    let enabled = match networking_enabled(&conn) {
        Ok(enabled) => enabled,
        Err(error) => return error.into(),
    };
    let shell = match ShellConnector::from_db(&conn, enabled) {
        Ok(shell) => shell,
        Err(error) => return error.into(),
    };

    // 主动搜集同样是外部请求：拿不到检索能力时明确报因，不静默返回空清单。
    let Some(provider) = shell.search() else {
        let offline = discovery::OfflineDiscovery::new(if enabled {
            "尚未启用检索连接器"
        } else {
            "联网能力已关闭，先在设置页开启检索连接器"
        });
        return intake_service::run_discovery(
            &conn,
            &offline,
            &master_id,
            &master_name,
            &domain,
            query.as_deref(),
        )
        .into();
    };

    let max_results = match council_tuning::int_of(&conn, "connector.max_results") {
        Ok(value) => value.clamp(1, 20) as usize,
        Err(error) => return error.into(),
    };
    let mode = match council_tuning::value_of(&conn, "connector.query_mode") {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = discovery::ShellDiscovery {
        conn: &conn,
        search: provider,
        max_results,
        mode,
    };
    intake_service::run_discovery(
        &conn,
        &client,
        &master_id,
        &master_name,
        &domain,
        query.as_deref(),
    )
    .into()
}

// ---------- P7 采集台 ----------

#[tauri::command]
pub fn capture_settings(state: State<'_, AppState>) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    capture_pipeline::settings_view(&conn, &state.capture.unavailable()).into()
}

/// 逐项开启或关闭采集能力，每次切换都写审计。
#[tauri::command]
pub fn capture_set_capability(
    state: State<'_, AppState>,
    kind: String,
    enabled: bool,
) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    capture_pipeline::set_capability(&conn, &kind, enabled, &state.capture.unavailable()).into()
}

/// 全局暂停或恢复。暂停期间采集链路不轮询、不写入。
#[tauri::command]
pub fn capture_set_paused(
    state: State<'_, AppState>,
    paused: bool,
) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    capture_pipeline::set_paused(&conn, paused, &state.capture.unavailable()).into()
}

#[tauri::command]
pub fn capture_set_redaction(
    state: State<'_, AppState>,
    enabled: bool,
    terms: Vec<String>,
    mask: Option<String>,
) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    let rules = RedactionRules {
        enabled,
        terms,
        mask: mask.unwrap_or_else(|| thought_forge_core::capture::redact::DEFAULT_MASK.to_string()),
    };
    let result: CoreResult<()> = capture_repo::set_redaction_rules(&conn, &rules);
    if let Err(error) = result {
        return error.into();
    }
    capture_pipeline::settings_view(&conn, &state.capture.unavailable()).into()
}

#[tauri::command]
pub fn capture_set_dedup(
    state: State<'_, AppState>,
    seconds: i64,
) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    let result: CoreResult<()> = capture_repo::set_dedup_seconds(&conn, seconds);
    if let Err(error) = result {
        return error.into();
    }
    capture_pipeline::settings_view(&conn, &state.capture.unavailable()).into()
}

/// 设置受关注目录，并立即重建文件监听，不需要重启应用。
#[tauri::command]
pub fn capture_set_watch_roots(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CommandResult<CaptureSettingsView> {
    let conn = lock(&state);
    let roots = match capture_pipeline::normalize_watch_roots(&paths) {
        Ok(roots) => roots,
        Err(error) => return error.into(),
    };
    // 先换监听再落盘：建立监听会因权限或句柄耗尽失败，那时设置值保持不动，
    // 界面显示的就仍是实际生效的目录。
    let applied: Vec<std::path::PathBuf> = roots.iter().map(std::path::PathBuf::from).collect();
    if let Err(error) = state.capture.set_watch_roots(applied) {
        return error.into();
    }
    if let Err(error) = capture_repo::set_watch_roots(&conn, &roots) {
        return error.into();
    }
    capture_pipeline::settings_view(&conn, &state.capture.unavailable()).into()
}

/// 触发一轮采集：剪贴板、前台窗口与受关注目录的文件活动。
#[tauri::command]
pub fn capture_collect(state: State<'_, AppState>) -> CommandResult<CaptureOutcome> {
    let mut conn = lock(&state);
    let now = match capture_repo::now(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    capture_pipeline::collect_once(&mut conn, &state.capture, &now).into()
}

#[tauri::command]
pub fn capture_events(
    state: State<'_, AppState>,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<CaptureEventView>> {
    let conn = lock(&state);
    capture_pipeline::list_events(
        &conn,
        &CaptureFilter {
            kind,
            from,
            to,
            limit,
        },
    )
    .into()
}

#[tauri::command]
pub fn capture_summaries(
    state: State<'_, AppState>,
    event_id: String,
) -> CommandResult<Vec<CaptureSummaryView>> {
    let conn = lock(&state);
    capture_pipeline::summaries_of(&conn, &event_id).into()
}

/// 删除一条采集记录及其派生摘要。
#[tauri::command]
pub fn capture_delete_event(
    state: State<'_, AppState>,
    event_id: String,
) -> CommandResult<bool> {
    let conn = lock(&state);
    capture_pipeline::delete_event(&conn, &event_id).into()
}

#[tauri::command]
pub fn capture_audit(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CommandResult<Vec<CaptureAuditView>> {
    let conn = lock(&state);
    capture_pipeline::list_audit(&conn, limit.unwrap_or(50)).into()
}

// ---------- P7 知识地形 ----------

#[tauri::command]
pub fn kb_sources(state: State<'_, AppState>) -> CommandResult<Vec<KbSourceView>> {
    let conn = lock(&state);
    kb_service::list_sources(&conn).into()
}

#[tauri::command]
pub fn kb_add_source(state: State<'_, AppState>, path: String) -> CommandResult<KbSourceView> {
    let conn = lock(&state);
    kb_service::add_source(&conn, &path).into()
}

#[tauri::command]
pub fn kb_remove_source(state: State<'_, AppState>, source_id: String) -> CommandResult<bool> {
    let conn = lock(&state);
    kb_service::remove_source(&conn, &source_id).into()
}

/// 扫描一个来源；不传来源时扫描全部。
#[tauri::command]
pub fn kb_scan(
    state: State<'_, AppState>,
    source_id: Option<String>,
) -> CommandResult<Vec<KbScanOutcome>> {
    let mut conn = lock(&state);
    match source_id.as_deref() {
        Some(id) => kb_service::scan_source(&mut conn, id)
            .map(|outcome| vec![outcome])
            .into(),
        None => kb_service::scan_all(&mut conn).into(),
    }
}

#[tauri::command]
pub fn kb_documents(
    state: State<'_, AppState>,
    source_id: Option<String>,
    domain: Option<String>,
    topic_id: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<KbDocumentView>> {
    let conn = lock(&state);
    kb_service::list_documents(
        &conn,
        &KbFilter {
            source_id,
            domain,
            topic_id,
            limit,
        },
    )
    .into()
}

#[tauri::command]
pub fn kb_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> CommandResult<Vec<KbSearchHit>> {
    let conn = lock(&state);
    kb_service::search(&conn, &query, limit).into()
}

#[tauri::command]
pub fn kb_overview(
    state: State<'_, AppState>,
    topic_limit: Option<i64>,
) -> CommandResult<KnowledgeOverview> {
    let conn = lock(&state);
    kb_service::overview(&conn, topic_limit.unwrap_or(24)).into()
}

// ---------- P8 自我蒸馏 ----------

/// 铜镜入口的就绪度：记录数量、是否已安装「你」、会诊席位开关。
#[tauri::command]
pub fn self_readiness(state: State<'_, AppState>) -> CommandResult<SelfReadiness> {
    let conn = lock(&state);
    self_service::readiness(&conn).into()
}

/// 读取最近一次自我蒸馏草稿；传入草稿号时读取指定草稿。
#[tauri::command]
pub fn self_draft(
    state: State<'_, AppState>,
    draft_id: Option<String>,
) -> CommandResult<Option<SelfDraftDetail>> {
    let conn = lock(&state);
    match draft_id {
        Some(id) => self_service::detail(&conn, &id).map(Some).into(),
        None => self_service::latest_detail(&conn).into(),
    }
}

/// 发起自我蒸馏：读取历史记录并生成待确认初稿。
#[tauri::command]
pub fn self_start(state: State<'_, AppState>) -> CommandResult<SelfDraftDetail> {
    let mut conn = lock(&state);
    let (enabled, inner) = match gated_model_client(&conn) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    let client = GatedClient {
        enabled,
        inner: inner.as_ref(),
    };
    self_service::start(&mut conn, &client, &RetryPolicy::default()).into()
}

/// 逐条确认某条判断框架，采纳或剔除。
#[tauri::command]
pub fn self_decide(
    state: State<'_, AppState>,
    draft_id: String,
    item_id: String,
    accepted: bool,
) -> CommandResult<SelfDraftDetail> {
    let conn = lock(&state);
    self_service::decide(&conn, &draft_id, &item_id, accepted).into()
}

/// 安装用户本人大师包。要求所有条目已确认且至少采纳一条。
#[tauri::command]
pub fn self_install(
    state: State<'_, AppState>,
    draft_id: String,
) -> CommandResult<InstallOutcome> {
    let mut conn = lock(&state);
    let output = default_pack_dir(&state);
    self_service::install(&mut conn, &draft_id, &output).into()
}

/// 开关「你」在会诊中的保留席位。
#[tauri::command]
pub fn self_set_seat(
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<SelfReadiness> {
    let conn = lock(&state);
    self_service::set_seat_enabled(&conn, enabled).into()
}

// ---------- P8 数据主权 ----------

#[tauri::command]
pub fn data_scope(state: State<'_, AppState>) -> CommandResult<DataScope> {
    let conn = lock(&state);
    data_service::scope(&conn).into()
}

/// 导出全部本地数据。不指定路径时写到应用数据目录的 exports 下。
#[tauri::command]
pub fn data_export(
    state: State<'_, AppState>,
    path: Option<String>,
) -> CommandResult<ExportOutcome> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<ExportOutcome> {
        let dest = match path {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => {
                let stamp = data_service::now(&conn)?.replace(':', "-");
                let base = state
                    .db_path
                    .parent()
                    .map(|dir| dir.join("exports"))
                    .unwrap_or_else(|| PathBuf::from("exports"));
                base.join(format!("forge-archive-{stamp}.json"))
            }
        };
        data_service::export(&conn, &dest)
    })();
    result.into()
}

/// 清除全部本地数据。必须显式传入 confirm 为真。
#[tauri::command]
pub fn data_purge(
    state: State<'_, AppState>,
    confirm: Option<bool>,
) -> CommandResult<PurgeOutcome> {
    let mut conn = lock(&state);
    let result = (|| -> CoreResult<PurgeOutcome> {
        if confirm != Some(true) {
            return Err(thought_forge_core::CoreError::InvalidInput(
                "清除数据需要显式确认".to_string(),
            ));
        }
        data_service::purge(&mut conn, thought_forge_core::data::SCOPE_ALL)
    })();
    result.into()
}

#[tauri::command]
pub fn data_events(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<Vec<DataEventView>> {
    let conn = lock(&state);
    data_service::events(&conn, limit.unwrap_or(50)).into()
}

// ---------- P9 资产统计 ----------

#[tauri::command]
pub fn asset_roots(state: State<'_, AppState>) -> CommandResult<Vec<AssetRootView>> {
    let conn = lock(&state);
    asset_service::list_roots(&conn).into()
}

#[tauri::command]
pub fn asset_add_root(state: State<'_, AppState>, path: String) -> CommandResult<AssetRootView> {
    let conn = lock(&state);
    asset_service::add_root(&conn, &path).into()
}

#[tauri::command]
pub fn asset_remove_root(state: State<'_, AppState>, root_id: String) -> CommandResult<bool> {
    let conn = lock(&state);
    asset_service::remove_root(&conn, &root_id).into()
}

/// 扫描一个根目录；不传根目录时扫描全部。
#[tauri::command]
pub fn asset_scan(
    state: State<'_, AppState>,
    root_id: Option<String>,
) -> CommandResult<Vec<AssetScanOutcome>> {
    let mut conn = lock(&state);
    asset_service::scan_assets(&mut conn, root_id.as_deref()).into()
}

#[tauri::command]
// 参数即前端 `asset_skills` 的筛选字段，合并成结构体会改变 IPC 形状。
#[allow(clippy::too_many_arguments)]
pub fn asset_skills(
    state: State<'_, AppState>,
    root_id: Option<String>,
    category: Option<String>,
    tag: Option<String>,
    enabled: Option<bool>,
    needs_repair: Option<bool>,
    query: Option<String>,
    limit: Option<i64>,
) -> CommandResult<Vec<SkillView>> {
    let conn = lock(&state);
    asset_service::list_skills(
        &conn,
        &SkillFilter {
            root_id,
            category,
            tag,
            enabled,
            needs_repair,
            query,
            limit,
        },
    )
    .into()
}

#[tauri::command]
pub fn asset_skill_detail(state: State<'_, AppState>, skill_id: String) -> CommandResult<SkillDetail> {
    let conn = lock(&state);
    asset_service::skill_detail(&conn, &skill_id).into()
}

#[tauri::command]
pub fn asset_summary(state: State<'_, AppState>) -> CommandResult<AssetSummary> {
    let conn = lock(&state);
    asset_service::summary(&conn).into()
}

// ---------- P14 成本、凭据与备份 ----------

/// 备份文件默认放在数据库同级的 `backups` 目录。
fn backup_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(|dir| dir.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

/// 最近若干天的费用汇总，带当日与当月累计、上限与策略。
#[tauri::command]
pub fn cost_summary(state: State<'_, AppState>, days: Option<i64>) -> CommandResult<CostSummary> {
    let conn = lock(&state);
    cost::cost_summary(&conn, days.unwrap_or(30)).into()
}

/// 按席位数、轮次上限与检索次数估算一次会诊的调用次数与费用。
#[tauri::command]
pub fn cost_estimate(
    state: State<'_, AppState>,
    seats: Option<i64>,
    rounds: Option<i64>,
    searches: Option<i64>,
) -> CommandResult<CostEstimate> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<CostEstimate> {
        let seats = seats
            .filter(|value| *value > 0)
            .unwrap_or(thought_forge_core::council::DEFAULT_PANEL_SIZE as i64);
        let rounds = match rounds.filter(|value| *value > 0) {
            Some(rounds) => rounds,
            None => council_tuning::int_of(&conn, "council.max_rounds")?,
        };
        let searches = searches.unwrap_or(seats + 1);
        cost::estimate(&conn, seats, rounds, searches)
    })();
    result.into()
}

/// 创建一份备份，并按保留份数与快照保留天数做清理。
#[tauri::command]
pub fn backup_create(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> CommandResult<BackupOutcome> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<BackupOutcome> {
        let dir = backup_dir(&state);
        let kind = kind.unwrap_or_else(|| backup::KIND_MANUAL.to_string());
        let outcome = backup::create(&conn, &dir, &kind)?;
        let keep = council_tuning::int_of(&conn, "backup.keep_count")?;
        backup::prune(&conn, keep)?;
        let retain = council_tuning::int_of(&conn, "snapshot.retain_days")?;
        backup::prune_snapshots(&conn, retain)?;
        Ok(outcome)
    })();
    result.into()
}

#[tauri::command]
pub fn backup_list(state: State<'_, AppState>, limit: Option<i64>) -> CommandResult<Vec<BackupView>> {
    let conn = lock(&state);
    backup::list(&conn, limit.unwrap_or(50)).into()
}

/// 恢复前先校验完整性，校验通过后才替换数据文件；应用需重启以加载恢复后的数据。
#[tauri::command]
pub fn backup_restore(state: State<'_, AppState>, path: String) -> CommandResult<BackupOutcome> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<BackupOutcome> {
        let source = PathBuf::from(&path);
        let outcome = backup::restore_prepare(&conn, &source)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        std::fs::copy(&source, &state.db_path)?;
        Ok(outcome)
    })();
    result.into()
}

/// 写入系统凭据库并登记引用名。
#[tauri::command]
pub fn credential_set(
    state: State<'_, AppState>,
    scope: String,
    owner_id: String,
    secret: String,
) -> CommandResult<CredentialRefView> {
    let conn = lock(&state);
    let store = crate::credential::ShellCredentialStore::new();
    credential::set(&conn, &store, &scope, &owner_id, &secret).into()
}

/// 查询某个归属的密钥是否已写入系统凭据库。
#[tauri::command]
pub fn credential_status(
    state: State<'_, AppState>,
    scope: String,
    owner_id: String,
) -> CommandResult<bool> {
    let conn = lock(&state);
    let result = (|| -> CoreResult<bool> {
        let ref_name = credential::ref_name_for(&scope, &owner_id)?;
        credential::status(
            &conn,
            &crate::credential::ShellCredentialStore::new(),
            &ref_name,
        )
    })();
    result.into()
}
