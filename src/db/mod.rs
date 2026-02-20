use anyhow::{Context, Result};
use rocksdb::{DB, Options};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Metadata about a file or directory in the virtual filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InodeMetadata {
    pub path: String,
    pub git_oid: Option<String>,
    pub is_dir: bool,
    pub size: u64,
    pub volatile: bool, // For untracked files like .env, node_modules
}

/// Bi-directional inode-to-Git mapping store
pub struct MetadataStore {
    db: DB,
}

impl MetadataStore {
    /// Open or create a metadata store at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, path)
            .context("Failed to open RocksDB")?;

        Ok(Self { db })
    }

    /// Open metadata store in read-only mode
    pub fn open_readonly<P: AsRef<Path>>(path: P) -> Result<Self> {
        let opts = Options::default();
        let db = DB::open_for_read_only(&opts, path, false)
            .context("Failed to open RocksDB in read-only mode")?;

        Ok(Self { db })
    }

    /// Store inode metadata with both forward and reverse mappings
    pub fn put_inode(&self, inode_id: u64, metadata: &InodeMetadata) -> Result<()> {
        let key = format!("inode:{}", inode_id);
        let value = serde_json::to_vec(metadata)?;
        self.db.put(key.as_bytes(), value)?;

        // Reverse mapping: path -> inode_id
        let path_key = format!("path:{}", metadata.path);
        let inode_bytes = inode_id.to_le_bytes();
        self.db.put(path_key.as_bytes(), inode_bytes)?;

        Ok(())
    }

    /// Get metadata by inode ID
    pub fn get_inode(&self, inode_id: u64) -> Result<Option<InodeMetadata>> {
        let key = format!("inode:{}", inode_id);
        let value = self.db.get(key.as_bytes())?;

        match value {
            Some(bytes) => {
                let metadata = serde_json::from_slice(&bytes)?;
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    /// Get inode ID by path
    pub fn get_inode_by_path(&self, path: &str) -> Result<Option<u64>> {
        let key = format!("path:{}", path);
        let value = self.db.get(key.as_bytes())?;

        match value {
            Some(bytes) => {
                let inode_id = u64::from_le_bytes(bytes.try_into().unwrap());
                Ok(Some(inode_id))
            }
            None => Ok(None),
        }
    }

    /// Delete inode and its reverse mapping
    pub fn delete_inode(&self, inode_id: u64) -> Result<()> {
        // First get the metadata to find the path
        if let Some(metadata) = self.get_inode(inode_id)? {
            let path_key = format!("path:{}", metadata.path);
            self.db.delete(path_key.as_bytes())?;
        }

        let key = format!("inode:{}", inode_id);
        self.db.delete(key.as_bytes())?;

        Ok(())
    }

    /// Rename an inode (update path mappings properly)
    pub fn rename_inode(&self, inode_id: u64, old_path: &str, new_path: &str) -> Result<()> {
        // Get current metadata
        let mut metadata = self.get_inode(inode_id)?
            .ok_or_else(|| anyhow::anyhow!("Inode {} not found", inode_id))?;

        // Delete old path mapping
        let old_path_key = format!("path:{}", old_path);
        self.db.delete(old_path_key.as_bytes())?;

        // Update metadata with new path
        metadata.path = new_path.to_string();

        // Store updated metadata (this also creates the new path mapping)
        self.put_inode(inode_id, &metadata)?;

        // If the file was dirty under the old path, update the dirty tracking
        let old_dirty_key = format!("dirty:{}", old_path);
        if self.db.get(old_dirty_key.as_bytes())?.is_some() {
            self.db.delete(old_dirty_key.as_bytes())?;
            let new_dirty_key = format!("dirty:{}", new_path);
            self.db.put(new_dirty_key.as_bytes(), b"1")?;
        }

        Ok(())
    }

    /// Get next available inode ID
    pub fn next_inode_id(&self) -> Result<u64> {
        let key = b"counter:inode";
        let value = self.db.get(key)?;

        let next_id = match value {
            Some(bytes) => {
                let current = u64::from_le_bytes(bytes.try_into().unwrap());
                current + 1
            }
            None => 100, // Start from 100 to avoid collisions with reserved IDs
        };

        self.db.put(key, next_id.to_le_bytes())?;
        Ok(next_id)
    }

    /// Mark a path as dirty (modified in session)
    pub fn mark_dirty(&self, path: &str) -> Result<()> {
        let key = format!("dirty:{}", path);
        self.db.put(key.as_bytes(), b"1")?;
        Ok(())
    }

    /// Check if a path is dirty
    pub fn is_dirty(&self, path: &str) -> Result<bool> {
        let key = format!("dirty:{}", path);
        Ok(self.db.get(key.as_bytes())?.is_some())
    }

    /// Get all dirty paths
    pub fn get_dirty_paths(&self) -> Result<Vec<String>> {
        let prefix = b"dirty:";
        let mut paths = Vec::new();

        let iter = self.db.prefix_iterator(prefix);
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(path) = key_str.strip_prefix("dirty:") {
                paths.push(path.to_string());
            }
        }

        Ok(paths)
    }

    /// Get all inodes
    pub fn get_all_inodes(&self) -> Result<Vec<(u64, InodeMetadata)>> {
        let prefix = b"inode:";
        let mut inodes = Vec::new();

        let iter = self.db.prefix_iterator(prefix);
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id_str) = key_str.strip_prefix("inode:") {
                if let Ok(id) = id_str.parse::<u64>() {
                    let metadata: InodeMetadata = serde_json::from_slice(&value)?;
                    inodes.push((id, metadata));
                }
            }
        }

        Ok(inodes)
    }

    /// Clone this metadata store to a new location.
    /// Used to create per-session copies from the base template.
    /// If the destination already exists, opens it directly (idempotent re-export).
    pub fn clone_to<P: AsRef<Path>>(&self, dest_path: P) -> Result<Self> {
        let dest_path = dest_path.as_ref();

        // If destination already exists, just open it (idempotent)
        if dest_path.exists() {
            return Self::open(dest_path);
        }

        let dest = Self::open(dest_path)?;

        // Copy all inodes
        let all_inodes = self.get_all_inodes()?;
        for (inode_id, metadata) in &all_inodes {
            dest.put_inode(*inode_id, metadata)?;
        }

        // Copy the inode counter
        let counter_key = b"counter:inode";
        if let Some(counter_val) = self.db.get(counter_key)? {
            dest.db.put(counter_key, counter_val)?;
        }

        Ok(dest)
    }

    /// Clear all dirty marks
    pub fn clear_dirty(&self) -> Result<()> {
        let prefix = b"dirty:";
        let keys: Vec<_> = self.db.prefix_iterator(prefix)
            .filter_map(|item| item.ok())
            .map(|(key, _)| key)
            .collect();

        for key in keys {
            self.db.delete(&key)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_metadata_store_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let store = MetadataStore::open(temp_dir.path().join("test.db")).unwrap();

        let metadata = InodeMetadata {
            path: "src/main.rs".to_string(),
            git_oid: Some("abc123".to_string()),
            is_dir: false,
            size: 1024,
            volatile: false,
        };

        let inode_id = store.next_inode_id().unwrap();
        store.put_inode(inode_id, &metadata).unwrap();

        let retrieved = store.get_inode(inode_id).unwrap().unwrap();
        assert_eq!(retrieved.path, metadata.path);
        assert_eq!(retrieved.git_oid, metadata.git_oid);
        assert_eq!(retrieved.size, metadata.size);
    }

    #[test]
    fn test_path_to_inode_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let store = MetadataStore::open(temp_dir.path().join("test.db")).unwrap();

        let metadata = InodeMetadata {
            path: "src/lib.rs".to_string(),
            git_oid: None,
            is_dir: false,
            size: 512,
            volatile: false,
        };

        let inode_id = store.next_inode_id().unwrap();
        store.put_inode(inode_id, &metadata).unwrap();

        let found_id = store.get_inode_by_path("src/lib.rs").unwrap().unwrap();
        assert_eq!(found_id, inode_id);
    }

    #[test]
    fn test_dirty_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let store = MetadataStore::open(temp_dir.path().join("test.db")).unwrap();

        assert!(!store.is_dirty("test.txt").unwrap());

        store.mark_dirty("test.txt").unwrap();
        assert!(store.is_dirty("test.txt").unwrap());

        let dirty_paths = store.get_dirty_paths().unwrap();
        assert_eq!(dirty_paths.len(), 1);
        assert_eq!(dirty_paths[0], "test.txt");

        store.clear_dirty().unwrap();
        assert!(!store.is_dirty("test.txt").unwrap());
    }
}
