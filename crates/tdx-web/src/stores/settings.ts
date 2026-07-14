import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api } from '../api/client'
import type { ServerSettings } from '../types'

export const useSettingsStore = defineStore('settings', () => {
  const data = ref<ServerSettings>({
    server: { host: '127.0.0.1', port: 8080 },
    paths: { tdx_data_dir: '', metadata_db_path: '', backup_dir: '', parquet_dir: '' },
    calendar: { benchmark_index_market: 1, benchmark_index_symbol: '000001', exchange: 'SSE' },
    tushare: { enabled: false, token: '', base_url: 'http://api.tushare.pro' },
    rate_limit: { market_hours_rps: 100, pre_post_market_rps: 150, off_hours_rps: 200 },
    adj_factor: { conflict_threshold_pct: 1.0, default_tier: 'L3' },
    alerts: { daily_completeness_threshold_pct: 95.0 },
    schedule: { daily_increment_cron: '', xdxr_sync_cron: '', adj_factor_sync_cron: '', calendar_check_cron: '', weekly_scan_cron: '' },
    retry: { max_attempts: 3, backoff_ms: 1000 },
  })

  async function fetch() { try { data.value = await api.get<ServerSettings>('/api/settings') } catch { /* */ } }

  async function save() {
    try { await api.put('/api/settings', data.value) } catch (e: any) { throw e }
  }

  return { data, fetch, save }
})
