use std::collections::HashMap;
use std::time::Instant;

use crate::capture::send_notification;

use super::types::{DownloadStatus, DownloadTask, FileProgress};
use super::Engine;

impl Engine {
    /// Background task that fires every second to sync download progress
    /// from the active downloaders into the shared task map and compute speeds.
    pub(super) async fn monitor_loop(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

        loop {
            interval.tick().await;

            let mut tasks = self.tasks.write().await;
            let mut speeds = self.last_speeds.lock().await;

            // ── HTTP tasks ──────────────────────────────────────────────────
            let http_active = self.http_active.lock().await;
            for (id, active) in http_active.iter() {
                if let Some(task) = tasks.get_mut(id) {
                    let parts = active.parts.lock().await;
                    task.parts = parts.clone();
                    let current_downloaded: u64 = parts.iter().map(|p| p.downloaded).sum();
                    drop(parts);

                    task.downloaded_bytes = current_downloaded;
                    Self::update_speed(id, current_downloaded, 0, task, &mut speeds);
                }
            }
            drop(http_active);

            // ── YouTube tasks ────────────────────────────────────────────────
            let youtube_active = self.youtube_active.lock().await;
            for (id, active) in youtube_active.iter() {
                if let Some(task) = tasks.get_mut(id) {
                    let current_downloaded =
                        active.downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
                    task.downloaded_bytes = current_downloaded;
                    Self::update_speed(id, current_downloaded, 0, task, &mut speeds);
                }
            }
            drop(youtube_active);

            // ── Torrent tasks ────────────────────────────────────────────────
            let torrent_active = self.torrent_active.lock().await;
            for (id, handle) in torrent_active.iter() {
                if let Some(task) = tasks.get_mut(id) {
                    let stats = handle.stats();
                    // Its files being checked (it starts, or goes on): progress_bytes is how much
                    // is checked, not downloaded (it went from 0 to 100%, then back to what it
                    // was). What's known stays meanwhile; it says how far the check is.
                    if matches!(stats.state, librqbit::TorrentStatsState::Initializing { .. }) {
                        task.checked_bytes = Some(stats.progress_bytes);
                        task.download_speed = 0;
                        task.upload_speed = 0;
                        task.eta_seconds = None;
                        speeds.remove(id);
                        continue;
                    }
                    task.checked_bytes = None;
                    // Its files (once the metadata is known), each with how much of it is downloaded:
                    // a torrent finishes its files at different times.
                    let folder = handle.output_folder().to_path_buf();
                    let only = handle.only_files();
                    if let Ok(files) = handle.with_metadata(|m| {
                        m.file_infos
                            .iter()
                            .enumerate()
                            .filter(|(_, f)| !f.attrs.padding)
                            .map(|(i, f)| FileProgress {
                                path: folder.join(&f.relative_filename).display().to_string(),
                                length: f.len,
                                downloaded: stats.file_progress.get(i).copied().unwrap_or(0),
                                selected: only.as_ref().map_or(true, |o| o.contains(&i)),
                            })
                            .collect::<Vec<_>>()
                    }) {
                        task.files = files;
                    }
                    task.total_bytes = Some(stats.total_bytes);
                    task.downloaded_bytes = stats.progress_bytes;
                    task.uploaded_bytes = stats.uploaded_bytes;

                    // Update name once the torrent metadata resolves
                    if let Some(name) = handle.name() {
                        if task.name.starts_with("Torrent_") || task.name == "Magnet Download" {
                            task.name = name;
                        }
                    }

                    if stats.finished && task.status == DownloadStatus::Downloading {
                        task.status = DownloadStatus::Completed;
                        send_notification(
                            "Torrent Download Complete",
                            &format!("{} finished downloading!", task.name),
                        );
                    }

                    let now = Instant::now();
                    let (prev_down, prev_up, prev_time) = speeds
                        .get(id)
                        .copied()
                        .unwrap_or((0, 0, now - std::time::Duration::from_secs(1)));

                    let elapsed = now.duration_since(prev_time).as_secs_f64().max(0.1);
                    let delta_down = stats.progress_bytes.saturating_sub(prev_down);
                    let delta_up = stats.uploaded_bytes.saturating_sub(prev_up);

                    // librqbit's own estimate while it's live: smoothed over several seconds.
                    // (progress_bytes grows a whole piece at a time, so a second's growth was 0
                    // now and then, and the speed with it.)
                    match &stats.live {
                        Some(live) => {
                            task.download_speed = live.download_speed.as_bytes();
                            task.upload_speed = live.upload_speed.as_bytes();
                        }
                        None => {
                            task.download_speed = (delta_down as f64 / elapsed) as u64;
                            task.upload_speed = (delta_up as f64 / elapsed) as u64;
                        }
                    }

                    if task.download_speed > 0 && stats.total_bytes > stats.progress_bytes {
                        task.eta_seconds =
                            Some((stats.total_bytes - stats.progress_bytes) / task.download_speed);
                    } else {
                        task.eta_seconds = None;
                    }

                    speeds.insert(id.clone(), (stats.progress_bytes, stats.uploaded_bytes, now));
                }
            }
            drop(torrent_active);
        }
    }

    /// Update download speed and ETA for an HTTP or YouTube task.
    fn update_speed(
        id: &str,
        current_downloaded: u64,
        current_uploaded: u64,
        task: &mut DownloadTask,
        speeds: &mut HashMap<String, (u64, u64, Instant)>,
    ) {
        let now = Instant::now();
        let (prev_down, _prev_up, prev_time) = speeds
            .get(id)
            .copied()
            .unwrap_or((0, 0, now - std::time::Duration::from_secs(1)));

        let elapsed = now.duration_since(prev_time).as_secs_f64().max(0.1);
        let delta_bytes = current_downloaded.saturating_sub(prev_down);
        let speed = (delta_bytes as f64 / elapsed) as u64;

        task.download_speed = speed;
        if let Some(total) = task.total_bytes {
            if speed > 0 && total > current_downloaded {
                task.eta_seconds = Some((total - current_downloaded) / speed);
            } else {
                task.eta_seconds = None;
            }
        }

        speeds.insert(id.to_string(), (current_downloaded, current_uploaded, now));
    }
}
