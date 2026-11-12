//! Multi-servant enhancement queue for the CN client.
//!
//! The queue is persisted under `app_data_dir()/servant_enhancement_queue.json`
//! and owns its own runner. The older single-target JP runner in
//! `crate::enhancement_runner` is intentionally left untouched so the two
//! server-specific flows can evolve independently.

pub mod runner;

pub(crate) use runner::ServantEnhancementRunner;

#[cfg(test)]
mod tests;

use crate::runner::LogLevel;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub(crate) const EVENT_NAME: &str = "servant-enhancement-automation-status";

/// One persisted queue entry. Fields are authoritative copies of catalog data
/// so the UI can render a saved queue without re-reading the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantEnhancementEntry {
    pub id: String,
    pub servant_id: u32,
    pub variant_key: String,
    pub name_cn: String,
    pub class_name: String,
    pub rarity: u32,
    pub target_level: u32,
    #[serde(default)]
    pub two_exp: bool,
    #[serde(default)]
    pub face_key: Option<String>,
}

/// Input accepted from the queue editor. The backend resolves servant metadata
/// and ids; the frontend only chooses variant keys and per-entry options.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantEnhancementEntryInput {
    #[serde(default)]
    pub id: Option<String>,
    pub variant_key: String,
    #[serde(default)]
    pub target_level: Option<u32>,
    #[serde(default)]
    pub two_exp: bool,
    #[serde(default)]
    pub face_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryStatus {
    Pending,
    Running,
    Completed,
    Skipped,
    Failed,
}

impl EntryStatus {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Skipped | Self::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantEnhancementEntryState {
    pub id: String,
    pub servant_id: u32,
    pub variant_key: String,
    pub name_cn: String,
    pub class_name: String,
    pub rarity: u32,
    pub target_level: u32,
    pub two_exp: bool,
    pub face_key: Option<String>,
    pub status: EntryStatus,
    pub current_level: Option<u32>,
    pub max_level: Option<u32>,
    pub message: String,
}

impl ServantEnhancementEntryState {
    pub(crate) fn from_entry(entry: &ServantEnhancementEntry) -> Self {
        Self {
            id: entry.id.clone(),
            servant_id: entry.servant_id,
            variant_key: entry.variant_key.clone(),
            name_cn: entry.name_cn.clone(),
            class_name: entry.class_name.clone(),
            rarity: entry.rarity,
            target_level: entry.target_level,
            two_exp: entry.two_exp,
            face_key: entry.face_key.clone(),
            status: EntryStatus::Pending,
            current_level: None,
            max_level: None,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantEnhancementAutomationEvent {
    pub state: String,
    pub status: &'static str,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
    pub entries: Vec<ServantEnhancementEntryState>,
    pub current_entry_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantEnhancementStatePayload {
    pub status: &'static str,
    pub entries: Vec<ServantEnhancementEntryState>,
    pub current_entry_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ServantEnhancementRunnerState {
    Idle,
    Starting,
    Running,
    Finished,
    Error { message: String },
}

impl ServantEnhancementRunnerState {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServantEnhancementLifecycleEvent {
    WorkerStarted,
    StopRequested,
    Finished,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServantEnhancementLifecycleTransition {
    pub(crate) next: ServantEnhancementRunnerState,
    pub(crate) accepted: bool,
}

pub(crate) fn servant_enhancement_lifecycle_transition(
    state: ServantEnhancementRunnerState,
    event: ServantEnhancementLifecycleEvent,
) -> ServantEnhancementLifecycleTransition {
    let next = match (&state, &event) {
        (ServantEnhancementRunnerState::Starting, ServantEnhancementLifecycleEvent::WorkerStarted) => {
            Some(ServantEnhancementRunnerState::Running)
        }
        (
            ServantEnhancementRunnerState::Starting | ServantEnhancementRunnerState::Running,
            ServantEnhancementLifecycleEvent::StopRequested,
        ) => Some(ServantEnhancementRunnerState::Finished),
        (ServantEnhancementRunnerState::Running, ServantEnhancementLifecycleEvent::Finished) => {
            Some(ServantEnhancementRunnerState::Finished)
        }
        (_, ServantEnhancementLifecycleEvent::Failed { message }) => {
            Some(ServantEnhancementRunnerState::Error {
                message: message.clone(),
            })
        }
        _ => None,
    };

    let accepted = next.is_some();
    ServantEnhancementLifecycleTransition {
        next: next.unwrap_or(state),
        accepted,
    }
}

pub struct ServantEnhancementRunnerHandle {
    pub state: Arc<Mutex<ServantEnhancementRunnerState>>,
    pub entries: Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
    pub current_entry_id: Arc<Mutex<Option<String>>>,
    pub cancel: Arc<AtomicBool>,
}

impl ServantEnhancementRunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            state: Arc::new(Mutex::new(ServantEnhancementRunnerState::Idle)),
            entries: Arc::new(Mutex::new(Vec::new())),
            current_entry_id: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Default)]
pub struct ServantEnhancementQueueStore {
    pub entries: Vec<ServantEnhancementEntry>,
}

impl ServantEnhancementQueueStore {
    pub fn load(app: &tauri::AppHandle) -> Self {
        let path = crate::paths::servant_enhancement_queue_path(app);
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Vec<ServantEnhancementEntry>>(&raw) {
            Ok(entries) => Self { entries },
            Err(err) => {
                eprintln!(
                    "[servant-enhancement] failed to parse {}: {err}",
                    path.display()
                );
                Self::default()
            }
        }
    }

    pub fn save(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let path: PathBuf = crate::paths::servant_enhancement_queue_path(app);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建队列目录失败: {e}"))?;
        }
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| format!("序列化强化队列失败: {e}"))?;
        fs::write(&path, json).map_err(|e| format!("写入强化队列失败: {e}"))
    }

    /// Persist the queue through the managed state.
    pub fn save_managed(app: &tauri::AppHandle) -> Result<(), String> {
        let state = app.state::<Mutex<ServantEnhancementQueueStore>>();
        let guard = state.lock().unwrap();
        guard.save(app)
    }
}

pub(crate) fn natural_max_for_rarity(rarity: u32) -> u32 {
    match rarity {
        5 => 90,
        4 => 80,
        3 => 70,
        2 => 65,
        _ => 60,
    }
}

pub(crate) fn normalize_level(value: u32) -> u32 {
    value.clamp(1, 120)
}

/// A target resolved against the servant catalog and the installed face assets.
#[derive(Debug, Clone)]
pub struct ServantEnhancementTarget {
    pub entry_id: String,
    pub servant_id: u32,
    pub variant_key: String,
    pub name_cn: String,
    pub class_name: String,
    pub rarity: u32,
    pub target_level: u32,
    pub two_exp: bool,
    pub face_key: Option<String>,
    pub face_template_paths: Vec<(String, PathBuf)>,
}
