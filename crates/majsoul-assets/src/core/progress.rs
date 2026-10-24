use anyhow::{Result, ensure};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
type Observer = Arc<dyn Fn(&Snapshot) + Send + Sync>;
#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    pub active: bool,
    pub operation: String,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}
#[derive(Default)]
pub struct Jobs {
    snapshot: Arc<Mutex<Snapshot>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    observer: Option<Observer>,
}
pub struct Task {
    snapshot: Arc<Mutex<Snapshot>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    observer: Option<Observer>,
}
impl Jobs {
    pub fn cancellation_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn with_observer(observer: impl Fn(&Snapshot) + Send + Sync + 'static) -> Self {
        Self {
            observer: Some(Arc::new(observer)),
            ..Self::default()
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn begin(&self, operation: &str) -> Result<Task> {
        ensure!(
            self.busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "另一个任务正在进行，请等待完成"
        );
        self.cancel.store(false, Ordering::SeqCst);
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Snapshot {
            active: true,
            operation: operation.into(),
            ..Default::default()
        };
        Ok(Task {
            snapshot: self.snapshot.clone(),
            busy: self.busy.clone(),
            cancel: self.cancel.clone(),
            observer: self.observer.clone(),
        })
    }
    pub fn cancel_export(&self) {
        let state = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if state.active && state.operation == "export" {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }
}
impl Task {
    pub fn update(&self, label: &str, done: usize, total: usize, current: &str) {
        let mut state = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        state.label = label.into();
        state.done = done;
        state.total = total;
        state.current = current.into();
        if let Some(observer) = &self.observer {
            let snapshot = state.clone();
            drop(state);
            observer(&snapshot);
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active = false;
        self.busy.store(false, Ordering::SeqCst);
    }
}
