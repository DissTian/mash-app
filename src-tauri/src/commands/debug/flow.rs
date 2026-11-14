//! CV debug "流程测试" trace commands.
//!
//! The tracing implementation behind them is only compiled with the
//! `flow-trace` Cargo feature (enabled by the local test-build script). Normal
//! releases keep these thin stubs so the command names stay registered.
#![cfg_attr(not(any(debug_assertions, feature = "flow-trace")), allow(unused_variables))]

#[cfg(any(debug_assertions, feature = "flow-trace"))]
use std::sync::Mutex;

#[cfg(any(debug_assertions, feature = "flow-trace"))]
use tauri::State;

#[cfg(any(debug_assertions, feature = "flow-trace"))]
use crate::flow_trace::{FlowTrace, FlowTraceSnapshot};

/// Enable flow tracing for the debug "流程测试" tab and reset old steps.
#[cfg(any(debug_assertions, feature = "flow-trace"))]
#[tauri::command]
pub fn flow_trace_start(trace: State<'_, Mutex<FlowTrace>>) -> Result<(), String> {
    trace
        .lock()
        .map_err(|_| "flow trace lock poisoned".to_string())?
        .start();
    Ok(())
}

#[cfg(not(any(debug_assertions, feature = "flow-trace")))]
#[tauri::command]
pub fn flow_trace_start() -> Result<(), String> {
    Err("当前构建未启用流程追踪".into())
}

#[cfg(any(debug_assertions, feature = "flow-trace"))]
#[tauri::command]
pub fn flow_trace_stop(trace: State<'_, Mutex<FlowTrace>>) -> Result<(), String> {
    trace
        .lock()
        .map_err(|_| "flow trace lock poisoned".to_string())?
        .stop();
    Ok(())
}

#[cfg(not(any(debug_assertions, feature = "flow-trace")))]
#[tauri::command]
pub fn flow_trace_stop() -> Result<(), String> {
    Ok(())
}

#[cfg(any(debug_assertions, feature = "flow-trace"))]
#[tauri::command]
pub fn flow_trace_clear(trace: State<'_, Mutex<FlowTrace>>) -> Result<(), String> {
    trace
        .lock()
        .map_err(|_| "flow trace lock poisoned".to_string())?
        .clear();
    Ok(())
}

#[cfg(not(any(debug_assertions, feature = "flow-trace")))]
#[tauri::command]
pub fn flow_trace_clear() -> Result<(), String> {
    Ok(())
}

#[cfg(any(debug_assertions, feature = "flow-trace"))]
#[tauri::command]
pub fn flow_trace_snapshot(
    trace: State<'_, Mutex<FlowTrace>>,
) -> Result<FlowTraceSnapshot, String> {
    Ok(trace
        .lock()
        .map_err(|_| "flow trace lock poisoned".to_string())?
        .snapshot())
}

#[cfg(not(any(debug_assertions, feature = "flow-trace")))]
#[tauri::command]
pub fn flow_trace_snapshot() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "enabled": false,
        "totalMs": 0.0,
        "steps": [],
        "currentStep": null,
        "unsupported": true,
    }))
}
