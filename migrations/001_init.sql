-- ============================================================
-- 交易日历表
-- ============================================================
CREATE TABLE IF NOT EXISTS trade_calendar (
    exchange     TEXT NOT NULL,
    trade_date   TEXT NOT NULL,
    is_open      INTEGER NOT NULL,
    source       TEXT NOT NULL DEFAULT 'tushare',
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (exchange, trade_date)
);

-- ============================================================
-- 除权除息原始事件表
-- ============================================================
CREATE TABLE IF NOT EXISTS xdxr_events (
    market       INTEGER NOT NULL,
    symbol       TEXT NOT NULL,
    ex_date      TEXT NOT NULL,
    category     INTEGER NOT NULL,
    fenhong      REAL DEFAULT 0,
    peigu        REAL DEFAULT 0,
    peigujia     REAL DEFAULT 0,
    songzhuangu  REAL DEFAULT 0,
    source       TEXT NOT NULL DEFAULT 'tdxrs',
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (market, symbol, ex_date, category)
);

-- ============================================================
-- 复权因子表
-- ============================================================
CREATE TABLE IF NOT EXISTS adj_factor (
    market       INTEGER NOT NULL,
    symbol       TEXT NOT NULL,
    trade_date   TEXT NOT NULL,
    adj_factor   REAL NOT NULL,
    data_source  TEXT NOT NULL,
    confidence   TEXT NOT NULL DEFAULT 'normal',
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (market, symbol, trade_date)
);

CREATE INDEX IF NOT EXISTS idx_adj_factor_symbol ON adj_factor (market, symbol);
CREATE INDEX IF NOT EXISTS idx_adj_factor_date ON adj_factor (trade_date);

-- ============================================================
-- 因子交叉校验记录表
-- ============================================================
CREATE TABLE IF NOT EXISTS factor_validation (
    market        INTEGER NOT NULL,
    symbol        TEXT NOT NULL,
    trade_date    TEXT NOT NULL,
    tushare_value REAL,
    local_value   REAL,
    diff_pct      REAL,
    status        TEXT NOT NULL,
    checked_at    TEXT NOT NULL,
    PRIMARY KEY (market, symbol, trade_date)
);

-- ============================================================
-- 系统元数据 / 降级状态表
-- ============================================================
CREATE TABLE IF NOT EXISTS sync_meta (
    key          TEXT PRIMARY KEY,
    value        TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

-- ============================================================
-- 任务执行日志
-- ============================================================
CREATE TABLE IF NOT EXISTS task_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type     TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    status        TEXT NOT NULL,
    done_count    INTEGER DEFAULT 0,
    skipped_count INTEGER DEFAULT 0,
    failed_count  INTEGER DEFAULT 0,
    detail        TEXT
);

-- ============================================================
-- 告警表
-- ============================================================
CREATE TABLE IF NOT EXISTS alerts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    level       TEXT NOT NULL,
    category    TEXT NOT NULL,
    message     TEXT NOT NULL,
    detail      TEXT,
    acknowledged INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

-- ============================================================
-- 扫描结果缓存表
-- ============================================================
CREATE TABLE IF NOT EXISTS scan_results (
    id          TEXT PRIMARY KEY,
    scan_type   TEXT NOT NULL,
    status      TEXT NOT NULL,
    result_json TEXT,
    created_at  TEXT NOT NULL,
    finished_at TEXT
);

-- 预置 sync_meta 键值
INSERT OR IGNORE INTO sync_meta (key, value, updated_at) VALUES
    ('adj_factor_tier', 'L3', datetime('now')),
    ('last_probe_at', '', datetime('now')),
    ('last_daily_update', '', datetime('now')),
    ('last_adj_factor_update', '', datetime('now')),
    ('calendar_source', 'index_derived', datetime('now'));
