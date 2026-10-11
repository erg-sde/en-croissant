//! Cancellation tokens for long-running jobs, one per job id.
//!
//! Each job owns a `watch` channel. The handle keeps a receiver of its own so the channel never
//! closes, and cancellation uses `send_replace`, which stores the value whether or not a stage is
//! currently waiting on it. Stages check the current value before they start waiting, so a cancel
//! that arrives between two stages is never lost.

use dashmap::DashMap;
use tokio::sync::watch;

pub struct JobHandle {
    pub cancel: watch::Sender<bool>,
    keep: watch::Receiver<bool>,
}

#[derive(Default)]
pub struct JobRegistry(DashMap<String, JobHandle>);

impl JobRegistry {
    /// Registers a job under `id`, cancelling and replacing any job already registered there.
    /// Returns a receiver for the new job; pass it to [`JobRegistry::remove`] when the job ends.
    pub fn insert(&self, id: &str) -> watch::Receiver<bool> {
        let (cancel, keep) = watch::channel(false);
        let rx = keep.clone();
        if let Some(previous) = self.0.insert(id.to_string(), JobHandle { cancel, keep }) {
            previous.cancel.send_replace(true);
        }
        rx
    }

    /// Requests cancellation of the job under `id`. Returns whether a job was registered.
    pub fn cancel(&self, id: &str) -> bool {
        match self.0.get(id) {
            Some(job) => {
                job.cancel.send_replace(true);
                true
            }
            None => false,
        }
    }

    /// Removes the job under `id` only if it is still the job `rx` belongs to, so a finishing run
    /// never unregisters the run that replaced it.
    pub fn remove(&self, id: &str, rx: &watch::Receiver<bool>) {
        self.0.remove_if(id, |_, job| job.keep.same_channel(rx));
    }

    #[cfg(test)]
    fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }
}

/// Whether cancellation was requested on `rx`. A closed channel counts as cancelled: it means the
/// job was unregistered while this stage was still running.
pub fn is_cancelled(rx: &watch::Receiver<bool>) -> bool {
    *rx.borrow() || rx.has_changed().is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_with_no_stage_waiting_is_stored() {
        let jobs = JobRegistry::default();
        let rx = jobs.insert("report_a");
        assert!(jobs.cancel("report_a"));
        assert!(is_cancelled(&rx));
    }

    #[test]
    fn cancel_unknown_id_is_a_no_op() {
        let jobs = JobRegistry::default();
        assert!(!jobs.cancel("report_a"));
    }

    #[test]
    fn insert_cancels_and_replaces_previous_run() {
        let jobs = JobRegistry::default();
        let first = jobs.insert("report_a");
        let second = jobs.insert("report_a");
        assert!(is_cancelled(&first));
        assert!(!is_cancelled(&second));
    }

    #[test]
    fn remove_keeps_a_newer_run() {
        let jobs = JobRegistry::default();
        let first = jobs.insert("report_a");
        let second = jobs.insert("report_a");
        jobs.remove("report_a", &first);
        assert!(jobs.contains("report_a"));
        jobs.remove("report_a", &second);
        assert!(!jobs.contains("report_a"));
    }

    #[test]
    fn removed_job_counts_as_cancelled() {
        let jobs = JobRegistry::default();
        let rx = jobs.insert("report_a");
        jobs.remove("report_a", &rx);
        assert!(is_cancelled(&rx));
    }

    #[tokio::test]
    async fn waiting_stage_wakes_on_cancel() {
        let jobs = JobRegistry::default();
        let mut rx = jobs.insert("report_a");
        let waiter = tokio::spawn(async move { rx.changed().await.is_ok() });
        tokio::task::yield_now().await;
        jobs.cancel("report_a");
        assert!(waiter.await.unwrap());
    }
}
