use std::path::PathBuf;
use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use futures::StreamExt;

use super::{ByteStream, StorageBackend, StorageError};

/// Local filesystem storage backend.
/// All objects are written under `base_dir` using the supplied key as a relative path.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }

    fn full_path(&self, key: &str) -> PathBuf {
        // Prevent path traversal: strip any leading / and collapse ..
        let safe: PathBuf = key
            .split('/')
            .filter(|s| !s.is_empty() && *s != "..")
            .collect();
        self.base_dir.join(safe)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<(), StorageError> {
        let path = self.full_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(&path).await?;
        file.write_all(&data).await?;
        file.flush().await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<ByteStream, StorageError> {
        let path = self.full_path(key);
        if !path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }
        let file = tokio::fs::File::open(&path).await?;
        let stream = ReaderStream::new(file)
            .map(|r| r.map_err(StorageError::Io));
        Ok(Box::pin(stream))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.full_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.full_path(key).exists())
    }

    async fn size(&self, key: &str) -> Result<u64, StorageError> {
        let path = self.full_path(key);
        let meta = tokio::fs::metadata(&path).await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        Ok(meta.len())
    }

    async fn list(&self, prefix: &str, delimiter: &str) -> Result<super::StorageListResult, StorageError> {
        let path = self.full_path(prefix);
        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();

        if path.is_dir() {
            let mut dirs_to_visit = vec![(path.clone(), prefix.trim_end_matches('/').to_string())];
            while let Some((curr_dir, curr_prefix)) = dirs_to_visit.pop() {
                let mut entries = tokio::fs::read_dir(&curr_dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let file_type = entry.file_type().await?;
                    let file_name = entry.file_name().to_string_lossy().to_string();

                    // Construct the virtual key (relative path from root)
                    let relative_key = if curr_prefix.is_empty() {
                        file_name.clone()
                    } else {
                        format!("{}/{}", curr_prefix, file_name)
                    };

                    if file_type.is_dir() {
                        if delimiter.is_empty() {
                            dirs_to_visit.push((entry.path(), relative_key));
                        } else {
                            common_prefixes.push(format!("{}/", relative_key));
                        }
                    } else {
                        let metadata = entry.metadata().await?;
                        let size = metadata.len();
                        let last_modified = metadata.modified().ok().map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.to_rfc3339()
                        });
                        objects.push(super::StorageObject {
                            key: relative_key,
                            size,
                            last_modified,
                        });
                    }
                }
            }
        } else if path.is_file() {
            let metadata = tokio::fs::metadata(&path).await?;
            let size = metadata.len();
            let last_modified = metadata.modified().ok().map(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                datetime.to_rfc3339()
            });
            objects.push(super::StorageObject {
                key: prefix.to_string(),
                size,
                last_modified,
            });
        }

        objects.sort_by(|a, b| a.key.cmp(&b.key));
        common_prefixes.sort();

        Ok(super::StorageListResult {
            objects,
            common_prefixes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_storage_list_delimited_and_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());

        storage.put("root.txt", Bytes::from("root")).await.unwrap();
        storage.put("folder/a.txt", Bytes::from("a")).await.unwrap();
        storage.put("folder/sub/b.txt", Bytes::from("b")).await.unwrap();

        // Delimited list at root: should see root.txt in objects and folder/ in common_prefixes
        let res_delimited = storage.list("", "/").await.unwrap();
        assert_eq!(res_delimited.objects.len(), 1);
        assert_eq!(res_delimited.objects[0].key, "root.txt");
        assert_eq!(res_delimited.common_prefixes, vec!["folder/"]);

        // Recursive list at root: should see all 3 files and 0 common_prefixes
        let res_recursive = storage.list("", "").await.unwrap();
        let keys: Vec<_> = res_recursive.objects.into_iter().map(|o| o.key).collect();
        assert_eq!(keys, vec!["folder/a.txt", "folder/sub/b.txt", "root.txt"]);
        assert!(res_recursive.common_prefixes.is_empty());

        // Recursive list under "folder/": should see both files under folder
        let res_folder = storage.list("folder/", "").await.unwrap();
        let folder_keys: Vec<_> = res_folder.objects.into_iter().map(|o| o.key).collect();
        assert_eq!(folder_keys, vec!["folder/a.txt", "folder/sub/b.txt"]);
    }
}
