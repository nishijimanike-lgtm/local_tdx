use super::models::*;
use sqlx::SqlitePool;

pub struct SyncMetaRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SyncMetaRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM sync_meta WHERE key = ?")
                .bind(key)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO sync_meta (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all(&self) -> anyhow::Result<Vec<SyncMetaRow>> {
        Ok(sqlx::query_as("SELECT key, value, updated_at FROM sync_meta ORDER BY key")
            .fetch_all(self.pool)
            .await?)
    }
}

pub struct TaskLogRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> TaskLogRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, task_type: &str) -> anyhow::Result<i64> {
        let now = now_iso();
        let result = sqlx::query(
            "INSERT INTO task_log (task_type, started_at, status) VALUES (?, ?, 'running')",
        )
        .bind(task_type)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn finish(
        &self,
        id: i64,
        status: &str,
        done: i32,
        skipped: i32,
        failed: i32,
        detail: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "UPDATE task_log SET finished_at = ?, status = ?, done_count = ?, skipped_count = ?, failed_count = ?, detail = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(status)
        .bind(done)
        .bind(skipped)
        .bind(failed)
        .bind(detail)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent(&self, limit: i64) -> anyhow::Result<Vec<TaskLogRow>> {
        Ok(sqlx::query_as(
            "SELECT id, task_type, started_at, finished_at, status, done_count, skipped_count, failed_count, detail
             FROM task_log ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn get(&self, id: i64) -> anyhow::Result<Option<TaskLogRow>> {
        Ok(sqlx::query_as(
            "SELECT id, task_type, started_at, finished_at, status, done_count, skipped_count, failed_count, detail
             FROM task_log WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?)
    }

    /// Delete all task_log rows. If `keep_active_id` is provided, that single
    /// row (the currently running task) is preserved so its in-flight state
    /// is not orphaned; everything else — including stale "running" rows left
    /// behind by past crashes/restarts — is removed.
    pub async fn clear_all_except(&self, keep_active_id: Option<i64>) -> anyhow::Result<()> {
        match keep_active_id {
            Some(id) => {
                sqlx::query("DELETE FROM task_log WHERE id != ?")
                    .bind(id)
                    .execute(self.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM task_log").execute(self.pool).await?;
            }
        }
        Ok(())
    }
}

pub struct AlertRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AlertRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        level: &str,
        category: &str,
        message: &str,
        detail: Option<&str>,
    ) -> anyhow::Result<i64> {
        let now = now_iso();
        let result = sqlx::query(
            "INSERT INTO alerts (level, category, message, detail, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(level)
        .bind(category)
        .bind(message)
        .bind(detail)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn list(&self, limit: i64) -> anyhow::Result<Vec<AlertRow>> {
        Ok(sqlx::query_as(
            "SELECT id, level, category, message, detail, acknowledged, created_at FROM alerts ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn acknowledge(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE alerts SET acknowledged = 1 WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

pub struct CalendarRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CalendarRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_batch(&self, rows: &[TradeCalendarRow]) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for row in rows {
            sqlx::query(
                "INSERT INTO trade_calendar (exchange, trade_date, is_open, source, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(exchange, trade_date) DO UPDATE SET
                   is_open = excluded.is_open, source = excluded.source, updated_at = excluded.updated_at",
            )
            .bind(&row.exchange)
            .bind(&row.trade_date)
            .bind(row.is_open)
            .bind(&row.source)
            .bind(&row.updated_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_trading_days(
        &self,
        exchange: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT trade_date FROM trade_calendar
             WHERE exchange = ? AND is_open = 1 AND trade_date >= ? AND trade_date <= ?
             ORDER BY trade_date",
        )
        .bind(exchange)
        .bind(start)
        .bind(end)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn latest_trading_day(&self, exchange: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT trade_date FROM trade_calendar WHERE exchange = ? AND is_open = 1 ORDER BY trade_date DESC LIMIT 1",
        )
        .bind(exchange)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn is_trading_day(&self, exchange: &str, date: &str) -> anyhow::Result<bool> {
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT is_open FROM trade_calendar WHERE exchange = ? AND trade_date = ?",
        )
        .bind(exchange)
        .bind(date)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.0 == 1).unwrap_or(false))
    }

    pub async fn count_open_days(&self, exchange: &str) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM trade_calendar WHERE exchange = ? AND is_open = 1",
        )
        .bind(exchange)
        .fetch_one(self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn list(
        &self,
        exchange: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<TradeCalendarRow>> {
        Ok(sqlx::query_as(
            "SELECT exchange, trade_date, is_open, source, updated_at FROM trade_calendar
             WHERE exchange = ? AND trade_date >= ? AND trade_date <= ? ORDER BY trade_date",
        )
        .bind(exchange)
        .bind(start)
        .bind(end)
        .fetch_all(self.pool)
        .await?)
    }
}

pub struct XdxrRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> XdxrRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, row: &XdxrEventRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO xdxr_events (market, symbol, ex_date, category, fenhong, peigu, peigujia, songzhuangu, source, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(market, symbol, ex_date, category) DO UPDATE SET
               fenhong=excluded.fenhong, peigu=excluded.peigu, peigujia=excluded.peigujia,
               songzhuangu=excluded.songzhuangu, updated_at=excluded.updated_at",
        )
        .bind(row.market)
        .bind(&row.symbol)
        .bind(&row.ex_date)
        .bind(row.category)
        .bind(row.fenhong)
        .bind(row.peigu)
        .bind(row.peigujia)
        .bind(row.songzhuangu)
        .bind(&row.source)
        .bind(&row.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_for_symbol(
        &self,
        market: i32,
        symbol: &str,
    ) -> anyhow::Result<Vec<XdxrEventRow>> {
        Ok(sqlx::query_as(
            "SELECT market, symbol, ex_date, category, fenhong, peigu, peigujia, songzhuangu, source, updated_at
             FROM xdxr_events WHERE market = ? AND symbol = ? ORDER BY ex_date",
        )
        .bind(market)
        .bind(symbol)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn count(&self) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM xdxr_events")
            .fetch_one(self.pool)
            .await?;
        Ok(row.0)
    }
}


pub struct DownloadCheckpointRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> DownloadCheckpointRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn save(&self, change_name: &str, market: &str, last_symbol: &str, progress: i32, total: i32) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO download_checkpoint (change_name, market, last_symbol, progress, total, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(change_name, market) DO UPDATE SET
               last_symbol = excluded.last_symbol, progress = excluded.progress,
               total = excluded.total, updated_at = excluded.updated_at",
        )
        .bind(change_name)
        .bind(market)
        .bind(last_symbol)
        .bind(progress)
        .bind(total)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn load(&self, change_name: &str, market: &str) -> anyhow::Result<Option<DownloadCheckpointRow>> {
        Ok(sqlx::query_as(
            "SELECT change_name, market, last_symbol, progress, total, updated_at
             FROM download_checkpoint WHERE change_name = ? AND market = ?",
        )
        .bind(change_name)
        .bind(market)
        .fetch_optional(self.pool)
        .await?)
    }

    pub async fn clear(&self, change_name: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM download_checkpoint WHERE change_name = ?")
            .bind(change_name)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}



pub struct ScanRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ScanRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, id: &str, scan_type: &str) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO scan_results (id, scan_type, status, created_at) VALUES (?, ?, 'running', ?)",
        )
        .bind(id)
        .bind(scan_type)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish(
        &self,
        id: &str,
        status: &str,
        result_json: &str,
    ) -> anyhow::Result<()> {
        let now = now_iso();
        sqlx::query(
            "UPDATE scan_results SET status = ?, result_json = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(result_json)
        .bind(&now)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Option<ScanResultRow>> {
        Ok(sqlx::query_as(
            "SELECT id, scan_type, status, result_json, created_at, finished_at FROM scan_results WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let schema = "
            CREATE TABLE IF NOT EXISTS sync_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS task_log (id INTEGER PRIMARY KEY AUTOINCREMENT, task_type TEXT NOT NULL, started_at TEXT NOT NULL, finished_at TEXT, status TEXT NOT NULL, done_count INTEGER DEFAULT 0, skipped_count INTEGER DEFAULT 0, failed_count INTEGER DEFAULT 0, detail TEXT);
            CREATE TABLE IF NOT EXISTS trade_calendar (exchange TEXT NOT NULL, trade_date TEXT NOT NULL, is_open INTEGER NOT NULL, source TEXT NOT NULL DEFAULT 'tushare', updated_at TEXT NOT NULL, PRIMARY KEY (exchange, trade_date));
            CREATE TABLE IF NOT EXISTS xdxr_events (market INTEGER NOT NULL, symbol TEXT NOT NULL, ex_date TEXT NOT NULL, category INTEGER NOT NULL, fenhong REAL DEFAULT 0, peigu REAL DEFAULT 0, peigujia REAL DEFAULT 0, songzhuangu REAL DEFAULT 0, source TEXT NOT NULL DEFAULT 'tdxrs', updated_at TEXT NOT NULL, PRIMARY KEY (market, symbol, ex_date, category));
            CREATE TABLE IF NOT EXISTS download_checkpoint (change_name TEXT NOT NULL, market TEXT NOT NULL, last_symbol TEXT NOT NULL, progress INTEGER NOT NULL DEFAULT 0, total INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL, PRIMARY KEY (change_name, market));
        ";
        sqlx::query(schema).execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_sync_meta_set_and_get() {
        let pool = setup().await;
        let repo = SyncMetaRepo::new(&pool);
        repo.set("foo", "bar").await.unwrap();
        assert_eq!(repo.get("foo").await.unwrap(), Some("bar".to_string()));
        assert_eq!(repo.get("nonexistent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_sync_meta_overwrite() {
        let pool = setup().await;
        let repo = SyncMetaRepo::new(&pool);
        repo.set("key", "v1").await.unwrap();
        repo.set("key", "v2").await.unwrap();
        assert_eq!(repo.get("key").await.unwrap(), Some("v2".to_string()));
    }

    #[tokio::test]
    async fn test_task_log_create_and_finish() {
        let pool = setup().await;
        let repo = TaskLogRepo::new(&pool);
        let id = repo.create("daily_bar_update").await.unwrap();
        assert!(id > 0);
        repo.finish(id, "success", 10, 2, 1, Some("detail")).await.unwrap();
        let list = repo.list_recent(10).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, "success");
        assert_eq!(list[0].done_count, 10);
    }

    #[tokio::test]
    async fn test_calendar_upsert_and_query() {
        let pool = setup().await;
        let repo = CalendarRepo::new(&pool);
        let rows = vec![
            TradeCalendarRow { exchange: "SSE".into(), trade_date: "2024-01-02".into(), is_open: 1, source: "test".into(), updated_at: "now".into() },
            TradeCalendarRow { exchange: "SSE".into(), trade_date: "2024-01-03".into(), is_open: 1, source: "test".into(), updated_at: "now".into() },
            TradeCalendarRow { exchange: "SSE".into(), trade_date: "2024-01-06".into(), is_open: 0, source: "test".into(), updated_at: "now".into() },
        ];
        repo.upsert_batch(&rows).await.unwrap();
        let days = repo.get_trading_days("SSE", "2024-01-01", "2024-12-31").await.unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0], "2024-01-02");
        let latest = repo.latest_trading_day("SSE").await.unwrap();
        assert_eq!(latest, Some("2024-01-03".to_string()));
        assert!(repo.is_trading_day("SSE", "2024-01-02").await.unwrap());
        assert!(!repo.is_trading_day("SSE", "2024-01-06").await.unwrap());
        assert_eq!(repo.count_open_days("SSE").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_xdxr_upsert_and_count() {
        let pool = setup().await;
        let repo = XdxrRepo::new(&pool);
        let row = XdxrEventRow {
            market: 1, symbol: "600000".into(), ex_date: "2024-06-15".into(),
            category: 1, fenhong: 0.5, peigu: 0.0, peigujia: 0.0,
            songzhuangu: 0.0, source: "local".into(), updated_at: "now".into(),
        };
        repo.upsert(&row).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 1);
        let list = repo.list_for_symbol(1, "600000").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fenhong, 0.5);
    }

    #[tokio::test]
    async fn test_download_checkpoint_save_load_clear() {
        let pool = setup().await;
        let repo = DownloadCheckpointRepo::new(&pool);
        repo.save("daily-full", "sh", "600000", 50, 200).await.unwrap();
        let cp = repo.load("daily-full", "sh").await.unwrap().unwrap();
        assert_eq!(cp.last_symbol, "600000");
        repo.save("daily-full", "sh", "600100", 100, 200).await.unwrap();
        let cp2 = repo.load("daily-full", "sh").await.unwrap().unwrap();
        assert_eq!(cp2.progress, 100);
        repo.clear("daily-full").await.unwrap();
        assert!(repo.load("daily-full", "sh").await.unwrap().is_none());
    }
}
