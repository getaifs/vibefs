use anyhow::Result;
use nfsserve::nfs::{fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, specdata3};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{InodeMetadata, MetadataStore};
use crate::git::GitRepo;

/// VibeFS NFS filesystem implementation
pub struct VibeNFS {
    metadata: Arc<RwLock<MetadataStore>>,
    git: Arc<RwLock<GitRepo>>,
    session_dir: PathBuf,
    vibe_id: String,
}

impl VibeNFS {
    pub fn new(
        metadata: Arc<RwLock<MetadataStore>>,
        git: Arc<RwLock<GitRepo>>,
        session_dir: PathBuf,
        vibe_id: String,
    ) -> Self {
        Self {
            metadata,
            git,
            session_dir,
            vibe_id,
        }
    }

    async fn get_session_path(&self, path: &Path) -> PathBuf {
        self.session_dir.join(path)
    }

    async fn get_metadata_by_inode(&self, inode: fileid3) -> Result<Option<InodeMetadata>> {
        let store = self.metadata.read().await;
        store.get_inode(inode)
    }

    async fn get_metadata_by_path(&self, path: &Path) -> Result<Option<(fileid3, InodeMetadata)>> {
        let path_str = path.to_string_lossy().to_string();
        let store = self.metadata.read().await;

        if let Some(inode_id) = store.get_inode_by_path(&path_str)? {
            if let Some(metadata) = store.get_inode(inode_id)? {
                return Ok(Some((inode_id, metadata)));
            }
        }

        Ok(None)
    }

    fn metadata_to_fattr(&self, inode: fileid3, metadata: &InodeMetadata) -> fattr3 {
        let ftype = if metadata.is_dir {
            ftype3::NF3DIR
        } else {
            ftype3::NF3REG
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();

        fattr3 {
            ftype,
            mode: if metadata.is_dir { 0o755 } else { 0o644 },
            nlink: 1,
            uid: 1000,
            gid: 1000,
            size: metadata.size,
            used: metadata.size,
            rdev: specdata3 { major: 0, minor: 0 },
            fsid: 1,
            fileid: inode,
            atime: nfstime3 { seconds: now.as_secs() as u32, nseconds: 0 },
            mtime: nfstime3 { seconds: now.as_secs() as u32, nseconds: 0 },
            ctime: nfstime3 { seconds: now.as_secs() as u32, nseconds: 0 },
        }
    }
}

#[async_trait::async_trait]
impl NFSFileSystem for VibeNFS {
    fn root_dir(&self) -> fileid3 {
        1 // Root directory always has inode 1
    }

    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_meta = self.get_metadata_by_inode(dirid).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let name = String::from_utf8_lossy(filename).to_string();
        let full_path = if dirid == 1 {
            PathBuf::from(&name)
        } else {
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let (inode, metadata) = self.get_metadata_by_path(&full_path).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let fattr = self.metadata_to_fattr(inode, &metadata);
        Ok((inode, fattr))
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        let metadata = self.get_metadata_by_inode(id).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        Ok(self.metadata_to_fattr(id, &metadata))
    }

    async fn setattr(&self, id: fileid3, _setattr: sattr3) -> Result<fattr3, nfsstat3> {
        // For now, just return current attributes
        self.getattr(id).await
    }

    async fn read(&self, id: fileid3, offset: u64, count: u32) -> Result<(Vec<u8>, bool), nfsstat3> {
        let metadata = self.get_metadata_by_inode(id).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        // Check if file is dirty (modified in session)
        let store = self.metadata.read().await;
        let is_dirty = store.is_dirty(&metadata.path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        let data = if is_dirty {
            // Read from session directory
            let session_path = self.get_session_path(Path::new(&metadata.path)).await;
            tokio::fs::read(&session_path).await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else if let Some(oid) = &metadata.git_oid {
            // Read from Git ODB
            let git = self.git.read().await;
            git.read_blob(oid)
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else {
            Vec::new()
        };

        let start = offset as usize;
        let end = std::cmp::min(start + count as usize, data.len());
        let chunk = if start < data.len() {
            data[start..end].to_vec()
        } else {
            Vec::new()
        };

        let eof = end >= data.len();
        Ok((chunk, eof))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let metadata = self.get_metadata_by_inode(id).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        // Write to session directory
        let session_path = self.get_session_path(Path::new(&metadata.path)).await;
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        // Read existing content if offset > 0
        let mut existing = if offset > 0 && session_path.exists() {
            tokio::fs::read(&session_path).await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else if offset > 0 && metadata.git_oid.is_some() {
            let git = self.git.read().await;
            git.read_blob(metadata.git_oid.as_ref().unwrap())
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else {
            Vec::new()
        };

        // Extend if necessary
        if offset as usize > existing.len() {
            existing.resize(offset as usize, 0);
        }

        // Write data at offset
        let end = offset as usize + data.len();
        if end > existing.len() {
            existing.resize(end, 0);
        }
        existing[offset as usize..end].copy_from_slice(data);

        tokio::fs::write(&session_path, &existing).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Mark as dirty
        let store = self.metadata.write().await;
        store.mark_dirty(&metadata.path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update size
        let new_size = existing.len() as u64;
        let mut updated_metadata = metadata.clone();
        updated_metadata.size = new_size;

        let store = self.metadata.write().await;
        store.put_inode(id, &updated_metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        Ok(self.metadata_to_fattr(id, &updated_metadata))
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        _attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_meta = self.get_metadata_by_inode(dirid).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let name = String::from_utf8_lossy(filename).to_string();
        let full_path = if dirid == 1 {
            PathBuf::from(&name)
        } else {
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let store = self.metadata.write().await;
        let new_inode = store.next_inode_id()
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let metadata = InodeMetadata {
            path: full_path.to_string_lossy().to_string(),
            git_oid: None,
            is_dir: false,
            size: 0,
            volatile: false,
        };

        store.put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Create empty file in session
        let session_path = self.get_session_path(&full_path).await;
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }
        tokio::fs::write(&session_path, b"").await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let fattr = self.metadata_to_fattr(new_inode, &metadata);
        Ok((new_inode, fattr))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        let (inode, _) = self.create(dirid, filename, sattr3::default()).await?;
        Ok(inode)
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_meta = self.get_metadata_by_inode(dirid).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let name = String::from_utf8_lossy(dirname).to_string();
        let full_path = if dirid == 1 {
            PathBuf::from(&name)
        } else {
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let store = self.metadata.write().await;
        let new_inode = store.next_inode_id()
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let metadata = InodeMetadata {
            path: full_path.to_string_lossy().to_string(),
            git_oid: None,
            is_dir: true,
            size: 0,
            volatile: false,
        };

        store.put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Create directory in session
        let session_path = self.get_session_path(&full_path).await;
        tokio::fs::create_dir_all(&session_path).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let fattr = self.metadata_to_fattr(new_inode, &metadata);
        Ok((new_inode, fattr))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let dir_meta = self.get_metadata_by_inode(dirid).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let name = String::from_utf8_lossy(filename).to_string();
        let full_path = if dirid == 1 {
            PathBuf::from(&name)
        } else {
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let (inode, _) = self.get_metadata_by_path(&full_path).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let store = self.metadata.write().await;
        store.delete_inode(inode)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Remove from session directory
        let session_path = self.get_session_path(&full_path).await;
        if session_path.exists() {
            tokio::fs::remove_file(&session_path).await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        Ok(())
    }

    async fn rename(
        &self,
        _from_dirid: fileid3,
        _from_filename: &filename3,
        _to_dirid: fileid3,
        _to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        // Simplified implementation
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        _start_after: fileid3,
        _max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let metadata = self.get_metadata_by_inode(dirid).await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        if !metadata.is_dir {
            return Err(nfsstat3::NFS3ERR_NOTDIR);
        }

        // Read directory contents from metadata store
        let mut entries = Vec::new();
        let store = self.metadata.read().await;

        // Find all children of this directory
        // This is a simplified implementation - in production, we'd need proper directory tracking
        let prefix = if dirid == 1 {
            String::new()
        } else {
            format!("{}/", metadata.path)
        };

        entries.push(DirEntry {
            fileid: dirid,
            name: ".".into(),
            attr: self.metadata_to_fattr(dirid, &metadata),
        });

        entries.push(DirEntry {
            fileid: dirid,
            name: "..".into(),
            attr: self.metadata_to_fattr(dirid, &metadata),
        });

        Ok(ReadDirResult {
            entries,
            end: true,
        })
    }

    async fn symlink(
        &self,
        _dirid: fileid3,
        _linkname: &filename3,
        _symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }

    async fn readlink(&self, _id: fileid3) -> Result<nfspath3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }
}
