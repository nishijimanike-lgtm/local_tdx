# Frontend Redesign Plan — Phase 1: Foundation

## Goal
Deep refactor of tdx-web frontend with Vue Router, Pinia, composables, lazy loading, PWA, accessibility.

## Architecture

```
src/
├── main.ts                    # App entry + router + pinia
├── App.vue                    # Shell: sidebar + router-view
├── router/
│   └── index.ts               # 6 routes with lazy loading
├── stores/
│   ├── dashboard.ts           # Dashboard stats + parquet
│   ├── tasks.ts               # Task progress + SSE + history
│   ├── alerts.ts              # Alerts list + acknowledge
│   ├── calendar.ts            # Calendar query + data
│   └── settings.ts            # Settings load + save
├── composables/
│   ├── useSSE.ts              # SSE subscription logic
│   ├── useToast.ts            # Toast notification system
│   └── useClock.ts            # Clock timer
├── components/
│   ├── layout/
│   │   ├── AppShell.vue       # Sidebar + header + main
│   │   ├── Sidebar.vue        # Nav + mobile hamburger
│   │   └── TopBar.vue         # Header: title + clock + status
│   ├── dashboard/
│   │   ├── DashboardView.vue  # Route component
│   │   ├── StatCard.vue       # Reusable stat card
│   │   └── ParquetPanel.vue   # Parquet stats section
│   ├── download/
│   │   └── AfterMarketDownload.vue  # (existing, refactored)
│   ├── tasks/
│   │   ├── TasksView.vue      # Route component
│   │   ├── TaskTrigger.vue    # Trigger buttons
│   │   ├── ProgressBar.vue    # Progress + controls
│   │   └── TaskHistory.vue    # Recent tasks table
│   ├── calendar/
│   │   └── CalendarView.vue
│   ├── alerts/
│   │   └── AlertsView.vue
│   ├── settings/
│   │   └── SettingsView.vue
│   └── ui/
│       ├── Toast.vue           # Toast notifications
│       ├── StatusDot.vue       # Connection indicator
│       ├── LoadingSkeleton.vue # Skeleton loader
│       └── ErrorBoundary.vue   # Error fallback
├── types/
│   └── index.ts               # Shared types
├── utils/
│   └── format.ts              # Date/time formatters
└── style.css                  # Tailwind + custom
```

## Key Decisions

1. **Pinia** replaces inject/provide for cross-component state
2. **Vue Router** for proper URL-based tab navigation  
3. **Composables** extract reusable logic from App.vue
4. **Lazy loading** via `defineAsyncComponent` per route
5. **PWA** via `vite-plugin-pwa` for offline capability
6. **Accessibility**: ARIA landmarks, keyboard focus traps, semantic HTML

## Cleanup

- Remove `HelloWorld.vue`, `vite.svg`, `vue.svg`, `hero.png`
- Remove `provide`/`inject` dependency chains
- Consolidate Tailwind utility patterns

## File Changes Summary

| Operation | Files |
|-----------|-------|
| **New** | router/index.ts, 5 Pinia stores, 3 composables, 14 components |
| **Modified** | main.ts, App.vue, style.css, index.html, vite.config.ts, package.json |
| **Deleted** | HelloWorld.vue, vite.svg, vue.svg, hero.png |
| **Kept (refactored)** | AfterMarketDownload.vue, CalendarTab→CalendarView, AlertsTab→AlertsView, SettingsTab→SettingsView, TasksTab→TasksView, DashboardTab→DashboardView, Sidebar, StatusIndicator→StatusDot, ToastContainer→Toast |
