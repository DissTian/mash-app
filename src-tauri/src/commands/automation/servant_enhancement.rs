//! CN multi-servant enhancement queue commands.

use super::*;
use crate::commands::catalog::load_servant_enhancement_target;
use crate::servant_enhancement::{
    natural_max_for_rarity, normalize_level, servant_enhancement_lifecycle_transition, EntryStatus,
    ServantEnhancementAutomationEvent, ServantEnhancementEntry, ServantEnhancementEntryInput,
    ServantEnhancementEntryState, ServantEnhancementLifecycleEvent,
    ServantEnhancementQueueStore, ServantEnhancementRunner, ServantEnhancementRunnerHandle,
    ServantEnhancementRunnerState, ServantEnhancementStatePayload, EVENT_NAME,
};
use std::collections::HashSet;

fn apply_lifecycle_event(
    state: &Arc<Mutex<ServantEnhancementRunnerState>>,
    event: ServantEnhancementLifecycleEvent,
) {
    let mut guard = state.lock().unwrap();
    let transition = servant_enhancement_lifecycle_transition(guard.clone(), event);
    if transition.accepted {
        *guard = transition.next;
    }
}

fn emit_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<ServantEnhancementRunnerState>>,
    entries: &Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
    current_entry_id: &Arc<Mutex<Option<String>>>,
    screen: &str,
    message: &str,
    level: LogLevel,
) {
    let (state_str, status) = {
        let state = state.lock().unwrap();
        (format!("{:?}", *state), state.status())
    };
    crate::recognition_diagnostics::logs::record(app, screen, message);
    let _ = app.emit(
        EVENT_NAME,
        ServantEnhancementAutomationEvent {
            state: state_str,
            status,
            current_screen: screen.into(),
            message: message.into(),
            level,
            entries: entries.lock().unwrap().clone(),
            current_entry_id: current_entry_id.lock().unwrap().clone(),
        },
    );
}

fn fail_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<ServantEnhancementRunnerState>>,
    entries: &Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
    current_entry_id: &Arc<Mutex<Option<String>>>,
    message: String,
) {
    apply_lifecycle_event(
        state,
        ServantEnhancementLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_status(
        app,
        state,
        entries,
        current_entry_id,
        "",
        &format!("启动失败: {message}"),
        LogLevel::Info,
    );
}

fn stop_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<ServantEnhancementRunnerState>>,
    entries: &Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
    current_entry_id: &Arc<Mutex<Option<String>>>,
) {
    apply_lifecycle_event(state, ServantEnhancementLifecycleEvent::StopRequested);
    emit_status(
        app,
        state,
        entries,
        current_entry_id,
        "",
        "从者强化队列已停止",
        LogLevel::Info,
    );
}

#[tauri::command]
pub(crate) fn get_servant_enhancement_queue(
    store: tauri::State<'_, Mutex<ServantEnhancementQueueStore>>,
) -> Vec<ServantEnhancementEntry> {
    store.lock().unwrap().entries.clone()
}

#[tauri::command]
pub(crate) fn save_servant_enhancement_queue(
    app: tauri::AppHandle,
    entries: Vec<ServantEnhancementEntryInput>,
    store: tauri::State<'_, Mutex<ServantEnhancementQueueStore>>,
    handle_state: tauri::State<'_, Mutex<ServantEnhancementRunnerHandle>>,
) -> Result<Vec<ServantEnhancementEntry>, String> {
    let servants = crate::commands::catalog::servants_data()?;
    let mut seen = HashSet::new();
    let mut next_entries = Vec::with_capacity(entries.len());

    for input in entries {
        if !seen.insert(input.variant_key.clone()) {
            continue;
        }
        let servant = servants
            .iter()
            .find(|servant| servant.variant_key == input.variant_key)
            .ok_or_else(|| format!("未找到目标从者 variantKey: {}", input.variant_key))?;
        let target_level = input
            .target_level
            .map(normalize_level)
            .unwrap_or_else(|| natural_max_for_rarity(servant.rarity));
        next_entries.push(ServantEnhancementEntry {
            id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            servant_id: servant.id,
            variant_key: servant.variant_key.clone(),
            name_cn: servant.name_cn.clone(),
            class_name: servant.class.clone(),
            rarity: servant.rarity,
            target_level,
            two_exp: input.two_exp,
            face_key: input.face_key.filter(|key| !key.is_empty()),
            enabled: input.enabled,
        });
    }

    {
        let mut guard = store.lock().unwrap();
        guard.entries = next_entries.clone();
        guard.save(&app)?;
    }

    {
        let handle = handle_state.lock().unwrap();
        let mut entries = handle.entries.lock().unwrap();
        *entries = next_entries
            .iter()
            .map(ServantEnhancementEntryState::from_entry)
            .collect();
        *handle.current_entry_id.lock().unwrap() = None;
    }

    Ok(next_entries)
}

#[tauri::command]
pub(crate) fn start_servant_enhancement_automation(
    app: tauri::AppHandle,
    handle_state: tauri::State<'_, Mutex<ServantEnhancementRunnerHandle>>,
    store: tauri::State<'_, Mutex<ServantEnhancementQueueStore>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    if server != Server::Cn {
        return Err("当前仅支持国服从者强化自动化".into());
    }

    let entries = store.lock().unwrap().entries.clone();
    if entries.is_empty() {
        return Err("从者强化队列为空".into());
    }

    let mut targets = Vec::with_capacity(entries.len());
    for entry in &entries {
        if !entry.enabled {
            continue;
        }
        targets.push(load_servant_enhancement_target(
            &app,
            &entry.id,
            &entry.variant_key,
            entry.target_level,
            entry.two_exp,
            entry.face_key.as_deref(),
        )?);
    }
    if targets.is_empty() {
        return Err("从者强化队列中没有启用的从者".into());
    }

    let automation_lease = coordinator.reserve(DeviceOperation::Automation(
        AutomationKind::ServantEnhancement,
    ))?;
    let selected_adb_serial = app
        .state::<Mutex<AdbDeviceSettings>>()
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();

    let state = Arc::new(Mutex::new(ServantEnhancementRunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let entry_states = Arc::new(Mutex::new(
        entries
            .iter()
            .map(|entry| {
                let mut state = ServantEnhancementEntryState::from_entry(entry);
                if !entry.enabled {
                    state.status = EntryStatus::Skipped;
                    state.message = "已禁用（本次跳过）".into();
                }
                state
            })
            .collect::<Vec<_>>(),
    ));
    let current_entry_id = Arc::new(Mutex::new(None));
    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
        handle.entries = entry_states.clone();
        handle.current_entry_id = current_entry_id.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        let _automation_lease = automation_lease;
        let Some(AutomationSession {
            adb: adb_dev,
            sidecar,
            input_size,
            ..
        }) = prepare_automation_session(
            &app,
            server,
            selected_adb_serial,
            &debug::DebugSidecar(debug_sidecar.clone()),
            &cancel,
            StartupFeedback {
                scope: "servant-enhancement",
                progress: &|message| {
                    emit_status(
                        &app,
                        &state,
                        &entry_states,
                        &current_entry_id,
                        "",
                        message,
                        LogLevel::Info,
                    )
                },
                failed: &|message| {
                    fail_start(
                        &app,
                        &state,
                        &entry_states,
                        &current_entry_id,
                        message,
                    )
                },
                cancelled: &|| {
                    stop_start(
                        &app,
                        &state,
                        &entry_states,
                        &current_entry_id,
                    )
                },
            },
            |_| Ok(()),
        )
        else {
            return;
        };

        let runner = ServantEnhancementRunner::new(
            adb_dev,
            sidecar,
            app,
            state,
            entry_states,
            current_entry_id,
            cancel,
            input_size,
            targets,
            Some(debug_sidecar),
        );
        runner.run();
    });

    Ok(())
}

#[tauri::command]
pub(crate) fn stop_servant_enhancement_automation(
    handle_state: tauri::State<'_, Mutex<ServantEnhancementRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_servant_enhancement_automation_status(
    handle_state: tauri::State<'_, Mutex<ServantEnhancementRunnerHandle>>,
) -> ServantEnhancementStatePayload {
    let handle = handle_state.lock().unwrap();
    let status = handle.state.lock().unwrap().status();
    let entries = handle.entries.lock().unwrap().clone();
    let current_entry_id = handle.current_entry_id.lock().unwrap().clone();
    ServantEnhancementStatePayload {
        status,
        entries,
        current_entry_id,
    }
}
