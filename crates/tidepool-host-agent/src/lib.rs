//! Reference host agent state for the Tidepool host protocol (v1).
//!
//! The agent deduplicates create intents by `executionId`. A replayed intent
//! returns the stored completion instead of running the prompt again. This
//! library is shared by the binary and its tests.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Reference agent execution state.
#[derive(Debug, Default)]
pub struct AgentState {
    running: HashSet<String>,
    completed: HashMap<String, String>,
}

impl AgentState {
    /// Handle a `create_agent` frame.
    ///
    /// Returns frames to send: `execution_started`, one `execution_output`
    /// delta and `execution_completed`. A replay returns only the stored
    /// completion.
    pub fn handle_create(&mut self, execution_id: &str, prompt: &str) -> Vec<Value> {
        if let Some(output) = self.completed.get(execution_id) {
            return vec![completion(execution_id, "succeeded", output)];
        }
        if !self.running.insert(execution_id.to_string()) {
            // Already running; do not start a second execution.
            return Vec::new();
        }

        let output = format!("reference agent received: {prompt}");
        self.running.remove(execution_id);
        self.completed
            .insert(execution_id.to_string(), output.clone());
        vec![
            json!({"v": 1, "type": "execution_started", "executionId": execution_id}),
            json!({"v": 1, "type": "execution_output", "executionId": execution_id, "text": output}),
            completion(execution_id, "succeeded", &output),
        ]
    }

    /// Handle `interrupt_execution`.
    pub fn handle_interrupt(&mut self, execution_id: &str) -> Vec<Value> {
        if let Some(output) = self.completed.get(execution_id) {
            return vec![completion(execution_id, "interrupted", output)];
        }
        if self.running.remove(execution_id) {
            self.completed
                .insert(execution_id.to_string(), "interrupted".to_string());
            return vec![completion(execution_id, "interrupted", "interrupted")];
        }
        vec![json!({
            "v": 1,
            "type": "error",
            "code": "invalid_request",
            "detail": "unknown execution"
        })]
    }

    /// True when the execution completed.
    pub fn is_complete(&self, execution_id: &str) -> bool {
        self.completed.contains_key(execution_id)
    }
}

fn completion(execution_id: &str, status: &str, output: &str) -> Value {
    json!({
        "v": 1,
        "type": "execution_completed",
        "executionId": execution_id,
        "status": status,
        "output": output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_runs_once_and_replays_the_completion() {
        let mut state = AgentState::default();
        let first = state.handle_create("exec-1", "hello");
        assert_eq!(first.len(), 3);
        assert_eq!(first[0]["type"], "execution_started");
        assert_eq!(first[2]["type"], "execution_completed");
        assert_eq!(first[2]["status"], "succeeded");

        let replay = state.handle_create("exec-1", "hello");
        assert_eq!(replay.len(), 1, "replay must not run again");
        assert_eq!(replay[0]["type"], "execution_completed");
        assert_eq!(replay[0]["executionId"], "exec-1");
    }

    #[test]
    fn different_executions_run_independently() {
        let mut state = AgentState::default();
        state.handle_create("exec-1", "one");
        state.handle_create("exec-2", "two");
        assert!(state.is_complete("exec-1"));
        assert!(state.is_complete("exec-2"));
    }

    #[test]
    fn interrupt_completes_with_interrupted() {
        let mut state = AgentState::default();
        state.handle_create("exec-1", "one");
        let frames = state.handle_interrupt("exec-1");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["status"], "interrupted");
    }
}
