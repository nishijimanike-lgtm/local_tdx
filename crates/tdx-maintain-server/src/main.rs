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
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tdx_maintain_core::{
    db::repos::*,
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
        .route("/api/health", get(health_check))
        .route("/api/dashboard", get(get_dashboard))
        .route("/api/parquet/stats", get(get_parquet_stats))
        .route("/api/calendar", get(get_calendar).post(update_calendar))
        .route("/api/scan/results/{id}", get(get_scan_results))
        .route("/api/scan/{type}", post(run_scan))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks/control/pause", post(pause_task))
        .route("/api/tasks/control/resume", post(resume_task))
        .route("/api/tasks/control/abort", post(abort_task))
        .route("/api/tasks/{action}", post(trigger_task))
        .route("/api/tasks/progress", get(subscribe_progress))
        .route("/api/settings", get(get_settings).put(update_settings))
        .route("/api/alerts", get(list_alerts))
        .route("/api/alerts/{id}/acknowledge", patch(acknowledge_alert))
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
async fn serve_dashboard() -> impl IntoResponse {
    Html(include_str!("index.html"))
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

// Handler: Dashboard Stats
async fn get_dashboard(State(state): State<AppState>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let meta_repo = SyncMetaRepo::new(&state.pool);
    let calendar_repo = CalendarRepo::new(&state.pool);
    let xdxr_repo = XdxrRepo::new(&state.pool);
    let adj_repo = AdjFactorRepo::new(&state.pool);

    let adj_factor_tier = meta_repo.get("adj_factor_tier").await.unwrap_or(None).unwrap_or_else(|| "L3".to_string());
    let last_probe_at = meta_repo.get("last_probe_at").await.unwrap_or(None).unwrap_or_default();
    let last_daily_update = meta_repo.get("last_daily_update").await.unwrap_or(None).unwrap_or_default();
    let last_adj_factor_update = meta_repo.get("last_adj_factor_update").await.unwrap_or(None).unwrap_or_default();
    let calendar_source = meta_repo.get("calendar_source").await.unwrap_or(None).unwrap_or_else(|| "index_derived".to_string());

    let open_days_count = calendar_repo.count_open_days(&state.config.calendar.exchange).await.unwrap_or(0);
    let xdxr_events_count = xdxr_repo.count().await.unwrap_or(0);
    let adj_factor_symbols_count = adj_repo.count_symbols().await.unwrap_or(0);

    // Compute daily bar date range from benchmark index files (SH 000001 and SZ 399001)
    let tdx_data_dir = state.config.paths.tdx_data_dir.clone();
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

        for (market, code) in &index_candidates {
            let filename = get_day_filename(*market, code, &base_dir);
            let path = base_dir.join(market.dir_name()).join("lday").join(&filename);
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

        (first_date, last_date)
    }).await.unwrap_or((None, None));

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

// Handler: Trigger Task
async fn trigger_task(
    State(state): State<AppState>,
    Path(action): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let kind = match action.as_str() {
        "daily-full" => TaskKind::DailyFull,
        "daily-increment" => TaskKind::DailyIncrement,
        "daily-gap-fill" => TaskKind::DailyGapFill,
        "xdxr-sync" => TaskKind::XdxrSync,
        "adj-factor-sync" => TaskKind::AdjFactorSync,
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
