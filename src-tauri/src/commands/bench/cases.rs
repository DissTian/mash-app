//! Benchmark case table and the measurements behind each case.
//!
//! Every case is a "primitive" that automations keep paying for: ADB latency,
//! frame capture, OCR, template matching. Cases never mutate game flow unless
//! `device_input` is set (those are only run when the caller opted in), so the
//! base preset can be executed at any time.

use super::{BenchCaseReport, BenchCategory, BenchSource};
use crate::adb;
use crate::commands::debug::{self, DebugSidecar};
use crate::screen::{CvError, NormRect, SidecarClient};
use crate::server::Server;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Same crop the CN enhancement runner OCRs for screen classification.
const FULL_SCREEN: NormRect = NormRect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};
/// Bottom-right 強化/强化 button luma region, identical to the enhancement
/// runner's `BUTTON_LUMA_REGION`.
const ENHANCE_BUTTON_LUMA: NormRect = NormRect {
    x: 0.833,
    y: 0.895,
    w: 0.035,
    h: 0.06,
};
/// Template shipped for both servers; used as the generic matching cost probe.
const ANCHOR_TEMPLATE: &str = "text_servant_avatar_bottom_line";
const ANCHOR_THRESHOLD: f64 = 0.6;

pub(super) struct CaseSpec {
    pub(super) id: &'static str,
    pub(super) category: BenchCategory,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) source: BenchSource,
    /// Runs real input on the device (never part of the default preset).
    pub(super) device_input: bool,
    pub(super) warmup: u32,
    pub(super) iterations: u32,
    pub(super) budget_ms: Option<f64>,
}

pub(super) struct PresetSpec {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) case_ids: &'static [&'static str],
    pub(super) estimated_seconds: u32,
}

pub(super) const CASES: &[CaseSpec] = &[
    CaseSpec {
        id: "adb.connect",
        category: BenchCategory::Device,
        name: "ADB 连接",
        description: "新建 ADB 连接并绑定设备，不含视频流",
        source: BenchSource::Device,
        device_input: false,
        warmup: 0,
        iterations: 3,
        budget_ms: Some(500.0),
    },
    CaseSpec {
        id: "adb.command",
        category: BenchCategory::Device,
        name: "ADB 命令往返",
        description: "读取设备分辨率，衡量一次 adb shell 往返",
        source: BenchSource::Device,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(200.0),
    },
    CaseSpec {
        id: "adb.screencap",
        category: BenchCategory::Device,
        name: "ADB 截图",
        description: "exec-out screencap 截图并编码为 PNG",
        source: BenchSource::Device,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(800.0),
    },
    CaseSpec {
        id: "adb.tap",
        category: BenchCategory::Device,
        name: "ADB 点击注入",
        description: "spawn 一次 adb input tap（会真实点击设备）",
        source: BenchSource::Device,
        device_input: true,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(400.0),
    },
    CaseSpec {
        id: "capture.frame",
        category: BenchCategory::Capture,
        name: "视频流取帧",
        description: "从 scrcpy 视频流取一帧 JPEG",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 2,
        iterations: 10,
        budget_ms: Some(150.0),
    },
    CaseSpec {
        id: "ocr.full_frame",
        category: BenchCategory::Ocr,
        name: "整屏 OCR",
        description: "对实时帧做整屏 det+rec，这是强化链路每一步的现状",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 1,
        iterations: 3,
        budget_ms: Some(4000.0),
    },
    CaseSpec {
        id: "ocr.full_frame_file",
        category: BenchCategory::Ocr,
        name: "整屏 OCR（上一张截图）",
        description: "对调试页的最后一张截图做整屏 OCR，无设备也能跑",
        source: BenchSource::LastCapture,
        device_input: false,
        warmup: 1,
        iterations: 3,
        budget_ms: Some(4000.0),
    },
    CaseSpec {
        id: "ocr.level_read",
        category: BenchCategory::Ocr,
        name: "当前等级读取",
        description: "专用等级 ROI 的几何切分 + 纯识别",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(300.0),
    },
    CaseSpec {
        id: "ocr.worker_cold",
        category: BenchCategory::Ocr,
        name: "OCR 冷启动",
        description: "释放 OCR worker 后第一次识别的耗时（模型重新加载）",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 0,
        iterations: 2,
        budget_ms: Some(8000.0),
    },
    CaseSpec {
        id: "luma.enhance_button",
        category: BenchCategory::Ocr,
        name: "强化按钮亮度",
        description: "读取右下角按钮区域的平均流明",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(300.0),
    },
    CaseSpec {
        id: "template.anchor",
        category: BenchCategory::Template,
        name: "模板锚点匹配",
        description: "在实时帧上匹配一条模板锚点",
        source: BenchSource::LiveFrame,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(300.0),
    },
    CaseSpec {
        id: "template.anchor_file",
        category: BenchCategory::Template,
        name: "模板锚点匹配（上一张截图）",
        description: "在最后一张截图上匹配模板锚点，无设备也能跑",
        source: BenchSource::LastCapture,
        device_input: false,
        warmup: 1,
        iterations: 5,
        budget_ms: Some(300.0),
    },
];

pub(super) const PRESETS: &[PresetSpec] = &[
    PresetSpec {
        id: "basic.performance",
        name: "基础性能",
        description: "ADB 响应、截图、取帧、OCR、模板匹配的基础耗时；不依赖游戏当前画面",
        case_ids: &[
            "adb.connect",
            "adb.command",
            "adb.screencap",
            "capture.frame",
            "ocr.full_frame",
            "ocr.full_frame_file",
            "ocr.level_read",
            "ocr.worker_cold",
            "luma.enhance_button",
            "template.anchor",
            "template.anchor_file",
        ],
        estimated_seconds: 45,
    },
    PresetSpec {
        id: "ocr.baseline",
        name: "OCR 基线",
        description: "整屏 OCR、截图 OCR、等级读取与冷启动，用来对照识别改造前后",
        case_ids: &[
            "ocr.full_frame",
            "ocr.full_frame_file",
            "ocr.level_read",
            "ocr.worker_cold",
            "luma.enhance_button",
        ],
        estimated_seconds: 30,
    },
    PresetSpec {
        id: "device.input",
        name: "设备输入",
        description: "ADB 点击注入耗时（会真实点击设备，需要勾选允许）",
        case_ids: &["adb.tap"],
        estimated_seconds: 5,
    },
];

pub(super) fn case_by_id(id: &str) -> Option<&'static CaseSpec> {
    CASES.iter().find(|case| case.id == id)
}

pub(super) fn preset_by_id(id: &str) -> Option<&'static PresetSpec> {
    PRESETS.iter().find(|preset| preset.id == id)
}

pub(super) fn preset_contains_device_input(preset: &PresetSpec) -> bool {
    preset
        .case_ids
        .iter()
        .filter_map(|id| case_by_id(id))
        .any(|case| case.device_input)
}

/// Owns every device/sidecar handle a case may need, connected lazily so a
/// device-less run only pays for what it actually measures.
pub(super) struct BenchCtx {
    app: tauri::AppHandle,
    sidecar: Arc<Mutex<Option<SidecarClient>>>,
    server: Server,
    serial: Option<String>,
    allow_device_input: bool,
    adb: Option<adb::Adb>,
    adb_error: Option<String>,
    image_path: PathBuf,
}

impl BenchCtx {
    pub(super) fn new(
        app: tauri::AppHandle,
        sidecar: Arc<Mutex<Option<SidecarClient>>>,
        server: Server,
        serial: Option<String>,
        allow_device_input: bool,
    ) -> Self {
        let image_path = debug::debug_image_path(&app);
        Self {
            app,
            sidecar,
            server,
            serial,
            allow_device_input,
            adb: None,
            adb_error: None,
            image_path,
        }
    }

    fn connect(&mut self) -> Result<(), String> {
        if self.adb.is_some() {
            return Ok(());
        }
        if let Some(error) = &self.adb_error {
            return Err(error.clone());
        }
        let mut device = adb::Adb::new(&self.app, self.serial.clone());
        match device.connect() {
            Ok(()) => {
                self.adb = Some(device);
                Ok(())
            }
            Err(error) => {
                let message = format!("ADB 连接失败：{error}");
                self.adb_error = Some(message.clone());
                Err(message)
            }
        }
    }

    fn device(&mut self) -> Result<&adb::Adb, String> {
        self.connect()?;
        self.adb
            .as_ref()
            .ok_or_else(|| "ADB 未连接".to_string())
    }

    fn ensure_sidecar(&mut self) -> Result<(), String> {
        let state = DebugSidecar(self.sidecar.clone());
        debug::ensure_debug_sidecar(&self.app, &state, self.server)
    }

    fn ensure_stream(&mut self) -> Result<(), String> {
        self.connect()?;
        let state = DebugSidecar(self.sidecar.clone());
        let device = self.adb.as_ref().ok_or_else(|| "ADB 未连接".to_string())?;
        debug::ensure_debug_stream(&self.app, &state, self.server, device)
            .map_err(|error| format!("视频流不可用：{error}"))
    }

    fn with_client<R>(
        &mut self,
        action: impl FnOnce(&mut SidecarClient) -> Result<R, CvError>,
    ) -> Result<R, String> {
        let mut guard = self
            .sidecar
            .lock()
            .map_err(|_| "CV sidecar 锁异常".to_string())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "CV sidecar 未就绪".to_string())?;
        action(client).map_err(|error| error.to_string())
    }

    fn capture_image(&self) -> Result<PathBuf, String> {
        if !self.image_path.is_file() {
            return Err("尚未截取画面，请先在调试页点击「截取画面」".to_string());
        }
        Ok(self.image_path.clone())
    }
}

/// Untimed setup that must happen before a timed call (e.g. dropping the OCR
/// worker so the next recognition pays the cold-start cost).
fn prepare_case(id: &str, ctx: &mut BenchCtx) -> Result<(), String> {
    if id == "ocr.worker_cold" {
        ctx.ensure_stream()?;
        ctx.with_client(|client| client.release_ocr())?;
    }
    Ok(())
}

fn measure_once(id: &str, ctx: &mut BenchCtx) -> Result<String, String> {
    match id {
        "adb.connect" => {
            let mut device = adb::Adb::new(&ctx.app, ctx.serial.clone());
            device.connect()?;
            Ok(device.serial().unwrap_or("已连接").to_string())
        }
        "adb.command" => match ctx.device()?.screen_size() {
            Some((width, height)) => Ok(format!("{width}x{height}")),
            None => Err("未读取到设备分辨率".to_string()),
        },
        "adb.screencap" => {
            let path = ctx.device()?.screenshot_to_file()?;
            let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let _ = std::fs::remove_file(&path);
            Ok(format!("{} KB", bytes / 1024))
        }
        "adb.tap" => {
            if !ctx.allow_device_input {
                return Err("该用例会真实点击设备，请在页面上勾选允许".to_string());
            }
            let (width, height) = ctx
                .device()?
                .screen_size()
                .ok_or_else(|| "未读取到设备分辨率".to_string())?;
            ctx.device()?.tap(width / 2, (f64::from(height) * 0.95) as u32)?;
            Ok("底部点击".to_string())
        }
        "capture.frame" => {
            ctx.ensure_stream()?;
            let bytes = ctx.with_client(|client| client.get_frame_jpeg(2.0))?;
            Ok(format!("{} KB", bytes.len() / 1024))
        }
        "ocr.full_frame" => {
            ctx.ensure_stream()?;
            let result = ctx.with_client(|client| client.ocr_region(None, FULL_SCREEN))?;
            Ok(format!("{} 行文字", result.fragments.len()))
        }
        "ocr.full_frame_file" => {
            ctx.ensure_sidecar()?;
            let path = ctx.capture_image()?;
            let result = ctx.with_client(|client| client.ocr_region(Some(&path), FULL_SCREEN))?;
            Ok(format!("{} 行文字（截图）", result.fragments.len()))
        }
        "ocr.level_read" => {
            ctx.ensure_stream()?;
            let result = ctx.with_client(|client| client.read_enhancement_current_level(None))?;
            Ok(format!("found={} text={:?}", result.found, result.text))
        }
        "ocr.worker_cold" => {
            ctx.ensure_stream()?;
            let result = ctx.with_client(|client| client.ocr_region(None, FULL_SCREEN))?;
            Ok(format!("冷启动后 {} 行文字", result.fragments.len()))
        }
        "luma.enhance_button" => {
            ctx.ensure_stream()?;
            let stats = ctx.with_client(|client| client.read_region_color(None, ENHANCE_BUTTON_LUMA))?;
            Ok(format!("luma {:.1}", stats.mean_luma))
        }
        "template.anchor" => {
            ctx.ensure_stream()?;
            let hit = ctx.with_client(|client| {
                client.find_element(None, ANCHOR_TEMPLATE, FULL_SCREEN, ANCHOR_THRESHOLD)
            })?;
            Ok(hit_label(hit))
        }
        "template.anchor_file" => {
            ctx.ensure_sidecar()?;
            let path = ctx.capture_image()?;
            let hit = ctx.with_client(|client| {
                client.find_element(Some(&path), ANCHOR_TEMPLATE, FULL_SCREEN, ANCHOR_THRESHOLD)
            })?;
            Ok(hit_label(hit))
        }
        other => Err(format!("未实现的测试用例：{other}")),
    }
}

fn hit_label(hit: Option<crate::screen::Point>) -> String {
    match hit {
        Some(point) => format!("命中 ({:.3}, {:.3})", point.x, point.y),
        None => "未命中".to_string(),
    }
}

pub(super) fn run_case(
    spec: &CaseSpec,
    ctx: &mut BenchCtx,
    cancel: &AtomicBool,
) -> BenchCaseReport {
    let mut samples = Vec::new();
    let mut note = String::new();
    let mut skipped = None;

    for index in 0..spec.warmup + spec.iterations {
        if cancel.load(Ordering::Relaxed) {
            skipped = Some("已取消".to_string());
            break;
        }
        if let Err(error) = prepare_case(spec.id, ctx) {
            skipped = Some(error);
            break;
        }
        let started = Instant::now();
        match measure_once(spec.id, ctx) {
            Ok(value) => {
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                note = value;
                if index >= spec.warmup {
                    samples.push(elapsed);
                }
            }
            Err(error) => {
                skipped = Some(error);
                break;
            }
        }
    }

    let (min_ms, median_ms, p95_ms) = summary(&samples);
    let passed = spec
        .budget_ms
        .filter(|_| skipped.is_none() && !samples.is_empty())
        .map(|budget| median_ms <= budget);

    BenchCaseReport {
        case_id: spec.id.to_string(),
        name: spec.name.to_string(),
        source: spec.source,
        samples_ms: samples,
        min_ms,
        median_ms,
        p95_ms,
        budget_ms: spec.budget_ms,
        passed,
        skipped,
        note,
    }
}

/// `(min, median, p95)` over the warm samples; nearest-rank percentile.
pub(super) fn summary(samples: &[f64]) -> (f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let len = sorted.len();
    let min = sorted[0];
    let median = if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    };
    let rank = ((0.95 * len as f64).ceil() as usize).clamp(1, len);
    (min, median, sorted[rank - 1])
}
