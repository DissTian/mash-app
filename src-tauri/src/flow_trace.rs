//! Lightweight step + tool tracing for the CV debug "流程测试" tab.
//!
//! The enhancement runner calls tools through a handful of choke points
//! (sidecar requests, taps, swipes, screen OCR, sleeps). When tracing is
//! enabled from the debug page, those calls are recorded with their wall-clock
//! duration and grouped into the runner's action steps so a whole flow run can
//! be inspected afterwards ("which step/tool is slow?").
//!
//! Production runs pay one mutex lock + bool check per action only.

use serde::Serialize;
use std::time::Instant;

/// Keep memory bounded for long queues; the debug page only needs the tail.
const MAX_STEPS: usize = 2000;
const MAX_TOOLS_PER_STEP: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowToolCall {
    pub name: String,
    pub label: String,
    pub duration_ms: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    pub index: usize,
    pub name: String,
    pub kind: String,
    pub started_at_ms: f64,
    pub duration_ms: f64,
    pub status: String,
    pub tools: Vec<FlowToolCall>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowTraceSnapshot {
    pub enabled: bool,
    pub total_ms: f64,
    pub steps: Vec<FlowStep>,
    /// Index of the step currently accepting tools, if any.
    pub current_step: Option<usize>,
}

/// Managed Tauri state (`Mutex<FlowTrace>`); the enhancement runner and the
/// debug commands share it through `AppHandle::state`.
pub struct FlowTrace {
    enabled: bool,
    started: Option<Instant>,
    steps: Vec<FlowStep>,
    current: Option<usize>,
}

impl Default for FlowTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowTrace {
    pub fn new() -> Self {
        Self {
            enabled: false,
            started: None,
            steps: Vec::new(),
            current: None,
        }
    }

    fn elapsed_ms(&self) -> f64 {
        self.started
            .map(|start| start.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    fn finish_current(&mut self, status: &str) {
        let now_ms = self.elapsed_ms();
        if let Some(index) = self.current {
            if let Some(step) = self.steps.get_mut(index) {
                if step.status == "running" {
                    step.status = status.to_string();
                    step.duration_ms = now_ms - step.started_at_ms;
                }
            }
        }
        self.current = None;
    }

    /// Enable tracing and reset the collected steps.
    pub fn start(&mut self) {
        self.enabled = true;
        self.started = Some(Instant::now());
        self.steps.clear();
        self.current = None;
    }

    /// Disable tracing. The collected snapshot stays readable afterwards.
    pub fn stop(&mut self) {
        self.finish_current("ok");
        self.enabled = false;
    }

    /// Drop all collected steps (keeps the enabled/disabled state).
    pub fn clear(&mut self) {
        self.steps.clear();
        self.current = None;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Start a new action step; the previous running step is closed as `ok`.
    pub fn begin_step(&mut self, name: impl Into<String>, kind: impl Into<String>) {
        if !self.enabled {
            return;
        }
        self.finish_current("ok");
        let index = self.steps.len();
        let started_at_ms = self.elapsed_ms();
        self.steps.push(FlowStep {
            index,
            name: name.into(),
            kind: kind.into(),
            started_at_ms,
            duration_ms: 0.0,
            status: "running".into(),
            tools: Vec::new(),
        });
        self.current = Some(index);
        if self.steps.len() > MAX_STEPS {
            self.steps.remove(0);
            for (position, step) in self.steps.iter_mut().enumerate() {
                step.index = position;
            }
            self.current = Some(self.steps.len() - 1);
        }
    }

    /// Close the running step (called when the run ends or fails).
    pub fn finish_step(&mut self, status: &str) {
        self.finish_current(status);
    }

    /// Attach a tool call to the current step (or a synthetic "启动" step).
    pub fn record_tool(&mut self, name: impl Into<String>, label: impl Into<String>, duration_ms: f64) {
        self.record_tool_with_status(name, label, duration_ms, "ok");
    }

    pub fn record_tool_with_status(
        &mut self,
        name: impl Into<String>,
        label: impl Into<String>,
        duration_ms: f64,
        status: &str,
    ) {
        if !self.enabled {
            return;
        }
        if self.current.is_none() {
            self.begin_step("启动", "misc");
        }
        let Some(index) = self.current else {
            return;
        };
        let Some(step) = self.steps.get_mut(index) else {
            return;
        };
        if step.tools.len() >= MAX_TOOLS_PER_STEP {
            step.tools.remove(0);
        }
        step.tools.push(FlowToolCall {
            name: name.into(),
            label: label.into(),
            duration_ms,
            status: status.into(),
        });
    }

    pub fn snapshot(&mut self) -> FlowTraceSnapshot {
        let total_ms = self.elapsed_ms();
        let mut steps = self.steps.clone();
        if let Some(index) = self.current {
            if let Some(step) = steps.get_mut(index) {
                step.duration_ms = total_ms - step.started_at_ms;
            }
        }
        FlowTraceSnapshot {
            enabled: self.enabled,
            total_ms,
            steps,
            current_step: self.current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_tool_is_a_noop_until_tracing_starts() {
        let mut trace = FlowTrace::new();
        trace.record_tool("cv", "ping", 1.0);
        assert!(trace.snapshot().steps.is_empty());
    }

    #[test]
    fn steps_collect_tools_and_finish_when_the_next_step_starts() {
        let mut trace = FlowTrace::new();
        trace.start();
        trace.begin_step("打开强化", "tap");
        trace.record_tool("tap", "tap (0.5, 0.5)", 12.0);
        trace.record_tool("sleep", "等待 1000 ms", 1000.0);
        trace.begin_step("识别画面", "detect");
        trace.record_tool("cv", "cv:ocr_region", 250.0);

        let snapshot = trace.snapshot();
        assert_eq!(snapshot.steps.len(), 2);
        let first = &snapshot.steps[0];
        assert_eq!(first.name, "打开强化");
        assert_eq!(first.status, "ok");
        assert_eq!(first.tools.len(), 2);
        assert!(first.duration_ms > 0.0);
        let second = &snapshot.steps[1];
        assert_eq!(second.name, "识别画面");
        assert_eq!(second.status, "running");
        assert_eq!(second.tools[0].name, "cv");
    }

    #[test]
    fn tools_without_a_step_create_a_startup_step() {
        let mut trace = FlowTrace::new();
        trace.start();
        trace.record_tool("cv", "cv:ping", 3.0);
        let snapshot = trace.snapshot();
        assert_eq!(snapshot.steps.len(), 1);
        assert_eq!(snapshot.steps[0].name, "启动");
        assert_eq!(snapshot.steps[0].tools.len(), 1);
    }

    #[test]
    fn stop_marks_the_running_step_as_ok_and_keeps_the_snapshot() {
        let mut trace = FlowTrace::new();
        trace.start();
        trace.begin_step("查找从者", "cv");
        trace.record_tool("cv", "cv:find_enhancement_servant_grid", 60.0);
        trace.stop();
        let snapshot = trace.snapshot();
        assert!(!snapshot.enabled);
        assert_eq!(snapshot.steps.len(), 1);
        assert_eq!(snapshot.steps[0].status, "ok");
        assert!(snapshot.steps[0].duration_ms >= 0.0);
    }

    #[test]
    fn clear_keeps_the_enabled_flag() {
        let mut trace = FlowTrace::new();
        trace.start();
        trace.begin_step("步骤", "misc");
        trace.clear();
        assert!(trace.is_enabled());
        assert!(trace.snapshot().steps.is_empty());
    }
}
