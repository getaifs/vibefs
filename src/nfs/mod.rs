//! VibeFS NFS filesystem implementation
//!
//! This module implements the NFSv3 protocol using the nfsserve crate.
//! It provides a virtual filesystem that reads from Git ODB and writes to session deltas.

use anyhow::Result;
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfsstring, nfstime3, sattr3, specdata3,
};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{InodeMetadata, MetadataStore};
use crate::git::GitRepo;

/// Root inode is always 1
const ROOT_INODE: fileid3 = 1;

/// VibeFS NFS filesystem implementation
pub struct VibeNFS {
    metadata: Arc<RwLock<MetadataStore>>,
    git: Arc<RwLock<GitRepo>>,
    session_dir: PathBuf,
    #[allow(dead_code)]
    vibe_id: String,
    /// Cache of parent -> children mappings for directory enumeration
    dir_children: Arc<RwLock<HashMap<fileid3, Vec<fileid3>>>>,
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
            dir_children: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the directory children cache from metadata store
    pub async fn build_directory_cache(&self) -> Result<()> {
        let store = self.metadata.read().await;
        let mut cache = self.dir_children.write().await;

        // Get all inodes and build parent-child relationships
        let mut all_entries: Vec<(fileid3, InodeMetadata)> = Vec::new();

        // We need to iterate through all inodes - this is a simplified approach
        // In production, we'd want a more efficient index in RocksDB
        for i in 1..=10000 {
            if let Ok(Some(meta)) = store.get_inode(i) {
                all_entries.push((i, meta));
            }
        }

        // Build directory tree
        for (inode, meta) in &all_entries {
            let path = Path::new(&meta.path);

            // Determine parent inode
            let parent_inode = if let Some(parent_path) = path.parent() {
                let parent_str = parent_path.to_string_lossy();
                if parent_str.is_empty() {
                    ROOT_INODE
                } else {
                    store
                        .get_inode_by_path(&parent_str)?
                        .unwrap_or(ROOT_INODE)
                }
            } else {
                ROOT_INODE
            };

            cache.entry(parent_inode).or_default().push(*inode);
        }

        Ok(())
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
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            size: metadata.size,
            used: metadata.size,
            rdev: specdata3 {
                specdata1: 0,
                specdata2: 0,
            },
            fsid: 1,
            fileid: inode,
            atime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
            mtime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
            ctime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
        }
    }

    /// Create the root directory fattr
    fn root_fattr(&self) -> fattr3 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();

        fattr3 {
            ftype: ftype3::NF3DIR,
            mode: 0o755,
            nlink: 2,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            size: 4096,
            used: 4096,
            rdev: specdata3 {
                specdata1: 0,
                specdata2: 0,
            },
            fsid: 1,
            fileid: ROOT_INODE,
            atime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
            mtime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
            ctime: nfstime3 {
                seconds: now.as_secs() as u32,
                nseconds: 0,
            },
        }
    }

    /// Add a child to a directory's children cache
    async fn add_child_to_cache(&self, parent_inode: fileid3, child_inode: fileid3) {
        let mut cache = self.dir_children.write().await;
        cache.entry(parent_inode).or_default().push(child_inode);
    }

    /// Remove a child from a directory's children cache
    async fn remove_child_from_cache(&self, parent_inode: fileid3, child_inode: fileid3) {
        let mut cache = self.dir_children.write().await;
        if let Some(children) = cache.get_mut(&parent_inode) {
            children.retain(|&id| id != child_inode);
        }
    }

    /// Convert string to nfsstring (filename3)
    fn to_nfsstring(s: &str) -> nfsstring {
        nfsstring(s.as_bytes().to_vec())
    }
}

#[async_trait::async_trait]
impl NFSFileSystem for VibeNFS {
    fn root_dir(&self) -> fileid3 {
        ROOT_INODE
    }

    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let name = String::from_utf8_lossy(&filename.0).to_string();

        // Handle . and ..
        if name == "." {
            return Ok(dirid);
        }
        if name == ".." {
            return Ok(ROOT_INODE);
        }

        // Get parent directory path
        let full_path = if dirid == ROOT_INODE {
            PathBuf::from(&name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let (inode, _metadata) = self
            .get_metadata_by_path(&full_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        Ok(inode)
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        if id == ROOT_INODE {
            return Ok(self.root_fattr());
        }

        let metadata = self
            .get_metadata_by_inode(id)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        Ok(self.metadata_to_fattr(id, &metadata))
    }

    async fn setattr(&self, id: fileid3, _setattr: sattr3) -> Result<fattr3, nfsstat3> {
        self.getattr(id).await
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let metadata = self
            .get_metadata_by_inode(id)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        if metadata.is_dir {
            return Err(nfsstat3::NFS3ERR_ISDIR);
        }

        // Check if file is dirty (modified in session)
        let store = self.metadata.read().await;
        let is_dirty = store
            .is_dirty(&metadata.path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        let data = if is_dirty {
            // Read from session directory
            let session_path = self.get_session_path(Path::new(&metadata.path)).await;
            tokio::fs::read(&session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else if let Some(oid) = &metadata.git_oid {
            // Read from Git ODB
            let git = self.git.read().await;
            git.read_blob(oid).map_err(|_| nfsstat3::NFS3ERR_IO)?
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
        let metadata = self
            .get_metadata_by_inode(id)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        if metadata.is_dir {
            return Err(nfsstat3::NFS3ERR_ISDIR);
        }

        // Write to session directory
        let session_path = self.get_session_path(Path::new(&metadata.path)).await;
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        // Read existing content if offset > 0
        let mut existing = if offset > 0 && session_path.exists() {
            tokio::fs::read(&session_path)
                .await
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

        tokio::fs::write(&session_path, &existing)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Mark as dirty
        let store = self.metadata.write().await;
        store
            .mark_dirty(&metadata.path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update size
        let new_size = existing.len() as u64;
        let mut updated_metadata = metadata.clone();
        updated_metadata.size = new_size;

        let store = self.metadata.write().await;
        store
            .put_inode(id, &updated_metadata)
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
        let name = String::from_utf8_lossy(&filename.0).to_string();

        let full_path = if dirid == ROOT_INODE {
            PathBuf::from(&name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let store = self.metadata.write().await;
        let new_inode = store
            .next_inode_id()
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let metadata = InodeMetadata {
            path: full_path.to_string_lossy().to_string(),
            git_oid: None,
            is_dir: false,
            size: 0,
            volatile: false,
        };

        store
            .put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Mark as dirty since it's a new file
        store
            .mark_dirty(&metadata.path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update directory cache
        self.add_child_to_cache(dirid, new_inode).await;

        // Create empty file in session
        let session_path = self.get_session_path(&full_path).await;
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }
        tokio::fs::write(&session_path, b"")
            .await
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
        let name = String::from_utf8_lossy(&dirname.0).to_string();

        let full_path = if dirid == ROOT_INODE {
            PathBuf::from(&name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let store = self.metadata.write().await;
        let new_inode = store
            .next_inode_id()
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let metadata = InodeMetadata {
            path: full_path.to_string_lossy().to_string(),
            git_oid: None,
            is_dir: true,
            size: 0,
            volatile: false,
        };

        store
            .put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update directory cache
        self.add_child_to_cache(dirid, new_inode).await;

        // Create directory in session
        let session_path = self.get_session_path(&full_path).await;
        tokio::fs::create_dir_all(&session_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let fattr = self.metadata_to_fattr(new_inode, &metadata);
        Ok((new_inode, fattr))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let name = String::from_utf8_lossy(&filename.0).to_string();

        let full_path = if dirid == ROOT_INODE {
            PathBuf::from(&name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let (inode, _) = self
            .get_metadata_by_path(&full_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let store = self.metadata.write().await;
        store
            .delete_inode(inode)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update directory cache
        self.remove_child_from_cache(dirid, inode).await;

        // Remove from session directory
        let session_path = self.get_session_path(&full_path).await;
        if session_path.exists() {
            tokio::fs::remove_file(&session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        Ok(())
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let from_name = String::from_utf8_lossy(&from_filename.0).to_string();
        let to_name = String::from_utf8_lossy(&to_filename.0).to_string();

        // Get source path
        let from_path = if from_dirid == ROOT_INODE {
            PathBuf::from(&from_name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(from_dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&from_name)
        };

        // Get destination path
        let to_path = if to_dirid == ROOT_INODE {
            PathBuf::from(&to_name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(to_dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&to_name)
        };

        // Get source inode and metadata
        let (inode, mut metadata) = self
            .get_metadata_by_path(&from_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        // Update metadata with new path
        metadata.path = to_path.to_string_lossy().to_string();

        let store = self.metadata.write().await;
        store
            .put_inode(inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update directory cache
        self.remove_child_from_cache(from_dirid, inode).await;
        self.add_child_to_cache(to_dirid, inode).await;

        // Move file in session directory if it exists
        let from_session = self.get_session_path(&from_path).await;
        let to_session = self.get_session_path(&to_path).await;
        if from_session.exists() {
            if let Some(parent) = to_session.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            }
            tokio::fs::rename(&from_session, &to_session)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        Ok(())
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        // Get directory metadata (for non-root)
        if dirid != ROOT_INODE {
            let metadata = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;

            if !metadata.is_dir {
                return Err(nfsstat3::NFS3ERR_NOTDIR);
            }
        }

        let mut entries = Vec::new();

        // Add . entry
        if start_after == 0 {
            let dot_attr = if dirid == ROOT_INODE {
                self.root_fattr()
            } else {
                self.getattr(dirid).await?
            };
            entries.push(DirEntry {
                fileid: dirid,
                name: Self::to_nfsstring("."),
                attr: dot_attr,
            });
        }

        // Add .. entry
        if start_after <= 1 && entries.len() < max_entries {
            entries.push(DirEntry {
                fileid: ROOT_INODE,
                name: Self::to_nfsstring(".."),
                attr: self.root_fattr(),
            });
        }

        // Get children from cache
        let cache = self.dir_children.read().await;
        if let Some(children) = cache.get(&dirid) {
            let store = self.metadata.read().await;

            for &child_inode in children {
                if child_inode <= start_after {
                    continue;
                }
                if entries.len() >= max_entries {
                    return Ok(ReadDirResult {
                        entries,
                        end: false,
                    });
                }

                if let Ok(Some(child_meta)) = store.get_inode(child_inode) {
                    // Extract just the filename from the path
                    let filename = Path::new(&child_meta.path)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    entries.push(DirEntry {
                        fileid: child_inode,
                        name: Self::to_nfsstring(&filename),
                        attr: self.metadata_to_fattr(child_inode, &child_meta),
                    });
                }
            }
        }

        Ok(ReadDirResult { entries, end: true })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let name = String::from_utf8_lossy(&linkname.0).to_string();
        let target = String::from_utf8_lossy(&symlink.0).to_string();

        let full_path = if dirid == ROOT_INODE {
            PathBuf::from(&name)
        } else {
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            PathBuf::from(&dir_meta.path).join(&name)
        };

        let store = self.metadata.write().await;
        let new_inode = store
            .next_inode_id()
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Store symlink target in git_oid field (temporary solution)
        let metadata = InodeMetadata {
            path: full_path.to_string_lossy().to_string(),
            git_oid: Some(format!("symlink:{}", target)),
            is_dir: false,
            size: target.len() as u64,
            volatile: true,
        };

        store
            .put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Update directory cache
        self.add_child_to_cache(dirid, new_inode).await;

        // Create symlink in session
        let session_path = self.get_session_path(&full_path).await;
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        #[cfg(unix)]
        {
            tokio::fs::symlink(&target, &session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        let mut fattr = self.metadata_to_fattr(new_inode, &metadata);
        fattr.ftype = ftype3::NF3LNK;
        Ok((new_inode, fattr))
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let metadata = self
            .get_metadata_by_inode(id)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        // Check if this is a symlink (stored with symlink: prefix in git_oid)
        if let Some(oid) = &metadata.git_oid {
            if let Some(target) = oid.strip_prefix("symlink:") {
                return Ok(nfsstring(target.as_bytes().to_vec()));
            }
        }

        // Try reading from session directory
        let session_path = self.get_session_path(Path::new(&metadata.path)).await;
        if session_path.is_symlink() {
            let target = tokio::fs::read_link(&session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            return Ok(nfsstring(target.to_string_lossy().as_bytes().to_vec()));
        }

        Err(nfsstat3::NFS3ERR_INVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_vibe_nfs_root() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();

        // Initialize a git repo for testing
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("test.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let metadata = MetadataStore::open(&db_path).unwrap();
        let git = GitRepo::open(&repo_dir).unwrap();

        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata)),
            Arc::new(RwLock::new(git)),
            session_dir,
            "test".to_string(),
        );

        // Test root directory
        assert_eq!(nfs.root_dir(), ROOT_INODE);

        let root_attr = nfs.getattr(ROOT_INODE).await.unwrap();
        // ftype3 doesn't implement PartialEq, so check mode instead
        assert_eq!(root_attr.mode, 0o755);
        assert_eq!(root_attr.fileid, ROOT_INODE);
    }
}
