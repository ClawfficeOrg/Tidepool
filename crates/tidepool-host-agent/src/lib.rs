//! Reference host agent state for the Tidepool host protocol (v1).
//!
//! The agent deduplicates create intents by `executionId`. A replayed intent
//! returns the stored completion instead of running the prompt again. A
//! completion is final: a later `interrupt_execution` or replay answers with
//! the status that was recorded, never a second, different terminal state.
//! This library is shared by the binary and its tests.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Reference agent execution state.
#[derive(Debug, Default)]
pub struct AgentState {
    running: HashSet<String>,
    completed: HashMap<String, Completion>,
}

/// A recorded terminal state.
#[derive(Debug, Clone)]
struct Completion {
    status: &'static str,
    output: String,
}

impl AgentState {
    /// Handle a `create_agent` frame.
    ///
    /// Returns frames to send: `execution_started`, one `execution_output`
    /// delta and `execution_completed`. A replay returns only the stored
    /// completion.
    pub fn handle_create(&mut self, execution_id: &str, prompt: &str) -> Vec<Value> {
        if let Some(done) = self.completed.get(execution_id) {
            return vec![completion(execution_id, done.status, &done.output)];
        }
        if !self.running.insert(execution_id.to_string()) {
            // Already running; do not start a second execution.
            return Vec::new();
        }

        let output = format!("reference agent received: {prompt}");
        self.running.remove(execution_id);
        self.completed.insert(
            execution_id.to_string(),
            Completion {
                status: "succeeded",
                output: output.clone(),
            },
        );
        vec![
            json!({"v": 1, "type": "execution_started", "executionId": execution_id}),
            json!({"v": 1, "type": "execution_output", "executionId": execution_id, "text": output}),
            completion(execution_id, "succeeded", &output),
        ]
    }

    /// Handle `interrupt_execution`.
    ///
    /// A completed execution answers with its stored completion and real
    /// status. A running one completes as `interrupted`. An unknown id is a
    /// no-op: the pool has no handler for host `error` frames.
    pub fn handle_interrupt(&mut self, execution_id: &str) -> Vec<Value> {
        if let Some(done) = self.completed.get(execution_id) {
            return vec![completion(execution_id, done.status, &done.output)];
        }
        if self.running.remove(execution_id) {
            let output = "interrupted".to_string();
            self.completed.insert(
                execution_id.to_string(),
                Completion {
                    status: "interrupted",
                    output: output.clone(),
                },
            );
            return vec![completion(execution_id, "interrupted", &output)];
        }
        Vec::new()
    }

    /// Recorded terminal status, when the execution completed.
    pub fn status(&self, execution_id: &str) -> Option<&'static str> {
        self.completed.get(execution_id).map(|done| done.status)
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
    fn interrupt_after_completion_keeps_the_recorded_status() {
        let mut state = AgentState::default();
        let first = state.handle_create("exec-1", "one");
        let output = first[2]["output"].clone();

        let frames = state.handle_interrupt("exec-1");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["type"], "execution_completed");
        assert_eq!(frames[0]["status"], "succeeded", "status must not regress");
        assert_eq!(frames[0]["output"], output);
        assert_eq!(state.status("exec-1"), Some("succeeded"));

        // A replay after the interrupt still reports the one terminal state.
        let replay = state.handle_create("exec-1", "one");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0]["status"], "succeeded");
    }

    #[test]
    fn interrupt_of_a_running_execution_is_final() {
        let mut state = AgentState::default();
        state.running.insert("exec-1".to_string());

        let frames = state.handle_interrupt("exec-1");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["status"], "interrupted");
        assert_eq!(state.status("exec-1"), Some("interrupted"));

        // Neither a second interrupt nor a replayed create changes it.
        assert_eq!(state.handle_interrupt("exec-1")[0]["status"], "interrupted");
        let replay = state.handle_create("exec-1", "one");
        assert_eq!(replay.len(), 1, "an interrupted execution must not run");
        assert_eq!(replay[0]["status"], "interrupted");
    }

    #[test]
    fn interrupt_of_an_unknown_execution_sends_nothing() {
        let mut state = AgentState::default();
        assert!(state.handle_interrupt("exec-missing").is_empty());
    }
}
