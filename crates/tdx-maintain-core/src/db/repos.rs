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

pub struct AdjFactorRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AdjFactorRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, row: &AdjFactorRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO adj_factor (market, symbol, trade_date, adj_factor, data_source, confidence, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(market, symbol, trade_date) DO UPDATE SET
               adj_factor=excluded.adj_factor, data_source=excluded.data_source,
               confidence=excluded.confidence, updated_at=excluded.updated_at",
        )
        .bind(row.market)
        .bind(&row.symbol)
        .bind(&row.trade_date)
        .bind(row.adj_factor)
        .bind(&row.data_source)
        .bind(&row.confidence)
        .bind(&row.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_batch(&self, rows: &[AdjFactorRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        
        // SQLite parameter limit is 999. With 7 columns, maximum batch size is 999 / 7 = 142.
        // We use a safe chunk size of 100.
        for chunk in rows.chunks(100) {
            let mut sql = String::from(
                "INSERT INTO adj_factor (market, symbol, trade_date, adj_factor, data_source, confidence, updated_at) VALUES "
            );
            
            for i in 0..chunk.len() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str("(?, ?, ?, ?, ?, ?, ?)");
            }
            
            sql.push_str(
                " ON CONFLICT(market, symbol, trade_date) DO UPDATE SET \
                 adj_factor=excluded.adj_factor, data_source=excluded.data_source, \
                 confidence=excluded.confidence, updated_at=excluded.updated_at"
            );
            
            let mut query = sqlx::query(&sql);
            for row in chunk {
                query = query
                    .bind(row.market)
                    .bind(&row.symbol)
                    .bind(&row.trade_date)
                    .bind(row.adj_factor)
                    .bind(&row.data_source)
                    .bind(&row.confidence)
                    .bind(&row.updated_at);
            }
            
            query.execute(&mut *tx).await?;
        }
        
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(
        &self,
        market: i32,
        symbol: &str,
        trade_date: &str,
    ) -> anyhow::Result<Option<AdjFactorRow>> {
        Ok(sqlx::query_as(
            "SELECT market, symbol, trade_date, adj_factor, data_source, confidence, updated_at
             FROM adj_factor WHERE market = ? AND symbol = ? AND trade_date = ?",
        )
        .bind(market)
        .bind(symbol)
        .bind(trade_date)
        .fetch_optional(self.pool)
        .await?)
    }

    pub async fn latest_date(
        &self,
        market: i32,
        symbol: &str,
    ) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT trade_date FROM adj_factor WHERE market = ? AND symbol = ? ORDER BY trade_date DESC LIMIT 1",
        )
        .bind(market)
        .bind(symbol)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn count_symbols(&self) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT market || ':' || symbol) FROM adj_factor",
        )
        .fetch_one(self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn list_validation(&self, limit: i64) -> anyhow::Result<Vec<FactorValidationRow>> {
        Ok(sqlx::query_as(
            "SELECT market, symbol, trade_date, tushare_value, local_value, diff_pct, status, checked_at
             FROM factor_validation ORDER BY checked_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn upsert_validation(&self, row: &FactorValidationRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO factor_validation (market, symbol, trade_date, tushare_value, local_value, diff_pct, status, checked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(market, symbol, trade_date) DO UPDATE SET
               tushare_value=excluded.tushare_value, local_value=excluded.local_value,
               diff_pct=excluded.diff_pct, status=excluded.status, checked_at=excluded.checked_at",
        )
        .bind(row.market)
        .bind(&row.symbol)
        .bind(&row.trade_date)
        .bind(row.tushare_value)
        .bind(row.local_value)
        .bind(row.diff_pct)
        .bind(&row.status)
        .bind(&row.checked_at)
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
