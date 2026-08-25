use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

#[derive(Clone, Default)]
pub(crate) struct JobRegistry {
    next_id: Arc<AtomicU64>,
    jobs: Arc<Mutex<HashMap<String, JobSnapshot>>>,
}

#[derive(Clone, Debug)]
pub(crate) struct JobSnapshot {
    pub(crate) id: String,
    pub(crate) state: &'static str,
    pub(crate) progress: u8,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

impl JobSnapshot {
    fn json(&self) -> Value {
        let mut value = json!({
            "job_id": self.id,
            "state": self.state,
            "progress": self.progress,
        });
        if let Some(result) = &self.result {
            value["result"] = result.clone();
        }
        if let Some(error) = &self.error {
            value["error"] = json!({
                "message": error,
                "error_code": "job_failed",
                "retryable": true,
            });
        }
        value
    }
}

impl JobRegistry {
    pub(crate) fn start<F>(&self, operation: F) -> String
    where
        F: FnOnce() -> Result<Value, String> + Send + 'static,
    {
        let id = format!("job-{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let snapshot = JobSnapshot {
            id: id.clone(),
            state: "queued",
            progress: 0,
            result: None,
            error: None,
        };
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(id.clone(), snapshot);
        }
        let jobs = Arc::clone(&self.jobs);
        let thread_id = id.clone();
        thread::spawn(move || {
            update(&jobs, &thread_id, "running", 10, None, None);
            match operation() {
                Ok(result) => update(&jobs, &thread_id, "completed", 100, Some(result), None),
                Err(error) => update(&jobs, &thread_id, "failed", 100, None, Some(error)),
            }
        });
        id
    }

    pub(crate) fn get(&self, id: &str) -> Option<JobSnapshot> {
        self.jobs.lock().ok()?.get(id).cloned()
    }
}

fn update(
    jobs: &Mutex<HashMap<String, JobSnapshot>>,
    id: &str,
    state: &'static str,
    progress: u8,
    result: Option<Value>,
    error: Option<String>,
) {
    if let Ok(mut guard) = jobs.lock() {
        if let Some(snapshot) = guard.get_mut(id) {
            snapshot.state = state;
            snapshot.progress = progress;
            snapshot.result = result;
            snapshot.error = error;
        }
    }
}

pub(crate) fn snapshot_response(snapshot: JobSnapshot) -> Value {
    snapshot.json()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::JobRegistry;

    #[test]
    fn registry_polls_to_completion() {
        let registry = JobRegistry::default();
        let id = registry.start(|| Ok(serde_json::json!({"ok": true})));
        for _ in 0..20 {
            if registry
                .get(&id)
                .is_some_and(|job| job.state == "completed")
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("job did not complete");
    }
}
