import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api } from '../api/client'
import type { DashboardStats, ParquetStats } from '../types'

export const useDashboardStore = defineStore('dashboard', () => {
  const data = ref<DashboardStats>({
    adj_factor_tier: 'L3', last_probe_at: '', last_daily_update: '', last_adj_factor_update: '',
    calendar_source: 'index_derived',
    counts: { open_days: 0, xdxr_events: 0, adj_factor_symbols: 0 },
    daily_bar_range: { start: null, end: null },
  })
  const parquet = ref<ParquetStats>({ exists: false, parquet_dir: '', markets: {}, total_files: 0, total_size_mb: '0.00' })
  const connected = ref(false)
  const connectionText = ref('未连接')

  async function fetch() {
    try {
      data.value = await api.get<DashboardStats>('/api/dashboard')
      connected.value = true
      connectionText.value = '运行中'
    } catch { connected.value = false; connectionText.value = '连接异常' }
  }

  async function fetchParquet() {
    try { parquet.value = await api.get<ParquetStats>('/api/parquet/stats') } catch { /* */ }
  }

  return { data, parquet, connected, connectionText, fetch, fetchParquet }
})
