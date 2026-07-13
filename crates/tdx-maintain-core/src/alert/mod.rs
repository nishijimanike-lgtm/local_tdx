use crate::db::repos::AlertRepo;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AlertEngine {
    pool: SqlitePool,
}

impl AlertEngine {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn info(&self, category: &str, message: &str, detail: Option<&str>) -> anyhow::Result<()> {
        AlertRepo::new(&self.pool)
            .create("info", category, message, detail)
            .await?;
        Ok(())
    }

    pub async fn warn(&self, category: &str, message: &str, detail: Option<&str>) -> anyhow::Result<()> {
        AlertRepo::new(&self.pool)
            .create("warn", category, message, detail)
            .await?;
        Ok(())
    }

    pub async fn error(&self, category: &str, message: &str, detail: Option<&str>) -> anyhow::Result<()> {
        AlertRepo::new(&self.pool)
            .create("error", category, message, detail)
            .await?;
        Ok(())
    }

    pub async fn tier_changed(&self, old: &str, new: &str) -> anyhow::Result<()> {
        self.warn(
            "tier_change",
            &format!("复权因子降级等级变化: {old} -> {new}"),
            None,
        )
        .await
    }
}
