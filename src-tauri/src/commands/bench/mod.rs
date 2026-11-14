//! In-app benchmark page backend: preset-driven timing of the primitives every
//! automation depends on (ADB, frame capture, OCR, template matching).
//!
//! Results never feed automation decisions; this module only measures and
//! reports so a recognition change can be compared before/after.

mod cases;
#[cfg(test)]
mod tests;

use crate::commands::debug::{self, DebugSidecar};
use crate::commands::settings::AdbDeviceSettings;
use crate::craft_essence_enhancement_runner::CraftEssenceEnhancementRunnerHandle;
use crate::enhancement_runner::EnhancementRunnerHandle;
use crate::friend_point_summon_runner::FriendPointSummonRunnerHandle;
use crate::runner::RunnerHandle;
use crate::screen::SidecarClient;
use crate::server::Server;
use cases::{case_by_id, preset_by_id, preset_contains_device_input, BenchCtx, CaseSpec, CASES, PRESETS};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub(crate) const EVENT_NAME: &str = "bench-run-event";
const MAX_RUNS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BenchCategory {
    Device,
    Capture,
    Ocr,
    Template,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BenchSource {
    Device,
    LiveFrame,
    LastCapture,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchCaseInfo {
    pub(crate) id: &'static str,
    pub(crate) category: BenchCategory,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) source: BenchSource,
    pub(crate) device_input: bool,
    pub(crate) iterations: u32,
    pub(crate) budget_ms: Option<f64>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchPresetInfo {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) case_ids: Vec<&'static str>,
    pub(crate) device_input: bool,
    pub(crate) estimated_seconds: u32,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchCaseReport {
    pub(crate) case_id: String,
    pub(crate) name: String,
    pub(crate) source: BenchSource,
    pub(crate) samples_ms: Vec<f64>,
    pub(crate) min_ms: f64,
    pub(crate) median_ms: f64,
    pub(crate) p95_ms: f64,
    pub(crate) budget_ms: Option<f64>,
    pub(crate) passed: Option<bool>,
    pub(crate) skipped: Option<String>,
    pub(crate) note: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchRunSummary {
    pub(crate) run_id: String,
    pub(crate) preset_id: Option<String>,
    pub(crate) preset_name: String,
    pub(crate) started_at_ms: u64,
    pub(crate) finished_at_ms: u64,
    pub(crate) device_label: String,
    pub(crate) server: String,
    pub(crate) reports: Vec<BenchCaseReport>,
}

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchStatus {
    pub(crate) running: bool,
    pub(crate) current_case: Option<String>,
    pub(crate) completed: u32,
    pub(crate) total: u32,
}

#[derive(Clone, Default)]
pub(crate) struct BenchState {
    status: Arc<Mutex<BenchStatus>>,
    runs: Arc<Mutex<Vec<BenchRunSummary>>>,
    cancel: Arc<AtomicBool>,
}

impl BenchState {
    fn snapshot(&self) -> BenchStatus {
        self.status.lock().map(|status| status.clone()).unwrap_or_default()
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchEvent {
    status: BenchStatus,
    report: Option<BenchCaseReport>,
    summary: Option<BenchRunSummary>,
    error: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn emit(
    app: &AppHandle,
    state: &BenchState,
    report: Option<BenchCaseReport>,
    summary: Option<BenchRunSummary>,
    error: Option<String>,
) {
    let payload = BenchEvent {
        status: state.snapshot(),
        report,
        summary,
        error,
    };
    let _ = app.emit(EVENT_NAME, payload);
}

#[tauri::command]
pub(crate) fn bench_list_cases() -> Vec<BenchCaseInfo> {
    CASES
        .iter()
        .map(|case| BenchCaseInfo {
            id: case.id,
            category: case.category,
            name: case.name,
            description: case.description,
            source: case.source,
            device_input: case.device_input,
            iterations: case.iterations,
            budget_ms: case.budget_ms,
        })
        .collect()
}

#[tauri::command]
pub(crate) fn bench_list_presets() -> Vec<BenchPresetInfo> {
    PRESETS
        .iter()
        .map(|preset| BenchPresetInfo {
            id: preset.id,
            name: preset.name,
            description: preset.description,
            case_ids: preset.case_ids.to_vec(),
            device_input: preset_contains_device_input(preset),
            estimated_seconds: preset.estimated_seconds,
        })
        .collect()
}

#[tauri::command]
pub(crate) fn bench_get_status(state: tauri::State<'_, BenchState>) -> BenchStatus {
    state.snapshot()
}

#[tauri::command]
pub(crate) fn bench_get_runs(state: tauri::State<'_, BenchState>) -> Vec<BenchRunSummary> {
    state.runs.lock().map(|runs| runs.clone()).unwrap_or_default()
}

#[tauri::command]
pub(crate) fn bench_clear_runs(state: tauri::State<'_, BenchState>) {
    if let Ok(mut runs) = state.runs.lock() {
        runs.clear();
    }
}

#[tauri::command]
pub(crate) fn bench_cancel(state: tauri::State<'_, BenchState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn bench_run_preset(
    app: AppHandle,
    preset_id: String,
    allow_device_input: bool,
    bench_state: tauri::State<'_, BenchState>,
    debug_state: tauri::State<'_, DebugSidecar>,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    if bench_state.snapshot().running {
        return Err("测试任务正在运行中".into());
    }
    let Some(preset) = preset_by_id(&preset_id) else {
        return Err(format!("未知的测试预设：{preset_id}"));
    };
    debug::require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let specs: Vec<&'static CaseSpec> = preset
        .case_ids
        .iter()
        .filter_map(|id| case_by_id(*id))
        .collect();
    if specs.is_empty() {
        return Err("该预设没有可用用例".into());
    }

    let serial = adb_settings_state
        .lock()
        .map_err(|_| "ADB 设置锁异常".to_string())?
        .selected_adb_serial
        .clone();
    let server = *server_state.lock().map_err(|_| "服务器设置锁异常".to_string())?;

    let state = bench_state.inner().clone();
    state.cancel.store(false, Ordering::Relaxed);
    {
        let mut status = state
            .status
            .lock()
            .map_err(|_| "测试状态锁异常".to_string())?;
        *status = BenchStatus {
            running: true,
            current_case: None,
            completed: 0,
            total: specs.len() as u32,
        };
    }

    let sidecar = debug_state.0.clone();
    let preset_name = preset.name.to_string();
    std::thread::spawn(move || {
        run_preset(
            app,
            preset.id.to_string(),
            preset_name,
            specs,
            state,
            sidecar,
            server,
            serial,
            allow_device_input,
        );
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_preset(
    app: AppHandle,
    preset_id: String,
    preset_name: String,
    specs: Vec<&'static CaseSpec>,
    state: BenchState,
    sidecar: Arc<Mutex<Option<SidecarClient>>>,
    server: Server,
    serial: Option<String>,
    allow_device_input: bool,
) {
    let started_at_ms = now_ms();
    let server_label = server.to_string();
    let device_label = serial.clone().unwrap_or_else(|| "自动选择".to_string());
    let mut ctx = BenchCtx::new(
        app.clone(),
        sidecar,
        server,
        serial,
        allow_device_input,
    );
    let mut reports = Vec::with_capacity(specs.len());

    for (index, spec) in specs.iter().enumerate() {
        if state.cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Ok(mut status) = state.status.lock() {
            status.current_case = Some(spec.name.to_string());
            status.completed = index as u32;
        }
        emit(&app, &state, None, None, None);

        let report = cases::run_case(spec, &mut ctx, &state.cancel);

        if let Ok(mut status) = state.status.lock() {
            status.completed = index as u32 + 1;
        }
        emit(&app, &state, Some(report.clone()), None, None);
        reports.push(report);
    }

    let summary = BenchRunSummary {
        run_id: uuid::Uuid::new_v4().to_string(),
        preset_id: Some(preset_id),
        preset_name,
        started_at_ms,
        finished_at_ms: now_ms(),
        device_label,
        server: server_label,
        reports,
    };

    if let Ok(mut runs) = state.runs.lock() {
        runs.insert(0, summary.clone());
        runs.truncate(MAX_RUNS);
    }
    if let Ok(mut status) = state.status.lock() {
        status.running = false;
        status.current_case = None;
    }
    emit(&app, &state, None, Some(summary), None);
}
