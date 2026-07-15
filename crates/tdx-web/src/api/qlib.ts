import { api } from './client'

export interface QlibDumpStats {
  total_files: number
  a_stock_count: number
  processed: number
  skipped: number
  failed: number
  failures: string[]
  calendar_days: number
  output_dir: string
  elapsed_secs: number
}

export interface QlibProgress {
  running: boolean
  progress?: {
    processed: number
    total: number
    current_symbol: string
    message: string
  }
  stats?: QlibDumpStats
  error?: string
}

export const qlibApi = {
  /** Trigger Qlib binary data dump (returns immediately, use pollProgress for updates) */
  dump(): Promise<{ started: boolean }> {
    return api.post<{ started: boolean }>('/api/qlib/dump')
  },

  /** Poll current dump progress */
  progress(): Promise<QlibProgress> {
    return api.get<QlibProgress>('/api/qlib/progress')
  },
}
