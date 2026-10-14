use anyhow::{bail, Context, Result};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions,
};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub struct TorrentManager {
    session: Arc<Session>,
    output_dir: PathBuf,
}

impl TorrentManager {
    pub async fn new(output_dir: PathBuf) -> Result<Self> {
        let opts = SessionOptions::default();
        let session = Session::new_with_opts(output_dir.clone(), opts)
            .await
            .context("Failed to initialize librqbit session")?;

        Ok(Self {
            session,
            output_dir,
        })
    }

    #[allow(dead_code)]
    pub fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }

    /// Adds a torrent, downloading into `custom_dir` (or the default folder). `own_folder`: as the
    /// torrent has it, a torrent in folder form in its folder there, a single file by itself (librqbit
    /// puts a folder's files straight into the folder it's given): its details are read first (a
    /// magnet's metadata fetched), then it's added with the peers found meanwhile.
    pub async fn add(&self, uri: &str, custom_dir: Option<&Path>, own_folder: bool, only_files: Option<Vec<usize>>) -> Result<Arc<ManagedTorrent>> {
        let mut folder = custom_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| self.output_dir.clone());
        let mut initial_peers = None;
        if own_folder {
            let listed = self.session.add_torrent(
                AddTorrent::from_cli_argument(uri)?,
                Some(AddTorrentOptions { list_only: true, ..Default::default() }),
            ).await.context("Failed to read the torrent's details")?;
            match listed {
                AddTorrentResponse::ListOnly(listed) => {
                    // A torrent in folder form (files listed, not one `length`): its files are paths in the
                    // folder `name`, as the torrent has them (librqbit keeps their own subfolders).
                    if listed.info.info().length.is_none() {
                        if let Some(name) = listed.info.name() {
                            let name = Path::new(name.as_ref());
                            let mut parts = name.components();
                            if let (Some(Component::Normal(_)), None) = (parts.next(), parts.next()) {
                                folder = folder.join(name); // a plain name: no path in it
                            }
                        }
                    }
                    initial_peers = Some(listed.seen_peers);
                }
                AddTorrentResponse::AlreadyManaged(_, handle) | AddTorrentResponse::Added(_, handle) => return Ok(handle),
            }
        }
        let opts = AddTorrentOptions {
            output_folder: Some(folder.display().to_string()),
            overwrite: true,
            initial_peers,
            only_files,
            ..Default::default()
        };

        let add = AddTorrent::from_cli_argument(uri)?;
        let resp = self.session.add_torrent(add, Some(opts)).await
            .context("Failed to add torrent to librqbit session")?;

        match resp.into_handle() {
            Some(handle) => Ok(handle),
            None => bail!("Could not get handle for torrent"),
        }
    }

    /// Downloads only these of its files (by index).
    pub async fn update_only_files(&self, handle: &Arc<ManagedTorrent>, only: &std::collections::HashSet<usize>) -> Result<()> {
        self.session.update_only_files(handle, only).await
            .context("Failed to change which files are downloaded")?;
        Ok(())
    }

    pub async fn pause(&self, handle: &Arc<ManagedTorrent>) -> Result<()> {
        self.session.pause(handle).await
            .context("Failed to pause torrent")?;
        Ok(())
    }

    pub async fn unpause(&self, handle: &Arc<ManagedTorrent>) -> Result<()> {
        self.session.unpause(handle).await
            .context("Failed to unpause torrent")?;
        Ok(())
    }

    pub async fn remove(&self, handle: &ManagedTorrent, delete_files: bool) -> Result<()> {
        let id = handle.id();
        self.session.delete(librqbit::api::TorrentIdOrHash::Id(id), delete_files).await
            .context("Failed to delete torrent")?;
        Ok(())
    }
}
