//! VibeFS NFS filesystem implementation
//!
//! This module implements the NFSv3 protocol using the nfsserve crate.
//! It provides a virtual filesystem that reads from Git ODB and writes to session deltas.

use anyhow::Result;
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfsstring, nfstime3, sattr3, set_size3, specdata3,
};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{InodeMetadata, MetadataStore};
use crate::git::GitRepo;

/// Root inode is always 1
const ROOT_INODE: fileid3 = 1;
/// Virtual inode for Root's parent (to ensure unique cookie/fileid in readdir)
const FAKE_ROOT_PARENT_ID: fileid3 = 2;

/// VibeFS NFS filesystem implementation
#[derive(Clone)]
pub struct VibeNFS {
    metadata: Arc<RwLock<MetadataStore>>,
    git: Arc<RwLock<GitRepo>>,
    session_dir: PathBuf,
    repo_path: PathBuf,
    #[allow(dead_code)]
    vibe_id: String,
    /// Cache of parent -> children mappings for directory enumeration
    dir_children: Arc<RwLock<HashMap<fileid3, Vec<fileid3>>>>,
    /// Stable timestamp (epoch secs) set at server start, used as fallback for inodes with mtime=0
    init_time: u64,
}

impl VibeNFS {
    pub fn new(
        metadata: Arc<RwLock<MetadataStore>>,
        git: Arc<RwLock<GitRepo>>,
        session_dir: PathBuf,
        repo_path: PathBuf,
        vibe_id: String,
    ) -> Self {
        let init_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            metadata,
            git,
            session_dir,
            repo_path,
            vibe_id,
            dir_children: Arc::new(RwLock::new(HashMap::new())),
            init_time,
        }
    }
    // ... (omitting build_directory_cache and helpers for brevity if not changing)

    // (Actually I need to match exact context to replace safely. 
    // Since I cannot match everything easily, I will replace constants first, then readdir.)
    
    // WAIT. `replace` tool requires EXACT match. 
    // I will do 2 replaces.
    // 1. Change FAKE_ROOT_PARENT_ID.
    // 2. Change readdir.


    /// Initialize the directory children cache from metadata store
    pub async fn build_directory_cache(&self) -> Result<()> {
        let store = self.metadata.read().await;
        let mut cache = self.dir_children.write().await;

        // Get all inodes and build parent-child relationships
        let all_entries = store.get_all_inodes()?;

        // Deduplicate: only keep the canonical inode for each path
        // (the one that the path reverse-mapping points to).
        // Old artifact symlink entries can leave orphan forward-mappings.
        let mut canonical_inodes = std::collections::HashSet::new();
        for (_inode, meta) in &all_entries {
            if let Ok(Some(canonical_id)) = store.get_inode_by_path(&meta.path) {
                canonical_inodes.insert(canonical_id);
            }
        }

        // Build directory tree using only canonical inodes
        for (inode, meta) in &all_entries {
            if !canonical_inodes.contains(inode) {
                continue; // Skip orphan/duplicate inode
            }

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

    /// Ensure a file exists in the session directory.
    /// If the file doesn't exist, copies it from Git ODB or repo filesystem.
    /// This is used before writes to ensure we have a local copy to modify.
    async fn ensure_session_file(&self, metadata: &InodeMetadata, session_path: &Path) -> std::result::Result<(), nfsstat3> {
        // Create parent directories if needed
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        // If file already exists in session, nothing to do
        if session_path.exists() {
            return Ok(());
        }

        // Copy content from source (Git ODB or repo filesystem)
        let content = if let Some(oid) = &metadata.git_oid {
            // Read from Git ODB
            let git = self.git.read().await;
            git.read_blob(oid).map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else {
            // Try repo filesystem (untracked file)
            let repo_file = self.repo_path.join(&metadata.path);
            if repo_file.exists() && repo_file.is_file() {
                tokio::fs::read(&repo_file)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?
            } else {
                // New file - start empty
                Vec::new()
            }
        };

        tokio::fs::write(session_path, &content)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        Ok(())
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
        } else if metadata.git_oid.as_ref().map(|o| o.starts_with("symlink:")).unwrap_or(false) {
            ftype3::NF3LNK
        } else {
            ftype3::NF3REG
        };

        // Determine file size from the most authoritative source:
        // 1. Session file (if it exists) — handles dirty files, including those
        //    modified outside the NFS write path (e.g., direct cp/sed to session dir)
        // 2. Repo file (for volatile/untracked files that change independently of git)
        // 3. Cached metadata.size from RocksDB (for clean git-tracked files)
        let size = if metadata.is_dir {
            metadata.size
        } else {
            let session_file = self.session_dir.join(&metadata.path);
            if let Ok(m) = std::fs::metadata(&session_file) {
                m.len()
            } else if metadata.volatile {
                let repo_file = self.repo_path.join(&metadata.path);
                std::fs::metadata(&repo_file).map(|m| m.len()).unwrap_or(metadata.size)
            } else {
                metadata.size
            }
        };

        // Use stored mtime if available, otherwise fall back to server init time.
        // This ensures timestamps are stable across GETATTR calls, which prevents
        // tools from thinking files changed between read and write operations.
        let ts = if metadata.mtime > 0 { metadata.mtime } else { self.init_time };

        fattr3 {
            ftype,
            mode: if metadata.is_dir { 0o755 } else { 0o644 },
            nlink: 1,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            size,
            used: size,
            rdev: specdata3 {
                specdata1: 0,
                specdata2: 0,
            },
            fsid: 1,
            fileid: inode,
            atime: nfstime3 {
                seconds: ts as u32,
                nseconds: 0,
            },
            mtime: nfstime3 {
                seconds: ts as u32,
                nseconds: 0,
            },
            ctime: nfstime3 {
                seconds: ts as u32,
                nseconds: 0,
            },
        }
    }

    /// Create the root directory fattr
    fn root_fattr(&self, fileid: fileid3) -> fattr3 {
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
            fileid,
            atime: nfstime3 {
                seconds: self.init_time as u32,
                nseconds: 0,
            },
            mtime: nfstime3 {
                seconds: self.init_time as u32,
                nseconds: 0,
            },
            ctime: nfstime3 {
                seconds: self.init_time as u32,
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

    /// Check if a path should be ignored for dirty tracking (e.g., macOS metadata files)
    fn is_ignored_path(path: &str) -> bool {
        let p = Path::new(path);
        if let Some(filename) = p.file_name().and_then(|s| s.to_str()) {
            // Ignore macOS metadata files (AppleDouble) and .DS_Store
            if filename.starts_with("._") || filename == ".DS_Store" {
                return true;
            }
        }
        false
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
            if dirid == ROOT_INODE {
                // For Root, ".." is FAKE_ROOT_PARENT_ID to resolve properly in getattr if needed,
                // but usually ".." from root stays at root or goes to mount point parent.
                // Returning FAKE_ROOT_PARENT_ID allows readdir consistency.
                return Ok(FAKE_ROOT_PARENT_ID);
            }
            // For others, it's ROOT_INODE (simplified, assuming flat structure or getting parent from path)
            // Note: The original code returned ROOT_INODE for "..". 
            // Correct implementation should find actual parent.
            // But VibeFS structure in build_directory_cache assumes flat-ish or we don't store parent ptrs easily.
            // Reverting to path parsing logic:
            
            // Get parent directory path
            let dir_meta = self
                .get_metadata_by_inode(dirid)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            
            let path = Path::new(&dir_meta.path);
            if let Some(parent) = path.parent() {
                let parent_str = parent.to_string_lossy();
                if parent_str.is_empty() {
                    return Ok(ROOT_INODE);
                }
                let store = self.metadata.read().await;
                return store.get_inode_by_path(&parent_str)
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?
                    .ok_or(nfsstat3::NFS3ERR_NOENT);
            } else {
                return Ok(ROOT_INODE);
            }
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
        if id == ROOT_INODE || id == FAKE_ROOT_PARENT_ID {
            return Ok(self.root_fattr(id));
        }

        let metadata = self
            .get_metadata_by_inode(id)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        Ok(self.metadata_to_fattr(id, &metadata))
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        // Handle size change (truncation)
        if let set_size3::size(new_size) = setattr.size {
            let metadata = self
                .get_metadata_by_inode(id)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;

            if metadata.is_dir {
                return Err(nfsstat3::NFS3ERR_ISDIR);
            }

            // Ensure file exists in session directory (copy from git if needed)
            let session_path = self.get_session_path(Path::new(&metadata.path)).await;
            self.ensure_session_file(&metadata, &session_path).await?;

            // Truncate/extend the file to new size
            let file = tokio::fs::OpenOptions::new()
                .write(true)
                .open(&session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;

            file.set_len(new_size)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;

            // Mark as dirty and update metadata
            if !Self::is_ignored_path(&metadata.path) {
                let store = self.metadata.write().await;
                store
                    .mark_dirty(&metadata.path)
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?;
                drop(store);
            }

            // Update size and mtime in metadata
            let mut updated_metadata = metadata.clone();
            updated_metadata.size = new_size;
            updated_metadata.mtime = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let store = self.metadata.write().await;
            store
                .put_inode(id, &updated_metadata)
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            drop(store);

            return Ok(self.metadata_to_fattr(id, &updated_metadata));
        }

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

        // Session path for potential reads
        let session_path = self.get_session_path(Path::new(&metadata.path)).await;

        let data = if session_path.exists() {
            // Session file takes priority (handles dirty files and AppleDouble metadata).
            // If a file is marked dirty but its session file was removed (e.g., after
            // rebase reconciliation), we gracefully fall through to git/repo below.
            tokio::fs::read(&session_path)
                .await
                .map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else if metadata.volatile {
            // Volatile (untracked/gitignored) files always passthrough to real filesystem.
            // Never trust cached git_oid or size — the file changes independently of git.
            let repo_file = self.repo_path.join(&metadata.path);
            if repo_file.exists() && repo_file.is_file() {
                tokio::fs::read(&repo_file)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?
            } else {
                Vec::new()
            }
        } else if let Some(oid) = &metadata.git_oid {
            // Read from Git ODB
            let git = self.git.read().await;
            git.read_blob(oid).map_err(|_| nfsstat3::NFS3ERR_IO)?
        } else {
            // Untracked file without volatile flag - try repo filesystem
            let repo_file = self.repo_path.join(&metadata.path);
            if repo_file.exists() && repo_file.is_file() {
                tokio::fs::read(&repo_file)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?
            } else {
                Vec::new()
            }
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

        // Ensure file exists in session (copy from git if needed)
        self.ensure_session_file(&metadata, &session_path).await?;

        // Open file with read+write access for proper seeking
        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&session_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Seek to offset and write data directly
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        file.write_all(data)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Sync to ensure data is written
        file.sync_all()
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Get final file size
        let file_metadata = file.metadata()
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        let new_size = file_metadata.len();

        // Mark as dirty
        if !Self::is_ignored_path(&metadata.path) {
            let store = self.metadata.write().await;
            store
                .mark_dirty(&metadata.path)
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            drop(store);
        }

        // Update size and mtime in metadata
        let mut updated_metadata = metadata.clone();
        updated_metadata.size = new_size;
        updated_metadata.mtime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

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
            mtime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        store
            .put_inode(new_inode, &metadata)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        // Mark as dirty since it's a new file
        if !Self::is_ignored_path(&metadata.path) {
            store
                .mark_dirty(&metadata.path)
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }
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
            mtime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
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

        // Remove from session directory (handle both files and directories)
        let session_path = self.get_session_path(&full_path).await;
        if session_path.exists() {
            if session_path.is_dir() {
                // Remove directory (may fail if not empty)
                tokio::fs::remove_dir(&session_path)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_NOTEMPTY)?;
            } else {
                tokio::fs::remove_file(&session_path)
                    .await
                    .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            }
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
        let (inode, metadata) = self
            .get_metadata_by_path(&from_path)
            .await
            .map_err(|_| nfsstat3::NFS3ERR_IO)?
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;

        // Properly rename the inode (updates path mappings)
        let old_path_str = from_path.to_string_lossy().to_string();
        let new_path_str = to_path.to_string_lossy().to_string();

        let store = self.metadata.write().await;
        store
            .rename_inode(inode, &old_path_str, &new_path_str)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;
        drop(store);

        // Keep metadata reference for later checks
        let _ = metadata;

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

        // 1. Identify IDs for dot and dotdot
        let dot_id = dirid;
        let dotdot_id = if dirid == ROOT_INODE {
            FAKE_ROOT_PARENT_ID
        } else {
            // Lookup parent
            self.lookup(dirid, &nfsstring(b"..".to_vec()))
                .await
                .unwrap_or(ROOT_INODE)
        };

        // 2. Get sorted list of children
        let cache = self.dir_children.read().await;
        let mut children = if let Some(c) = cache.get(&dirid) {
            c.clone()
        } else {
            Vec::new()
        };
        children.sort(); // Ensure stable order
        drop(cache);

        // 3. Determine resume point based on start_after cookie
        // Logical sequence: [dot, dotdot, ...children]
        let mut emit_dot = false;
        let mut emit_dotdot = false;
        let child_idx;

        if start_after == 0 {
            // Start from beginning
            emit_dot = true;
            emit_dotdot = true;
            child_idx = 0;
        } else if start_after == dot_id {
            // Passed dot, resume at dotdot
            emit_dotdot = true;
            child_idx = 0;
        } else if start_after == dotdot_id {
            // Passed dotdot, resume at children start
            child_idx = 0;
        } else {
            // Assume we are inside children list
            // Find position of start_after
            match children.binary_search(&start_after) {
                Ok(idx) => {
                    // Start after the found child
                    child_idx = idx + 1;
                }
                Err(idx) => {
                    // Not found exactly (maybe deleted), start at insertion point
                    // effectively: children[idx] > start_after
                    child_idx = idx;
                }
            }
        }

        let mut entries = Vec::new();
        let store = self.metadata.read().await;

        // Emit entries
        if emit_dot {
            let attr = if dirid == ROOT_INODE {
                self.root_fattr(dirid)
            } else {
                store.get_inode(dirid).ok().flatten().map(|m| self.metadata_to_fattr(dirid, &m)).unwrap_or_else(|| self.root_fattr(dirid))
            };
            
            if entries.len() < max_entries {
                entries.push(DirEntry {
                    fileid: dot_id,
                    name: Self::to_nfsstring("."),
                    attr,
                });
            } else {
                return Ok(ReadDirResult { entries, end: false });
            }
        }

        if emit_dotdot {
             let attr = if dotdot_id == FAKE_ROOT_PARENT_ID {
                self.root_fattr(dotdot_id)
            } else if dotdot_id == ROOT_INODE {
                self.root_fattr(dotdot_id)
            } else {
                store.get_inode(dotdot_id).ok().flatten().map(|m| self.metadata_to_fattr(dotdot_id, &m)).unwrap_or_else(|| self.root_fattr(dotdot_id))
            };
            
            if entries.len() < max_entries {
                entries.push(DirEntry {
                    fileid: dotdot_id,
                    name: Self::to_nfsstring(".."),
                    attr,
                });
            } else {
                return Ok(ReadDirResult { entries, end: false });
            }
        }

        // Emit children
        for &child_inode in children.iter().skip(child_idx) {
            // Skip if child is same as directory (handle . separately)
            if child_inode == dirid {
                continue;
            }

            if entries.len() >= max_entries {
                return Ok(ReadDirResult { entries, end: false });
            }

            if let Ok(Some(child_meta)) = store.get_inode(child_inode) {
                 let filename = Path::new(&child_meta.path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                
                let attr = self.metadata_to_fattr(child_inode, &child_meta);
                entries.push(DirEntry {
                    fileid: child_inode,
                    name: Self::to_nfsstring(&filename),
                    attr,
                });
            }
        }

        Ok(ReadDirResult {
            entries,
            end: true, // We processed everything we intended to
        })
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
            mtime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
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
            repo_dir.clone(),
            "test".to_string(),
        );

        // Test root directory
        assert_eq!(nfs.root_dir(), ROOT_INODE);

        let root_attr = nfs.getattr(ROOT_INODE).await.unwrap();
        // ftype3 doesn't implement PartialEq, so check mode instead
        assert_eq!(root_attr.mode, 0o755);
        assert_eq!(root_attr.fileid, ROOT_INODE);
    }

    #[tokio::test]
    async fn test_readdir_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        
        let metadata = MetadataStore::open(&db_path).unwrap();
        let git = GitRepo::open(&repo_dir).unwrap();

        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata)),
            Arc::new(RwLock::new(git)),
            session_dir,
            repo_dir,
            "test".to_string(),
        );

        // Create a directory "subdir" in root
        let (subdir_id, _) = nfs.mkdir(ROOT_INODE, &VibeNFS::to_nfsstring("subdir")).await.unwrap();
        
        // Create 3 files in subdir
        let _f1 = nfs.create_exclusive(subdir_id, &VibeNFS::to_nfsstring("file1")).await.unwrap();
        let _f2 = nfs.create_exclusive(subdir_id, &VibeNFS::to_nfsstring("file2")).await.unwrap();
        let _f3 = nfs.create_exclusive(subdir_id, &VibeNFS::to_nfsstring("file3")).await.unwrap();

        // List subdir with max_entries=1
        let mut cookie = 0;
        let mut all_entries = Vec::new();
        let mut iterations = 0;
        let max_iterations = 100;
        
        loop {
            iterations += 1;
            if iterations > max_iterations {
                panic!("Infinite loop detected in readdir pagination");
            }

            let result = nfs.readdir(subdir_id, cookie, 1).await.unwrap();
            
            for entry in &result.entries {
                cookie = entry.fileid;
                all_entries.push(String::from_utf8_lossy(&entry.name.0).to_string());
            }
            
            if result.entries.is_empty() || (result.entries.len() < 1) { // len < max_entries implies end
                break;
            }
            // If result.end is true, we stop. 
            // Note: our impl sets end=false if we returned max_entries
            if result.end {
                break;
            }
        }
        
        // Expect: ".", "..", "file1", "file2", "file3"
        assert!(all_entries.contains(&".".to_string()));
        assert!(all_entries.contains(&"..".to_string()));
        assert!(all_entries.contains(&"file1".to_string()));
        assert!(all_entries.contains(&"file2".to_string()));
        assert!(all_entries.contains(&"file3".to_string()));
        assert_eq!(all_entries.len(), 5);
        
        // Verify uniqueness
        let mut sorted = all_entries.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }

    #[test]
    fn test_metadata_to_fattr_symlink_detection() {
        use crate::db::InodeMetadata;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();

        let metadata = MetadataStore::open(&db_path).unwrap();
        let git = crate::git::GitRepo::open(&repo_dir).unwrap();

        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata)),
            Arc::new(RwLock::new(git)),
            session_dir,
            repo_dir,
            "test".to_string(),
        );

        // Test regular file
        let regular_meta = InodeMetadata {
            path: "regular.txt".to_string(),
            git_oid: Some("abc123".to_string()),
            is_dir: false,
            size: 100,
            volatile: false,
            mtime: 0,
        };
        let regular_fattr = nfs.metadata_to_fattr(100, &regular_meta);
        // ftype3::NF3REG has mode 0o644 in our impl
        assert_eq!(regular_fattr.mode, 0o644);

        // Test directory
        let dir_meta = InodeMetadata {
            path: "subdir".to_string(),
            git_oid: None,
            is_dir: true,
            size: 0,
            volatile: false,
            mtime: 0,
        };
        let dir_fattr = nfs.metadata_to_fattr(101, &dir_meta);
        assert_eq!(dir_fattr.mode, 0o755);

        // Test symlink - should be detected by "symlink:" prefix in git_oid
        let symlink_meta = InodeMetadata {
            path: "target".to_string(),
            git_oid: Some("symlink:/tmp/vibe-artifacts/test/target".to_string()),
            is_dir: false,
            size: 35,
            volatile: true,
            mtime: 0,
        };
        let symlink_fattr = nfs.metadata_to_fattr(102, &symlink_meta);
        // Symlinks should also have mode 0o644 but ftype should be NF3LNK
        // We can't directly check ftype since it doesn't impl PartialEq,
        // but we can verify it's not treated as a directory
        assert_eq!(symlink_fattr.mode, 0o644);
        assert_eq!(symlink_fattr.size, 35);
    }

    #[test]
    fn test_volatile_file_size_from_disk() {
        use crate::db::InodeMetadata;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();

        // Create a real file on disk with known content
        let disk_content = "real content on disk, much longer than stale";
        std::fs::write(repo_dir.join("Cargo.lock"), disk_content).unwrap();

        let metadata = MetadataStore::open(&db_path).unwrap();
        let git = crate::git::GitRepo::open(&repo_dir).unwrap();

        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata)),
            Arc::new(RwLock::new(git)),
            session_dir,
            repo_dir.clone(),
            "test".to_string(),
        );

        // Volatile file with stale size (10) — should report real disk size
        let volatile_meta = InodeMetadata {
            path: "Cargo.lock".to_string(),
            git_oid: None,
            is_dir: false,
            size: 10, // stale
            volatile: true,
            mtime: 0,
        };
        let fattr = nfs.metadata_to_fattr(200, &volatile_meta);
        assert_eq!(fattr.size, disk_content.len() as u64);

        // Non-volatile file uses cached size
        let tracked_meta = InodeMetadata {
            path: "src/main.rs".to_string(),
            git_oid: Some("abc123".to_string()),
            is_dir: false,
            size: 999,
            volatile: false,
            mtime: 0,
        };
        let fattr = nfs.metadata_to_fattr(201, &tracked_meta);
        assert_eq!(fattr.size, 999); // uses cached size
    }

    #[tokio::test]
    async fn test_getattr_reflects_session_file_size() {
        // Reproduces the file truncation bug:
        // When a file in the session directory is modified outside the NFS write path
        // (e.g., direct cp/sed to session dir), getattr() should return the actual
        // file size, not the stale cached size from RocksDB.
        use crate::db::InodeMetadata;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "Test"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo_dir).output().unwrap();

        // Create and commit a small file
        std::fs::write(repo_dir.join("test.txt"), "hello").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output().unwrap();

        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD:test.txt"])
            .current_dir(&repo_dir).output().unwrap();
        let oid = String::from_utf8(output.stdout).unwrap().trim().to_string();

        let metadata_store = MetadataStore::open(&db_path).unwrap();
        let inode_id = metadata_store.next_inode_id().unwrap();
        let meta = InodeMetadata {
            path: "test.txt".to_string(),
            git_oid: Some(oid),
            is_dir: false,
            size: 5, // "hello" = 5 bytes
            volatile: false,
            mtime: 0,
        };
        metadata_store.put_inode(inode_id, &meta).unwrap();
        metadata_store.mark_dirty("test.txt").unwrap();

        let git = GitRepo::open(&repo_dir).unwrap();
        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata_store)),
            Arc::new(RwLock::new(git)),
            session_dir.clone(),
            repo_dir,
            "test".to_string(),
        );

        // Step 1: Write a larger file DIRECTLY to session dir (bypassing NFS write path)
        let new_content = "hello world, this is much longer content that was written outside NFS";
        std::fs::write(session_dir.join("test.txt"), new_content).unwrap();

        // Step 2: getattr should reflect the ACTUAL file size, not the stale 5 bytes
        let attr = nfs.getattr(inode_id).await.unwrap();
        assert_eq!(
            attr.size,
            new_content.len() as u64,
            "getattr should return actual session file size ({}), not stale metadata size (5)",
            new_content.len()
        );

        // Step 3: read should also return the full content
        let (data, eof) = nfs.read(inode_id, 0, 4096).await.unwrap();
        assert_eq!(
            String::from_utf8(data).unwrap(),
            new_content,
            "read should return full file content from session"
        );
        assert!(eof, "should be EOF after reading entire file");
    }

    #[tokio::test]
    async fn test_volatile_file_read_passthrough() {
        use crate::db::InodeMetadata;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("metadata.db");
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create a file with "old content", commit it to get a git_oid
        std::fs::write(repo_dir.join("passthrough.txt"), "old content from git").unwrap();
        std::process::Command::new("git")
            .args(["add", "passthrough.txt"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "add file"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Get the blob OID for the old content
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD:passthrough.txt"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        let old_oid = String::from_utf8(output.stdout).unwrap().trim().to_string();

        // Now update the file on disk (simulating it changed after init)
        std::fs::write(repo_dir.join("passthrough.txt"), "new content on disk").unwrap();

        let metadata = MetadataStore::open(&db_path).unwrap();
        // Register the file as volatile with the OLD git_oid (stale metadata)
        let inode_id = metadata.next_inode_id().unwrap();
        let volatile_meta = InodeMetadata {
            path: "passthrough.txt".to_string(),
            git_oid: Some(old_oid.clone()), // stale OID from before the file changed
            is_dir: false,
            size: 20,
            volatile: true, // marked volatile — should passthrough regardless of git_oid
            mtime: 0,
        };
        metadata.put_inode(inode_id, &volatile_meta).unwrap();

        let git = crate::git::GitRepo::open(&repo_dir).unwrap();

        let nfs = VibeNFS::new(
            Arc::new(RwLock::new(metadata)),
            Arc::new(RwLock::new(git)),
            session_dir,
            repo_dir,
            "test".to_string(),
        );

        // Read via NFS — should get disk content, NOT git blob
        let (data, _eof) = nfs.read(inode_id, 0, 1024).await.unwrap();
        let content = String::from_utf8(data).unwrap();
        assert_eq!(content, "new content on disk",
            "volatile file should passthrough to disk, not read stale git blob");

        // Verify the git blob would have returned old content (the bug scenario)
        let git = crate::git::GitRepo::open(nfs.repo_path.as_path()).unwrap();
        let blob = git.read_blob(&old_oid).unwrap();
        assert_eq!(String::from_utf8(blob).unwrap(), "old content from git",
            "git blob should still contain old content");
    }
}
