//! Pluggable logo storage. The default `db` backend keeps logo bytes in
//! Postgres so they survive a redeploy; `disk` is the legacy local-file backend
//! (does not survive redeploys on most platforms); `s3` is wired as an
//! extension point but not yet implemented. The backend is chosen once at boot
//! from `STORAGE_BACKEND` and held in `AppState`.

use anyhow::{bail, Context};
use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub enum LogoStore {
    /// Bytes live in the `tenant_logos` table, served from `/logo/:tenant_id`.
    Db,
    /// Legacy: bytes written to a local directory, served from `/uploads`.
    Disk { dir: String },
    // S3 { bucket, endpoint, … } — future: add an arm here and in `store_logo`.
}

impl LogoStore {
    /// Build the configured backend, failing fast on an unknown or not-yet
    /// implemented choice so a misconfigured deploy is caught at boot.
    pub fn from_config(cfg: &crate::config::Config) -> anyhow::Result<Self> {
        match cfg.storage_backend.as_str() {
            "db" => Ok(LogoStore::Db),
            "disk" => Ok(LogoStore::Disk {
                dir: cfg.uploads_dir.clone(),
            }),
            "s3" => bail!(
                "STORAGE_BACKEND=s3 is not implemented yet: add a LogoStore::S3 arm \
                 (object-storage upload) or use STORAGE_BACKEND=db"
            ),
            other => bail!("unknown STORAGE_BACKEND `{other}` (expected: db, disk, s3)"),
        }
    }

    /// Persist a tenant's logo and return the URL to store in
    /// `tenants.logo_url`. `ext`/`content_type` come from the validated upload.
    pub async fn store_logo(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        tenant_slug: &str,
        ext: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<String> {
        // Content-hash for cache-busting: the URL changes whenever the image
        // does, so clients (and email caches) never serve a stale logo.
        let digest = sha2::Sha256::digest(bytes);
        let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();

        match self {
            LogoStore::Db => {
                sqlx::query(
                    "INSERT INTO tenant_logos (tenant_id, bytes, content_type, updated_at) \
                     VALUES ($1, $2, $3, NOW()) \
                     ON CONFLICT (tenant_id) DO UPDATE \
                     SET bytes = EXCLUDED.bytes, \
                         content_type = EXCLUDED.content_type, \
                         updated_at = NOW()",
                )
                .bind(tenant_id)
                .bind(bytes)
                .bind(content_type)
                .execute(pool)
                .await
                .context("storing tenant logo in database")?;
                Ok(format!("/logo/{tenant_id}?v={short}"))
            }
            LogoStore::Disk { dir } => {
                let file_name = format!("logo-{tenant_slug}-{short}.{ext}");
                let path = std::path::Path::new(dir).join(&file_name);
                tokio::fs::write(&path, bytes)
                    .await
                    .with_context(|| format!("writing logo to {}", path.display()))?;
                Ok(format!("/uploads/{file_name}"))
            }
        }
    }

    /// Drop a tenant's stored logo. Only the db backend deletes anything; the
    /// disk backend leaves the (content-hashed) files in place, as before.
    pub async fn delete_logo(&self, pool: &PgPool, tenant_id: Uuid) -> anyhow::Result<()> {
        if let LogoStore::Db = self {
            sqlx::query("DELETE FROM tenant_logos WHERE tenant_id = $1")
                .bind(tenant_id)
                .execute(pool)
                .await
                .context("deleting tenant logo")?;
        }
        Ok(())
    }
}
