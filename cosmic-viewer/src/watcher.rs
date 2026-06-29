use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum WatcherEvent {
    Created(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
    Error(String),
}

pub fn watch_directory(dir: Option<PathBuf>) -> Subscription<WatcherEvent> {
    Subscription::run_with(dir, |dir| {
        cosmic::iced::stream::channel(100, {
            let dir = dir.clone();
            move |mut output: cosmic::iced::futures::channel::mpsc::Sender<WatcherEvent>| async move {
                let Some(dir) = dir else {
                    std::future::pending::<()>().await;
                    unreachable!()
                };

                let (tx, mut rx) = mpsc::channel(100);

                let watcher_result = RecommendedWatcher::new(
                    move |res: Result<Event, notify::Error>| {
                        let _ = tx.blocking_send(res);
                    },
                    Config::default(),
                );

                let mut watcher = match watcher_result {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = output.send(WatcherEvent::Error(e.to_string())).await;
                        std::future::pending::<()>().await;
                        unreachable!();
                    }
                };

                if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                    let _ = output.send(WatcherEvent::Error(e.to_string())).await;
                    std::future::pending::<()>().await;
                    unreachable!();
                }

                while let Some(result) = rx.recv().await {
                    match result {
                        Ok(event) => {
                            for path in event.paths {
                                let msg = match event.kind {
                                    EventKind::Create(_) => Some(WatcherEvent::Created(path)),
                                    EventKind::Remove(_) => Some(WatcherEvent::Removed(path)),
                                    EventKind::Modify(_) => Some(WatcherEvent::Modified(path)),
                                    _ => None,
                                };

                                if let Some(msg) = msg {
                                    let _ = output.send(msg).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = output.send(WatcherEvent::Error(e.to_string())).await;
                        }
                    }
                }

                std::future::pending::<()>().await;
                unreachable!()
            }
        })
    })
}
