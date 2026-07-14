-- ============================================================
-- 移除已迁移到 Parquet 存储的 adj_factor 及相关表
-- ============================================================

-- factor_validation 表从未写入数据（仅 Parquet 存储）
DROP TABLE IF EXISTS factor_validation;

-- adj_factor 表数据已迁移到 {parquet_dir}/{market}/{symbol}.parquet
DROP TABLE IF EXISTS adj_factor;
