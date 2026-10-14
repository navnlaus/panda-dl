use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadKind {
    Http,
    Torrent,
    Youtube,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub info_hash: String,
    pub seeds: usize,
    pub peers: usize,
    pub total_pieces: usize,
    pub finished_pieces: usize,
}

/// One file of a torrent: where it's written, its size, and how much of it is downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProgress {
    pub path: String,
    pub length: u64,
    pub downloaded: u64,
    /// Being downloaded (not skipped).
    #[serde(default = "selected_by_default")]
    pub selected: bool,
}

fn selected_by_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub kind: DownloadKind,
    pub status: DownloadStatus,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub eta_seconds: Option<u64>,
    pub output_path: String,
    pub created_at: u64,
    pub parts: Vec<PartInfo>,
    pub torrent_info: Option<TorrentInfo>,
    /// A torrent's files, once its metadata is known (empty otherwise).
    #[serde(default)]
    pub files: Vec<FileProgress>,
    /// A torrent in folder form goes in its folder inside `output_path`, as the torrent has it
    /// (downloads added before this put its files in `output_path` itself).
    #[serde(default)]
    pub own_folder: bool,
    /// A torrent's files being downloaded, by index (None: all of them).
    #[serde(default)]
    pub only_files: Option<Vec<usize>>,
    /// A torrent's files being checked (when it starts or goes on): how much of it is checked so
    /// far. Its progress stays what it was meanwhile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub active_downloads: usize,
    pub total_download_speed: u64,
    pub total_upload_speed: u64,
    pub downloads: Vec<DownloadTask>,
}
