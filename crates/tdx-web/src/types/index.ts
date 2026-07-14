export interface DashboardStats {
  adj_factor_tier: string
  last_probe_at: string
  last_daily_update: string
  last_adj_factor_update: string
  calendar_source: string
  counts: {
    open_days: number
    xdxr_events: number
    adj_factor_symbols: number
  }
  daily_bar_range: {
    start: string | null
    end: string | null
  }
}

export interface ParquetStats {
  exists: boolean
  parquet_dir: string
  markets: Record<string, { files: number; size_mb: string }>
  total_files: number
  total_size_mb: string
}

export interface TaskProgress {
  task_id: number
  task_type: string
  done: number
  skipped: number
  failed: number
  total: number
  message: string
  finished: boolean
  paused: boolean
  aborted: boolean
}

export interface TaskLog {
  id: number
  task_type: string
  started_at: string
  finished_at: string | null
  status: string
  done_count: number
  skipped_count: number
  failed_count: number
  detail: string | null
}

export interface Alert {
  id: number
  level: string
  category: string
  message: string
  detail: string | null
  acknowledged: number
  created_at: string
}

export interface CalendarDay {
  exchange: string
  trade_date: string
  is_open: number
  source: string
  updated_at: string
}

export interface ServerSettings {
  server: { host: string; port: number }
  paths: {
    tdx_data_dir: string
    metadata_db_path: string
    backup_dir: string
    parquet_dir: string
  }
  calendar: {
    benchmark_index_market: number
    benchmark_index_symbol: string
    exchange: string
  }
  tushare: { enabled: boolean; token: string; base_url: string }
  rate_limit: {
    market_hours_rps: number
    pre_post_market_rps: number
    off_hours_rps: number
  }
  adj_factor: { conflict_threshold_pct: number; default_tier: string }
  alerts: { daily_completeness_threshold_pct: number }
  schedule: {
    daily_increment_cron: string
    xdxr_sync_cron: string
    adj_factor_sync_cron: string
    calendar_check_cron: string
    weekly_scan_cron: string
  }
  retry: { max_attempts: number; backoff_ms: number }
}

export interface TaskDefinition {
  name: string
  action: string
  desc: string
  cronKey: string
}

export interface ConsoleLogEntry {
  time: string
  tag: string
  text: string
  tagClass: string
  textClass: string
}

export interface Toast {
  id: number
  message: string
  type: 'success' | 'error' | 'info'
}
