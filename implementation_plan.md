# Parquet Storage Implementation Plan for TDX Adjustment Factors

This plan outlines the changes to migrate/augment the storage of cumulative forward adjustment factors from the local SQLite database to compressed Parquet files using the `arrow` and `parquet` crates.

## User Review Required

> [!IMPORTANT]
> - We will add `arrow = "53.0.0"` and `parquet = { version = "53.0.0", features = ["arrow", "zstd"] }` to the workspace dependencies.
> - A new path parameter `parquet_dir` will be added to the settings under `[paths]`. The default directory will be `D:\tdx_maintain\parquet`.
> - During the `adj_factor_update` sync task, the system will save the computed factors to `{parquet_dir}/{market}/{symbol}.parquet` (with Zstd compression) in addition to updating the SQLite database.
> - This parallel-write approach allows existing Web UI and DB queries to function perfectly while generating the optimized Parquet dataset for future Polars integration.

## Open Questions

> [!NOTE]
> - Do you want the system to write *only* to Parquet files and skip writing to the SQLite database entirely for `adj_factor`?
> - *Recommendation*: Currently, we write to both SQLite and Parquet. Writing to SQLite is fast now (due to the `upsert_batch` optimization and WAL mode), and it keeps the dashboard stats working out-of-the-box. We can disable SQLite writes for `adj_factor` once Polars query integration is completed in a future phase.

## Proposed Changes

### Dependencies

#### [MODIFY] [Cargo.toml](file:///d:/gp/local_tdx/Cargo.toml)
- Add workspace dependencies:
  - `arrow = "53.0.0"`
  - `parquet = { version = "53.0.0", features = ["arrow", "zstd"] }`

#### [MODIFY] [Cargo.toml](file:///d:/gp/local_tdx/crates/tdx-maintain-core/Cargo.toml)
- Import workspace dependencies:
  - `arrow.workspace = true`
  - `parquet.workspace = true`

---

### Component: Config

#### [MODIFY] [mod.rs (config)](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/config/mod.rs)
- Add `parquet_dir: String` to `PathsConfig`.

#### [MODIFY] [default.toml](file:///d:/gp/local_tdx/config/default.toml)
- Add `parquet_dir = 'D:\tdx_maintain\parquet'` to the `[paths]` section.

---

### Component: Server & UI Settings

#### [MODIFY] [main.rs](file:///d:/gp/local_tdx/crates/tdx-maintain-server/src/main.rs)
- Add `parquet_dir: String` to the `PathsConfigData` struct.

#### [MODIFY] [index.html](file:///d:/gp/local_tdx/crates/tdx-maintain-server/src/index.html)
- Add input field for `parquet_dir` in the Settings tab under "本地环境路径".
- Add directory info card/display for `parquet_dir` in the Dashboard tab under "服务器路径与参数".

---

### Component: Adjustment Factor Writer

#### [MODIFY] [mod.rs (adj_factor)](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/adj_factor/mod.rs)
- Implement `write_parquet_file(path: &Path, rows: &[AdjFactorRow]) -> anyhow::Result<()>` helper using `ArrowWriter` and `RecordBatch`.
- In `sync()`, after successful SQLite batch upsert (or in parallel), create target directory `{parquet_dir}/{market}/` and write the data as `{symbol}.parquet` with Zstd compression.

---

## Verification Plan

### Automated Tests
- Prepend MinGW path to environment and run `cargo check --workspace` to ensure compiling succeeds with new dependencies.

### Manual Verification
- Recompile and restart the backend server.
- Run `adj-factor-sync` task from the UI dashboard or REST API.
- Verify that a folder `D:\tdx_maintain\parquet` is created and contains subfolders `sh` and `sz` with `.parquet` files.
