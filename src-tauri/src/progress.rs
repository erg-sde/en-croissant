use dashmap::DashMap;
use log::warn;
use serde::Serialize;
use specta::Type;
use tauri_specta::Event;
use tokio::sync::watch;

use crate::{error::Error, jobs::JobRegistry};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProgressStage {
    Engine,
    Commentary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProgressStatus {
    Running,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct ProgressItem {
    pub id: String,
    pub progress: f32,
    pub finished: bool,
    #[specta(optional)]
    pub stage: Option<ProgressStage>,
    #[specta(optional)]
    pub status: Option<ProgressStatus>,
}

#[derive(Clone, Debug, Serialize, Type, Event)]
pub struct ProgressEvent {
    pub id: String,
    pub progress: f32,
    pub finished: bool,
    #[specta(optional)]
    pub stage: Option<ProgressStage>,
    #[specta(optional)]
    pub status: Option<ProgressStatus>,
}

impl From<ProgressItem> for ProgressEvent {
    fn from(item: ProgressItem) -> Self {
        ProgressEvent {
            id: item.id,
            progress: item.progress,
            finished: item.finished,
            stage: item.stage,
            status: item.status,
        }
    }
}

pub type ProgressStore = DashMap<String, ProgressItem>;

pub fn update_progress(
    store: &ProgressStore,
    app: &tauri::AppHandle,
    id: String,
    progress: f32,
    finished: bool,
) -> Result<(), Error> {
    AppProgress { store, app }.publish(ProgressItem {
        id,
        progress,
        finished,
        stage: None,
        status: None,
    })
}

/// Where a [`ProgressGuard`] records and announces progress.
pub trait ProgressSink {
    fn publish(&self, item: ProgressItem) -> Result<(), Error>;
}

/// The app's progress store plus the `progress-event` emitted to the webview.
pub struct AppProgress<'a> {
    pub store: &'a ProgressStore,
    pub app: &'a tauri::AppHandle,
}

impl ProgressSink for AppProgress<'_> {
    fn publish(&self, item: ProgressItem) -> Result<(), Error> {
        self.store.insert(item.id.clone(), item.clone());
        ProgressEvent::from(item).emit(self.app)?;
        Ok(())
    }
}

/// Owns a job's registration and its progress id for the duration of one command.
///
/// Every exit path ends with a terminal progress event: [`ProgressGuard::finished`] and
/// [`ProgressGuard::cancelled`] report their status explicitly, and dropping the guard any other
/// way (an early `?` return, a panic) reports `Failed`. The job is unregistered in all cases.
pub struct ProgressGuard<'a, S: ProgressSink> {
    sink: S,
    jobs: &'a JobRegistry,
    id: String,
    stage: ProgressStage,
    job: watch::Receiver<bool>,
    done: bool,
}

impl<'a, S: ProgressSink> ProgressGuard<'a, S> {
    /// Registers a job under `id` (cancelling any job already running under it) and takes
    /// ownership of its progress reporting.
    pub fn start(sink: S, jobs: &'a JobRegistry, id: String, stage: ProgressStage) -> Self {
        let job = jobs.insert(&id);
        Self {
            sink,
            jobs,
            id,
            stage,
            job,
            done: false,
        }
    }

    /// The job's cancellation token.
    pub fn cancel_token(&self) -> watch::Receiver<bool> {
        self.job.clone()
    }

    pub fn progress(&self, progress: f32) -> Result<(), Error> {
        self.sink
            .publish(self.item(progress, false, ProgressStatus::Running))
    }

    pub fn finished(mut self) -> Result<(), Error> {
        self.end(ProgressStatus::Finished)
    }

    pub fn cancelled(mut self) -> Result<(), Error> {
        self.end(ProgressStatus::Cancelled)
    }

    fn end(&mut self, status: ProgressStatus) -> Result<(), Error> {
        self.done = true;
        let published = self.sink.publish(self.item(100.0, true, status));
        self.jobs.remove(&self.id, &self.job);
        published
    }

    fn item(&self, progress: f32, finished: bool, status: ProgressStatus) -> ProgressItem {
        ProgressItem {
            id: self.id.clone(),
            progress,
            finished,
            stage: Some(self.stage),
            status: Some(status),
        }
    }
}

impl<S: ProgressSink> Drop for ProgressGuard<'_, S> {
    fn drop(&mut self) {
        if !self.done {
            if let Err(e) = self.end(ProgressStatus::Failed) {
                warn!("Failed to report progress for {}: {:?}", self.id, e);
            }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_progress(id: String, state: tauri::State<'_, crate::AppState>) -> Option<ProgressItem> {
    state.progress_state.get(&id).map(|v| v.clone())
}

#[tauri::command]
#[specta::specta]
pub fn clear_progress(id: String, state: tauri::State<'_, crate::AppState>) {
    state.progress_state.remove(&id);
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::jobs::is_cancelled;

    #[derive(Clone, Default)]
    struct Recorder(Rc<RefCell<Vec<ProgressItem>>>);

    impl ProgressSink for Recorder {
        fn publish(&self, item: ProgressItem) -> Result<(), Error> {
            self.0.borrow_mut().push(item);
            Ok(())
        }
    }

    impl Recorder {
        fn statuses(&self) -> Vec<(bool, Option<ProgressStatus>)> {
            self.0
                .borrow()
                .iter()
                .map(|item| (item.finished, item.status))
                .collect()
        }
    }

    #[test]
    fn finished_reports_once_and_unregisters() {
        let recorder = Recorder::default();
        let jobs = JobRegistry::default();
        let guard = ProgressGuard::start(
            recorder.clone(),
            &jobs,
            "report_a".into(),
            ProgressStage::Engine,
        );
        let token = guard.cancel_token();
        guard.progress(50.0).unwrap();
        guard.finished().unwrap();
        assert_eq!(
            recorder.statuses(),
            vec![
                (false, Some(ProgressStatus::Running)),
                (true, Some(ProgressStatus::Finished)),
            ]
        );
        assert!(!jobs.cancel("report_a"));
        assert!(is_cancelled(&token));
    }

    #[test]
    fn cancelled_reports_cancelled() {
        let recorder = Recorder::default();
        let jobs = JobRegistry::default();
        let guard = ProgressGuard::start(
            recorder.clone(),
            &jobs,
            "report_a".into(),
            ProgressStage::Engine,
        );
        guard.cancelled().unwrap();
        assert_eq!(
            recorder.statuses(),
            vec![(true, Some(ProgressStatus::Cancelled))]
        );
    }

    #[test]
    fn dropping_without_an_outcome_reports_failed() {
        let recorder = Recorder::default();
        let jobs = JobRegistry::default();
        let fails = || -> Result<(), Error> {
            let _guard = ProgressGuard::start(
                recorder.clone(),
                &jobs,
                "report_a".into(),
                ProgressStage::Engine,
            );
            Err(Error::EngineDisconnected)
        };
        assert!(fails().is_err());
        assert_eq!(
            recorder.statuses(),
            vec![(true, Some(ProgressStatus::Failed))]
        );
        assert!(!jobs.cancel("report_a"));
    }

    #[test]
    fn a_replaced_run_does_not_unregister_its_successor() {
        let recorder = Recorder::default();
        let jobs = JobRegistry::default();
        let first = ProgressGuard::start(
            recorder.clone(),
            &jobs,
            "report_a".into(),
            ProgressStage::Engine,
        );
        let first_token = first.cancel_token();
        let second = ProgressGuard::start(
            recorder.clone(),
            &jobs,
            "report_a".into(),
            ProgressStage::Engine,
        );
        assert!(is_cancelled(&first_token));
        first.cancelled().unwrap();
        assert!(jobs.cancel("report_a"));
        assert!(is_cancelled(&second.cancel_token()));
    }
}
