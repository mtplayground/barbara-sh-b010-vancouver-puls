use anyhow::{Context, Result};
use s3::{creds::Credentials, Bucket, Region};

use crate::config::ObjectStorageConfig;

#[derive(Debug, Clone)]
pub struct ObjectStorage {
    bucket_client: Box<Bucket>,
    bucket: String,
    prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub bucket: String,
    pub key: String,
    pub content_length: i64,
}

impl ObjectStorage {
    pub async fn from_config(config: &ObjectStorageConfig) -> Result<Self> {
        let credentials = Credentials::new(
            Some(&config.access_key_id),
            Some(&config.secret_access_key),
            None,
            None,
            None,
        )
        .context("failed to build object storage credentials")?;
        let region = Region::Custom {
            region: config.region.clone(),
            endpoint: config.endpoint.clone(),
        };
        let bucket_client = Bucket::new(&config.bucket, region, credentials)
            .context("failed to build object storage bucket client")?
            .with_path_style();

        Ok(Self {
            bucket_client,
            bucket: config.bucket.clone(),
            prefix: normalize_prefix(&config.prefix),
        })
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn object_key(&self, key: &str) -> Result<String> {
        let normalized_key = normalize_object_key(key)?;

        if self.prefix.is_empty() {
            return Ok(normalized_key);
        }

        Ok(format!("{}/{}", self.prefix, normalized_key))
    }

    pub async fn put_bytes(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<StoredObject> {
        let storage_key = self.object_key(key)?;
        let content_length = i64::try_from(bytes.len()).context("object is too large to upload")?;
        let response = self
            .bucket_client
            .put_object_with_content_type(
                &storage_key,
                &bytes,
                content_type.unwrap_or("application/octet-stream"),
            )
            .await
            .with_context(|| format!("failed to upload object `{storage_key}`"))?;

        if !is_success_status(response.status_code()) {
            anyhow::bail!(
                "object storage upload failed for `{}` with status {}",
                storage_key,
                response.status_code()
            );
        }

        Ok(StoredObject {
            bucket: self.bucket.clone(),
            key: storage_key,
            content_length,
        })
    }

    pub async fn get_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let storage_key = self.object_key(key)?;
        let response = self
            .bucket_client
            .get_object(&storage_key)
            .await
            .with_context(|| format!("failed to fetch object `{storage_key}`"))?;

        if !is_success_status(response.status_code()) {
            anyhow::bail!(
                "object storage fetch failed for `{}` with status {}",
                storage_key,
                response.status_code()
            );
        }

        Ok(response.bytes().to_vec())
    }

    pub async fn delete_object(&self, key: &str) -> Result<()> {
        let storage_key = self.object_key(key)?;
        let response = self
            .bucket_client
            .delete_object(&storage_key)
            .await
            .with_context(|| format!("failed to delete object `{storage_key}`"))?;

        if !is_success_status(response.status_code()) {
            anyhow::bail!(
                "object storage delete failed for `{}` with status {}",
                storage_key,
                response.status_code()
            );
        }

        Ok(())
    }

    pub async fn check_bucket(&self) -> Result<()> {
        let (_, status_code) = self
            .bucket_client
            .list_page(self.prefix.clone(), None, None, None, Some(1))
            .await
            .with_context(|| format!("failed to access object storage bucket `{}`", self.bucket))?;

        if !is_success_status(status_code) {
            anyhow::bail!(
                "object storage bucket check failed for `{}` with status {}",
                self.bucket,
                status_code
            );
        }

        Ok(())
    }
}

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim_matches('/').to_owned()
}

fn is_success_status(status_code: u16) -> bool {
    (200..300).contains(&status_code)
}

fn normalize_object_key(key: &str) -> Result<String> {
    let normalized = key.trim_matches('/');

    if normalized.is_empty() {
        anyhow::bail!("object key must not be empty");
    }

    if normalized.split('/').any(|part| part == "..") {
        anyhow::bail!("object key must not contain parent directory segments");
    }

    Ok(normalized.to_owned())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{normalize_object_key, ObjectStorage};

    #[test]
    fn object_key_prepends_configured_prefix() -> Result<()> {
        let storage = test_storage("rendered/assets")?;

        assert_eq!(
            storage.object_key("/posts/reel.mp4")?,
            "rendered/assets/posts/reel.mp4"
        );

        Ok(())
    }

    #[test]
    fn object_key_allows_empty_prefix() -> Result<()> {
        let storage = test_storage("")?;

        assert_eq!(storage.object_key("source/image.jpg")?, "source/image.jpg");

        Ok(())
    }

    #[test]
    fn object_key_rejects_parent_segments() {
        assert!(normalize_object_key("cached/../secret").is_err());
    }

    fn test_storage(prefix: &str) -> Result<ObjectStorage> {
        let credentials = s3::creds::Credentials::anonymous()?;
        let region = s3::Region::Custom {
            region: "auto".to_owned(),
            endpoint: "https://object-storage.example.com".to_owned(),
        };
        let bucket_client = s3::Bucket::new("bucket", region, credentials)?.with_path_style();

        Ok(ObjectStorage {
            bucket_client,
            bucket: "bucket".to_owned(),
            prefix: prefix.to_owned(),
        })
    }
}
