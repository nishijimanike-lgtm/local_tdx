-- ============================================================
-- 股票/指数基本信息表
-- ============================================================
CREATE TABLE IF NOT EXISTS stocks (
    market           INTEGER NOT NULL,        -- 0=sz, 1=sh, 2=bj
    symbol           TEXT NOT NULL,           -- 股票/指数代码 (如 '600000', '000001')
    name             TEXT NOT NULL,           -- 股票/指数名称
    pinyin_initials  TEXT NOT NULL,           -- 拼音首字母 (如 'pfyh')
    PRIMARY KEY (market, symbol)
);

CREATE INDEX IF NOT EXISTS idx_stocks_symbol ON stocks (symbol);
CREATE INDEX IF NOT EXISTS idx_stocks_name ON stocks (name);
CREATE INDEX IF NOT EXISTS idx_stocks_pinyin ON stocks (pinyin_initials);
