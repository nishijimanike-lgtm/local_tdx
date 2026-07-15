import { api } from './client'

export interface MarketFreshness {
  market: string
  server_latest_date: string | null
  local_latest_date: string | null
  days_behind: number | null
  total_stocks: number
  up_to_date_stocks: number
  behind_stocks: number
  missing_stocks: number
  status: string // "up_to_date" | "behind" | "no_data" | "server_unreachable"
}

export interface FreshnessSummary {
  total_stocks: number
  up_to_date: number
  behind: number
  missing: number
}

export interface FreshnessReport {
  checked_at: string
  server_reachable: boolean
  needs_update: boolean
  markets: MarketFreshness[]
  summary: FreshnessSummary
}

export function fetchFreshness(): Promise<FreshnessReport> {
  return api.get<FreshnessReport>('/api/checker/freshness')
}
