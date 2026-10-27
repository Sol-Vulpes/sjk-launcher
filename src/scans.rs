//! PK3 scans on a few worker threads, remembered by the file's MD5 for the run.

use crate::pk3scan::{self, Location, Report};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

const WORKERS: usize = 4;

#[derive(Clone)]
pub enum ScanState {
    Queued,
    Done(Arc<Report>),
    Failed(String),
}

#[derive(Clone)]
pub struct Scanner {
    states: Arc<Mutex<HashMap<[u8; 16], ScanState>>>,
    queue: Sender<([u8; 16], Location)>,
}

impl Scanner {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        let (queue, jobs) = channel::<([u8; 16], Location)>();
        let jobs: Arc<Mutex<Receiver<_>>> = Arc::new(Mutex::new(jobs));
        let states: Arc<Mutex<HashMap<[u8; 16], ScanState>>> = Arc::default();
        for _ in 0..WORKERS {
            let (jobs, states, ctx) = (Arc::clone(&jobs), Arc::clone(&states), ctx.clone());
            std::thread::spawn(move || {
                loop {
                    let job = jobs.lock().unwrap_or_else(|error| error.into_inner()).recv();
                    let Ok((key, location)) = job else { return };
                    let state = match pk3scan::scan_location(&location) {
                        Ok(report) => ScanState::Done(Arc::new(report)),
                        Err(error) => ScanState::Failed(error),
                    };
                    states.lock().unwrap_or_else(|error| error.into_inner()).insert(key, state);
                    ctx.request_repaint();
                }
            });
        }
        Self { states, queue }
    }

    /// Scan unless it is scanned or queued already; a failed scan is tried again.
    pub fn request(&self, key: [u8; 16], location: Location) {
        let mut states = self.states.lock().unwrap_or_else(|error| error.into_inner());
        if matches!(states.get(&key), None | Some(ScanState::Failed(_))) {
            states.insert(key, ScanState::Queued);
            let _ = self.queue.send((key, location));
        }
    }

    pub fn state(&self, key: &[u8; 16]) -> Option<ScanState> {
        self.states.lock().unwrap_or_else(|error| error.into_inner()).get(key).cloned()
    }
}
