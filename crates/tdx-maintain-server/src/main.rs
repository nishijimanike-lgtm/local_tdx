use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::{Html, IntoResponse},
    routing::{get, post, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_stream::StreamExt as _;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tdx_maintain_core::{
    checker::DataFreshnessChecker,
    db::repos::*,
    qlib::QlibDumper,
    task::TaskKind,
    tdx::{DailyBarReader, Market, get_day_filename},
    AppConfig, AppState,
};

#[derive(Debug, Deserialize)]
struct CalendarQuery {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    server: ServerConfigData,
    paths: PathsConfigData,
    calendar: CalendarConfigData,
    tushare: TushareConfigData,
    rate_limit: RateLimitConfigData,
    adj_factor: AdjFactorConfigData,
    alerts: AlertsConfigData,
    schedule: ScheduleConfigData,
    retry: RetryConfigData,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfigData {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
struct PathsConfigData {
    tdx_data_dir: String,
    metadata_db_path: String,
    backup_dir: String,
    parquet_dir: String,
    qlib_dir: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CalendarConfigData {
    benchmark_index_market: i32,
    benchmark_index_symbol: String,
    exchange: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TushareConfigData {
    enabled: bool,
    token: String,
    base_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RateLimitConfigData {
    market_hours_rps: u32,
    pre_post_market_rps: u32,
    off_hours_rps: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct AdjFactorConfigData {
    conflict_threshold_pct: f64,
    default_tier: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlertsConfigData {
    daily_completeness_threshold_pct: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ScheduleConfigData {
    daily_increment_cron: String,
    xdxr_sync_cron: String,
    adj_factor_sync_cron: String,
    calendar_check_cron: String,
    weekly_scan_cron: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RetryConfigData {
    max_attempts: u32,
    backoff_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting TDX Data Maintenance Server...");

    // Load configuration
    let config = AppConfig::load()?;
    let host = config.server.host.clone();
    let port = config.server.port;

    // Initialize AppState (handles DB pool initialization & migrations)
    let state = AppState::new(config).await?;

    // Start background cron scheduler
    start_scheduler(state.clone()).await?;

    // Build Axum router
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .nest_service("/assets", ServeDir::new("crates/tdx-web/dist/assets"))
        .route("/api/health", get(health_check))
        .route("/api/dashboard", get(get_dashboard))
        .route("/api/parquet/stats", get(get_parquet_stats))
        .route("/api/calendar", get(get_calendar).post(update_calendar))
        .route("/api/scan/results/{id}", get(get_scan_results))
        .route("/api/scan/{type}", post(run_scan))
        .route("/api/tasks", get(list_tasks).delete(clear_task_history))
        .route("/api/tasks/control/pause", post(pause_task))
        .route("/api/tasks/control/resume", post(resume_task))
        .route("/api/tasks/control/abort", post(abort_task))
        .route("/api/tasks/{action}", post(trigger_task))
        .route("/api/tasks/progress", get(subscribe_progress))
        .route("/api/settings", get(get_settings).put(update_settings))
        .route("/api/alerts", get(list_alerts))
        .route("/api/alerts/{id}/acknowledge", patch(acknowledge_alert))
        .route("/api/stocks/search", get(search_stocks))
        .route("/api/stock/kline", get(get_stock_kline))
        .route("/api/checker/freshness", get(check_freshness))
        .route("/api/qlib/dump", post(trigger_qlib_dump))
        .route("/api/qlib/progress", get(get_qlib_progress))
        .fallback(spa_fallback)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind and serve
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn start_scheduler(state: AppState) -> anyhow::Result<()> {
    let sched = JobScheduler::new().await?;
    let config = state.config.clone();
    let task_queue = state.task_queue.clone();

    // 1. Daily Increment Update
    let tq = task_queue.clone();
    let job_daily = Job::new_async(config.schedule.daily_increment_cron.as_str(), move |_uuid, _l| {
        let tq = tq.clone();
        Box::pin(async move {
            info!("Cron: Triggering Daily K-Line Update");
            let _ = tq.enqueue(TaskKind::DailyIncrement).await;
        })
    })?;
    sched.add(job_daily).await?;

    // 2. XDXR Sync
    let tq = task_queue.clone();
    let job_xdxr = Job::new_async(config.schedule.xdxr_sync_cron.as_str(), move |_uuid, _l| {
        let tq = tq.clone();
        Box::pin(async move {
            info!("Cron: Triggering XDXR Events Sync");
            let _ = tq.enqueue(TaskKind::XdxrSync).await;
        })
    })?;
    sched.add(job_xdxr).await?;

    // 3. Adj Factor Sync
    let tq = task_queue.clone();
    let job_adj = Job::new_async(config.schedule.adj_factor_sync_cron.as_str(), move |_uuid, _l| {
        let tq = tq.clone();
        Box::pin(async move {
            info!("Cron: Triggering Adjustment Factors Sync");
            let _ = tq.enqueue(TaskKind::AdjFactorSync).await;
        })
    })?;
    sched.add(job_adj).await?;

    // 4. Calendar Check
    let tq = task_queue.clone();
    let job_cal = Job::new_async(config.schedule.calendar_check_cron.as_str(), move |_uuid, _l| {
        let tq = tq.clone();
        Box::pin(async move {
            info!("Cron: Triggering Calendar Update");
            let _ = tq.enqueue(TaskKind::CalendarSync).await;
        })
    })?;
    sched.add(job_cal).await?;

    // 5. Weekly Scan
    let tq = task_queue.clone();
    let job_scan = Job::new_async(config.schedule.weekly_scan_cron.as_str(), move |_uuid, _l| {
        let tq = tq.clone();
        Box::pin(async move {
            info!("Cron: Triggering Weekly Integrity Scan");
            let _ = tq.enqueue(TaskKind::DailyBarScan).await;
        })
    })?;
    sched.add(job_scan).await?;

    tokio::spawn(async move {
        if let Err(e) = sched.start().await {
            tracing::error!("Scheduler error: {e}");
        }
    });

    Ok(())
}

// Handler: Serve Frontend Dashboard
// index.html references content-hashed assets (/assets/index-<hash>.js|css).
// Those hashes change on every frontend rebuild, so the HTML document must
// never be cached heuristically by the browser — otherwise a stale HTML
// referencing now-404 old hashes renders a blank page. Hashed assets under
// /assets are still safely cacheable (ServeDir serves them immutably).
async fn serve_dashboard() -> impl IntoResponse {
    (
        [(axum::http::header::CACHE_CONTROL, "no-cache, must-revalidate")],
        Html(include_str!("../../tdx-web/dist/index.html")),
    )
}

// Handler: SPA Fallback
// vue-router uses HTML5 history mode, so deep links like /settings or /download
// must be served the app shell (index.html) so client-side routing can take
// over on refresh/direct-access. Unknown /api/* paths return a real 404 (not
// HTML) so frontend fetch errors stay diagnosable.
async fn spa_fallback(uri: axum::http::Uri) -> axum::response::Response {
    if uri.path().starts_with("/api") {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    (
        [(axum::http::header::CACHE_CONTROL, "no-cache, must-revalidate")],
        Html(include_str!("../../tdx-web/dist/index.html")),
    )
        .into_response()
}

// Handler: Health Check
async fn health_check() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

// Handler: Parquet Storage Statistics (runs blocking I/O in spawn_blocking)
async fn get_parquet_stats(State(state): State<AppState>) -> impl IntoResponse {
    let parquet_dir = state.config.paths.parquet_dir.clone();
    let result = tokio::task::spawn_blocking(move || {
        use std::path::Path;
        let parquet_dir_path = Path::new(&parquet_dir);

        if !parquet_dir_path.exists() {
            return json!({
                "exists": false,
                "parquet_dir": parquet_dir,
                "markets": {},
                "total_files": 0,
                "total_size_mb": 0.0
            });
        }

        let mut markets: serde_json::Map<String, Value> = serde_json::Map::new();
        let mut total_files: u64 = 0;
        let mut total_size: u64 = 0;

        let entries = match std::fs::read_dir(parquet_dir_path) {
            Ok(entries) => entries,
            Err(_) => {
                return json!({
                    "exists": true,
                    "parquet_dir": parquet_dir,
                    "markets": {},
                    "total_files": 0,
                    "total_size_mb": 0.0,
                    "error": "无法读取目录"
                });
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let market_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let mut file_count: u64 = 0;
                let mut dir_size: u64 = 0;

                if let Ok(dir_entries) = std::fs::read_dir(&path) {
                    for f in dir_entries.flatten() {
                        let fp = f.path();
                        if fp.extension().map_or(false, |e| e == "parquet") {
                            file_count += 1;
                            if let Ok(meta) = fp.metadata() {
                                dir_size += meta.len();
                            }
                        }
                    }
                }

                total_files += file_count;
                total_size += dir_size;

                markets.insert(market_name, json!({
                    "files": file_count,
                    "size_mb": format!("{:.2}", dir_size as f64 / 1_048_576.0)
                }));
            }
        }

        json!({
            "exists": true,
            "parquet_dir": parquet_dir,
            "markets": markets,
            "total_files": total_files,
            "total_size_mb": format!("{:.2}", total_size as f64 / 1_048_576.0)
        })
    })
    .await
    .unwrap_or_else(|_| json!({
        "exists": false,
        "parquet_dir": state.config.paths.parquet_dir,
        "markets": {},
        "total_files": 0,
        "total_size_mb": 0.0,
        "error": "spawn_blocking 执行失败"
    }));

    Json(result)
}

/// Pick the more recent of two RFC3339 timestamp strings.
/// Returns the non-empty one if the other is empty.
fn pick_more_recent(a: &Option<String>, b: &str) -> String {
    match (a, b) {
        (Some(a_val), b_val) if !b_val.is_empty() => {
            // Compare by parsing both as DateTime
            match (
                chrono::DateTime::parse_from_rfc3339(a_val),
                chrono::DateTime::parse_from_rfc3339(b_val),
            ) {
                (Ok(ta), Ok(tb)) => {
                    if ta > tb { a_val.clone() } else { b_val.to_string() }
                }
                _ => a_val.clone(), // fallback to file mtime if parsing fails
            }
        }
        (Some(a_val), _) => a_val.clone(),
        (None, b_val) => b_val.to_string(),
    }
}

// Handler: Dashboard Stats
async fn get_dashboard(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let meta_repo = SyncMetaRepo::new(&state.pool);
    let calendar_repo = CalendarRepo::new(&state.pool);
    let xdxr_repo = XdxrRepo::new(&state.pool);

    let adj_factor_tier = meta_repo.get("adj_factor_tier").await.unwrap_or(None).unwrap_or_else(|| "L3".to_string());
    let last_probe_at = meta_repo.get("last_probe_at").await.unwrap_or(None).unwrap_or_default();
    let last_daily_update = meta_repo.get("last_daily_update").await.unwrap_or(None).unwrap_or_default();
    let last_adj_factor_update = meta_repo.get("last_adj_factor_update").await.unwrap_or(None).unwrap_or_default();
    let calendar_source = meta_repo.get("calendar_source").await.unwrap_or(None).unwrap_or_else(|| "index_derived".to_string());

    let open_days_count = calendar_repo.count_open_days(&state.config.calendar.exchange).await.unwrap_or(0);
    let xdxr_events_count = xdxr_repo.count().await.unwrap_or(0);
    let adj_factor_symbols_count = {
        let parquet_dir = state.config.paths.parquet_dir.clone();
        tokio::task::spawn_blocking(move || {
            let path = std::path::Path::new(&parquet_dir);
            if !path.exists() { return 0i64; }
            let mut count = 0i64;
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Ok(files) = std::fs::read_dir(entry.path()) {
                            count += files.flatten()
                                .filter(|f| f.path().extension().map_or(false, |e| e == "parquet"))
                                .count() as i64;
                        }
                    }
                }
            }
            count
        }).await.unwrap_or(0)
    };

    // Compute daily bar date range from benchmark index files (SH 000001 and SZ 399001)
    // Also capture file modification time as a more reliable "last update" indicator
    // than sync_meta (which is only written when tasks complete via our task queue).
    let tdx_data_dir = state.config.paths.tdx_data_dir.clone();
    let parquet_dir_path = state.config.paths.parquet_dir.clone();
    let daily_bar_range = tokio::task::spawn_blocking(move || {
        let base_dir = {
            let p = std::path::Path::new(&tdx_data_dir);
            if p.ends_with("vipdoc") { p.to_path_buf() } else { p.join("vipdoc") }
        };

        // Define the two benchmark indices: SH 000001 and SZ 399001
        let index_candidates = [
            (Market::Sh, "000001"),
            (Market::Sz, "399001"),
        ];

        let reader = DailyBarReader::default();
        let mut first_date: Option<String> = None;
        let mut last_date: Option<String> = None;
        let mut latest_file_mtime: Option<std::time::SystemTime> = None;

        for (market, code) in &index_candidates {
            let filename = get_day_filename(*market, code, &base_dir);
            let path = base_dir.join(market.dir_name()).join("lday").join(&filename);

            // Capture file modification time
            if let Ok(meta) = std::fs::metadata(&path) {
                if let Ok(mtime) = meta.modified() {
                    latest_file_mtime = Some(match latest_file_mtime {
                        Some(existing) if existing > mtime => existing,
                        _ => mtime,
                    });
                }
            }

            if let Ok(bars) = reader.read_file(&path) {
                if let Some(first) = bars.first() {
                    let d = first.date.format("%Y-%m-%d").to_string();
                    first_date = Some(match &first_date {
                        None => d,
                        Some(existing) => if d < *existing { d } else { existing.clone() },
                    });
                }
                if let Some(last) = bars.last() {
                    let d = last.date.format("%Y-%m-%d").to_string();
                    last_date = Some(match &last_date {
                        None => d,
                        Some(existing) => if d > *existing { d } else { existing.clone() },
                    });
                }
            }
        }

        // Also check latest parquet file mtime for adj_factor freshness
        let mut latest_parquet_mtime: Option<std::time::SystemTime> = None;
        let pq_path = std::path::Path::new(&parquet_dir_path);
        if pq_path.exists() {
            if let Ok(market_dirs) = std::fs::read_dir(pq_path) {
                for mdir in market_dirs.flatten() {
                    if mdir.path().is_dir() {
                        if let Ok(files) = std::fs::read_dir(mdir.path()) {
                            for f in files.flatten() {
                                if f.path().extension().map_or(false, |e| e == "parquet") {
                                    if let Ok(meta) = f.metadata() {
                                        if let Ok(mtime) = meta.modified() {
                                            latest_parquet_mtime = Some(match latest_parquet_mtime {
                                                Some(existing) if existing > mtime => existing,
                                                _ => mtime,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (first_date, last_date, latest_file_mtime, latest_parquet_mtime)
    }).await.unwrap_or((None, None, None, None));

    // Use file mtime as primary source for "last update" timestamps.
    // Fall back to sync_meta if file mtime is unavailable.
    // Use the MORE RECENT of the two (file mtime vs sync_meta).
    let file_daily_ts = daily_bar_range.2
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    let file_adj_ts = daily_bar_range.3
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());

    let last_daily_update = pick_more_recent(&file_daily_ts, &last_daily_update);
    let last_adj_factor_update = pick_more_recent(&file_adj_ts, &last_adj_factor_update);

    Ok(Json(json!({
        "adj_factor_tier": adj_factor_tier,
        "last_probe_at": last_probe_at,
        "last_daily_update": last_daily_update,
        "last_adj_factor_update": last_adj_factor_update,
        "calendar_source": calendar_source,
        "counts": {
            "open_days": open_days_count,
            "xdxr_events": xdxr_events_count,
            "adj_factor_symbols": adj_factor_symbols_count
        },
        "daily_bar_range": {
            "start": daily_bar_range.0,
            "end": daily_bar_range.1
        }
    })))
}


// Handler: Get Calendar
async fn get_calendar(
    State(state): State<AppState>,
    Query(query): Query<CalendarQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = CalendarRepo::new(&state.pool);
    let start = query.start.unwrap_or_else(|| "1990-01-01".to_string());
    let end = query.end.unwrap_or_else(|| "2099-12-31".to_string());

    let list = repo.list(&state.config.calendar.exchange, &start, &end)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(list))
}

// Handler: Trigger Calendar Build
async fn update_calendar(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let task_id = state.task_queue.enqueue(TaskKind::CalendarSync)
        .await
        .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    Ok(Json(json!({ "task_id": task_id })))
}

// Handler: Get Scan Results
async fn get_scan_results(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = ScanRepo::new(&state.pool);
    let result = repo.get(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match result {
        Some(row) => {
            let val: Value = row.result_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            Ok(Json(json!({
                "id": row.id,
                "scan_type": row.scan_type,
                "status": row.status,
                "created_at": row.created_at,
                "finished_at": row.finished_at,
                "results": val
            })))
        }
        None => Err((StatusCode::NOT_FOUND, "Scan result not found".to_string())),
    }
}

// Handler: Trigger Scan
async fn run_scan(
    State(state): State<AppState>,
    Path(scan_type): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let kind = match scan_type.as_str() {
        "daily_bars" => TaskKind::DailyBarScan,
        "xdxr" => TaskKind::XdxrScan,
        "adj_factors" => TaskKind::AdjFactorScan,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid scan type".to_string())),
    };

    let task_id = state.task_queue.enqueue(kind)
        .await
        .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    Ok(Json(json!({ "task_id": task_id })))
}

// Handler: List Task Logs
async fn list_tasks(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = TaskLogRepo::new(&state.pool);
    let list = repo.list_recent(50)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(list))
}

// Handler: Clear Task History
// Removes all task_log rows except the currently running task (if any), so
// in-flight progress is preserved while stale/crashed "running" rows and
// finished history are wiped.
async fn clear_task_history(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let active_id = {
        let guard = state.task_queue.active_task.lock().await;
        guard.as_ref().map(|t| t.task_id)
    };
    TaskLogRepo::new(&state.pool)
        .clear_all_except(active_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "status": "ok" })))
}

// Handler: Trigger Task
async fn trigger_task(
    State(state): State<AppState>,
    Path(action): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let kind = match action.as_str() {
        "calendar-sync" => TaskKind::CalendarSync,
        "daily-full" => TaskKind::DailyFull,
        "daily-increment" => TaskKind::DailyIncrement,
        "daily-gap-fill" => TaskKind::DailyGapFill,
        "xdxr-sync" => TaskKind::XdxrSync,
        "adj-factor-sync" => TaskKind::AdjFactorSync,
        "daily_bars" => TaskKind::DailyBarScan,
        "local-import" => TaskKind::LocalImport,
        "xdxr" => TaskKind::XdxrScan,
        "adj_factors" => TaskKind::AdjFactorScan,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid action".to_string())),
    };

    let task_id = state.task_queue.enqueue(kind)
        .await
        .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    Ok(Json(json!({ "task_id": task_id })))
}

// Handler: Pause Running Task
async fn pause_task(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.task_queue.pause().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "status": "ok", "message": "Task paused" })))
}

// Handler: Resume Running Task
async fn resume_task(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.task_queue.resume().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "status": "ok", "message": "Task resumed" })))
}

// Handler: Abort Running Task
async fn abort_task(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.task_queue.abort().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "status": "ok", "message": "Task aborted" })))
}

// Handler: Subscribe Task Progress (SSE)
async fn subscribe_progress(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.task_queue.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|res| {
            match res {
                Ok(progress) => {
                    let val = serde_json::to_value(&progress).unwrap_or(Value::Null);
                    let event = Event::default().json_data(&val).ok()?;
                    Some(Ok::<Event, std::convert::Infallible>(event))
                }
                Err(_) => None,
            }
        });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Handler: Get Settings
async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    Json((*state.config).clone())
}

// Handler: Update Settings
async fn update_settings(
    State(_state): State<AppState>,
    Json(payload): Json<UpdateSettingsRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let toml_str = toml::to_string_pretty(&json!({
        "server": payload.server,
        "paths": payload.paths,
        "calendar": payload.calendar,
        "tushare": payload.tushare,
        "rate_limit": payload.rate_limit,
        "adj_factor": payload.adj_factor,
        "alerts": payload.alerts,
        "schedule": payload.schedule,
        "retry": payload.retry,
    })).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to generate TOML: {e}")))?;

    let config_path = std::env::var("TDX_MAINTAIN_CONFIG")
        .unwrap_or_else(|_| "config/default.toml".to_string());

    std::fs::write(&config_path, toml_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write config file: {e}")))?;

    info!("Configuration file updated at {}", config_path);

    Ok(Json(json!({ "status": "ok", "message": "Settings updated. Please restart server for some changes to take effect." })))
}

// Handler: List Alerts
async fn list_alerts(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = AlertRepo::new(&state.pool);
    let list = repo.list(50)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(list))
}

// Handler: Acknowledge Alert
async fn acknowledge_alert(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = AlertRepo::new(&state.pool);
    repo.acknowledge(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Debug, Deserialize)]
struct SearchStocksParams {
    q: String,
}

async fn search_stocks(
    State(state): State<AppState>,
    Query(params): Query<SearchStocksParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo = StockRepo::new(&state.pool);

    // Auto-sync if stocks table is empty
    let is_empty = repo.is_empty().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to check stocks: {e}")))?;
    if is_empty {
        info!("Stocks database table is empty. Auto-syncing stock list from TDX...");
        sync_stocks_impl(&state).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to sync stocks: {e}")))?;
    }

    let results = repo.search(&params.q).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to search stocks: {e}")))?;

    let json_res: Vec<serde_json::Value> = results.into_iter().map(|(market, symbol, name)| {
        let market_str = match market {
            0 => "sz",
            1 => "sh",
            2 => "bj",
            _ => "unknown",
        };
        json!({
            "market": market_str,
            "symbol": symbol,
            "name": name,
        })
    }).collect();

    Ok(Json(json_res))
}

#[derive(Debug, Deserialize)]
struct GetStockKlineParams {
    market: String,
    symbol: String,
    adjust: Option<String>,
}

async fn get_stock_kline(
    State(state): State<AppState>,
    Query(params): Query<GetStockKlineParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let market_str = params.market.to_lowercase();
    let market = Market::from_dir(&market_str)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("Invalid market: {}", params.market)))?;
    let symbol = params.symbol;
    let adjust = params.adjust.unwrap_or_else(|| "none".to_string());

    // 1. Resolve path to lday file
    let tdx: std::path::PathBuf = state.config.paths.tdx_data_dir.clone().into();
    let base_dir = if tdx.ends_with("vipdoc") {
        tdx
    } else {
        tdx.join("vipdoc")
    };
    let filename = get_day_filename(market, &symbol, &base_dir);
    let lday_path = base_dir.join(market.dir_name()).join("lday").join(&filename);

    if !lday_path.exists() {
        return Err((StatusCode::NOT_FOUND, format!("Local day file not found: {}#{}", market_str, symbol)));
    }

    // 2. Read raw bars
    let reader = DailyBarReader::default();
    let bars = reader.read_file_async(&lday_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read daily bars: {e}")))?;

    // Check if target is an Index (typically sh000xxx or sz399xxx)
    let is_index = (market_str == "sh" && symbol.starts_with("00"))
        || (market_str == "sz" && symbol.starts_with("39"));

    if adjust == "none" || is_index {
        let json_res: Vec<serde_json::Value> = bars.into_iter().map(|bar| {
            json!({
                "date": bar.date.format("%Y-%m-%d").to_string(),
                "open": bar.open,
                "high": bar.high,
                "low": bar.low,
                "close": bar.close,
                "volume": bar.volume,
                "amount": bar.amount,
            })
        }).collect();
        return Ok(Json(json_res));
    }

    // 3. For adjustment: load factors from parquet
    let parquet_base = std::path::Path::new(&state.config.paths.parquet_dir);
    let parquet_path = parquet_base.join(&market_str).join(format!("{}.parquet", symbol));

    let mut factors = Vec::new();
    if parquet_path.exists() {
        factors = tokio::task::spawn_blocking(move || {
            tdx_maintain_core::adj_factor::read_parquet_file(&parquet_path)
        }).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Spawn blocking failed: {e}")))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read parquet factors: {e}")))?;
    }

    if factors.is_empty() {
        let json_res: Vec<serde_json::Value> = bars.into_iter().map(|bar| {
            json!({
                "date": bar.date.format("%Y-%m-%d").to_string(),
                "open": bar.open,
                "high": bar.high,
                "low": bar.low,
                "close": bar.close,
                "volume": bar.volume,
                "amount": bar.amount,
            })
        }).collect();
        return Ok(Json(json_res));
    }

    // Sort factors by date to ensure correctness
    factors.sort_by(|a, b| a.trade_date.cmp(&b.trade_date));
    let latest_factor = factors.last().map(|r| r.adj_factor).unwrap_or(1.0);

    let json_res: Vec<serde_json::Value> = bars.into_iter().map(|bar| {
        let bar_date_str = bar.date.format("%Y-%m-%d").to_string();
        
        let factor = match factors.binary_search_by(|r| r.trade_date.cmp(&bar_date_str)) {
            Ok(idx) => factors[idx].adj_factor,
            Err(idx) => {
                if idx > 0 {
                    factors[idx - 1].adj_factor
                } else {
                    1.0
                }
            }
        };

        let (open, high, low, close, volume) = if adjust == "forward" {
            let ratio = factor / latest_factor;
            (
                bar.open * ratio,
                bar.high * ratio,
                bar.low * ratio,
                bar.close * ratio,
                bar.volume as f64 / ratio,
            )
        } else if adjust == "backward" {
            let ratio = factor; // relative to start
            (
                bar.open * ratio,
                bar.high * ratio,
                bar.low * ratio,
                bar.close * ratio,
                bar.volume as f64 / ratio,
            )
        } else {
            (bar.open, bar.high, bar.low, bar.close, bar.volume as f64)
        };

        json!({
            "date": bar_date_str,
            "open": open,
            "high": high,
            "low": low,
            "close": close,
            "volume": volume,
            "amount": bar.amount,
        })
    }).collect();

    Ok(Json(json_res))
}

/// Handler: Check data freshness vs TDX server.
///
/// Probes each market's remote TDX server to discover the latest trading date,
/// then compares against every local `.day` file to produce a per-market
/// and per-stock report showing what's up to date vs behind.
async fn check_freshness(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let checker = DataFreshnessChecker::new(state.config.clone(), state.pool.clone());
    let report = checker
        .check()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Freshness check failed: {e}")))?;

    Ok(Json(
        serde_json::to_value(&report)
            .unwrap_or_else(|_| json!({"error": "serialization failed"})),
    ))
}

/// Handler: Trigger Qlib Binary Dump
///
/// Spawns the dump in a background task and returns immediately.
/// The frontend polls `GET /api/qlib/progress` for real-time progress.
async fn trigger_qlib_dump(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let qlib_dir = std::path::PathBuf::from(&state.config.paths.qlib_dir);
    let pool = state.pool.clone();
    let config = state.config.clone();
    let progress_state = state.qlib_progress.clone();

    // Check if already running & mark as started
    if !progress_state.start(0) {
        return Err((
            StatusCode::CONFLICT,
            "Qlib dump is already running".to_string(),
        ));
    }

    // Spawn background task
    let ps_outer = progress_state.clone();
    tokio::spawn(async move {
        let ps = ps_outer.clone();
        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let dumper = QlibDumper::new(pool, config);
                let ps_cb = ps.clone();

                let stats = dumper
                    .dump(&qlib_dir, move |processed, tot, symbol, msg| {
                        ps_cb.update(processed, tot, symbol, msg);
                    })
                    .await;

                stats
            })
        })
        .await;

        match result {
            Ok(Ok(stats)) => {
                ps_outer.complete(stats);
            }
            Ok(Err(e)) => {
                ps_outer.fail(format!("{}", e));
            }
            Err(e) => {
                ps_outer.fail(format!("spawn_blocking error: {}", e));
            }
        }
    });

    Ok(Json(json!({ "started": true })))
}

/// Handler: Poll Qlib Dump Progress
///
/// Returns the current progress state: `{ running, progress, stats, error }`.
/// When no dump has ever been started, returns `{ running: false, progress: null, stats: null, error: null }`.
async fn get_qlib_progress(State(state): State<AppState>) -> impl IntoResponse {
    let resp = state.qlib_progress.snapshot()
        .unwrap_or(json!({ "running": false, "progress": null, "stats": null, "error": null }));
    (StatusCode::OK, Json(resp)).into_response()
}

async fn sync_stocks_impl(state: &AppState) -> anyhow::Result<()> {
    let downloader = tdx_maintain_core::downloader::DownloaderService::new(state.pool.clone(), state.config.clone());
    downloader.sync_stocks().await
}

