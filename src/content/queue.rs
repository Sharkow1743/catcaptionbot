use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use flume::{Receiver, bounded};
use reqwest::Client;

use crate::AppState;
use crate::text::TextComposer;

use super::generate_content;
use super::rarity::RarityManager;
use super::types::Content;

fn spawn_worker(
    app_state: Arc<AppState>,
    tx: flume::Sender<Content>,
    target: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
) {
    active.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        loop {
            let item = if let Ok(item) = generate_content(&app_state).await {
                item
            } else {
                continue;
            };
            if tx.send_async(item).await.is_err() {
                break;
            }
            if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                break;
            }
        }
        active.fetch_sub(1, Ordering::Relaxed);
    });
}

pub fn spawn_queue(app_state: AppState, size: usize, min: usize, max: usize) -> Receiver<Content> {
    let app_state = Arc::new(app_state);

    let (tx, rx) = bounded::<Content>(size);

    let target = Arc::new(AtomicUsize::new(min));
    let active = Arc::new(AtomicUsize::new(0));

    for _ in 0..min {
        spawn_worker(
            app_state.clone(),
            tx.clone(),
            target.clone(),
            active.clone(),
        );
    }

    {
        let tx = tx.clone();
        let target = target.clone();
        let active = active.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if tx.is_disconnected() {
                    break;
                }

                let free = tx.capacity().unwrap() - tx.len();
                let cur = target.load(Ordering::Relaxed);

                if free > tx.capacity().unwrap() * 3 / 4 && cur < max {
                    target.store(cur + 1, Ordering::Relaxed);
                    spawn_worker(
                        app_state.clone(),
                        tx.clone(),
                        target.clone(),
                        active.clone(),
                    );
                } else if free < tx.capacity().unwrap() / 4 && cur > min {
                    target.store(cur - 1, Ordering::Relaxed); // one worker retires itself
                }
            }
        });
    }

    rx
}
