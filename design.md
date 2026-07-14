# Design: 前端重构 — Vue 3 + Vite

## ADR 决策

### ADR-1: 独立 Vite 项目而非保持内嵌 HTML

- **理由**: 获得 TypeScript、模块化、HMR、构建优化、离线支持
- **风险**: 需要额外构建步骤 → `build.rs` 或 Makefile 自动化

### ADR-2: Vue 3 Composition API + `<script setup>` + TypeScript

- **理由**: 与现有 CDN 版 Vue 3 风格一致，迁移成本最低
- **替代**: 不考虑 React/Svelte（切换成本高）

### ADR-3: Tailwind CSS 4（Vite 插件）替代 CDN

- **理由**: 构建时 tree-shaking，仅包含用到的 class，显著减小体积
- **影响**: 原有自定义 CSS（~60行）迁移到 Tailwind 类

### ADR-4: `include_str!` 保持不变（不引入复杂构建工具）

- **理由**: 简单可靠，前端 dist 文件不大（预估 < 150KB gzip）
- **替代**: 不考虑 `rust-embed`（增加依赖）

## 目录结构

```
crates/tdx-web/
├── index.html                  # Vite entry
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── src/
│   ├── main.ts                 # Vue app entry
│   ├── App.vue                 # Root layout
│   ├── components/
│   │   ├── Sidebar.vue         # Nav sidebar with tabs
│   │   ├── DashboardTab.vue    # Stats cards + Parquet
│   │   ├── TasksTab.vue        # Task triggers + SSE
│   │   ├── CalendarTab.vue     # Calendar query
│   │   ├── AlertsTab.vue       # Alerts list
│   │   ├── SettingsTab.vue     # Settings form
│   │   ├── TaskProgress.vue    # Progress bar
│   │   ├── ConsoleLog.vue      # Terminal log
│   │   ├── Toast.vue           # Notifications
│   │   └── StatusIndicator.vue # Connection status
│   ├── api/
│   │   ├── client.ts           # fetch wrapper with error handling
│   │   ├── dashboard.ts
│   │   ├── tasks.ts
│   │   ├── calendar.ts
│   │   ├── alerts.ts
│   │   ├── settings.ts
│   │   └── parquet.ts
│   └── types/
│       └── index.ts            # Shared TypeScript types
└── dist/                       # Build output (gitignored)
    └── index.html              # → Rust include_str!()
```

## API 层设计

```typescript
// src/api/client.ts
export async function apiGet<T>(url: string): Promise<T>
export async function apiPost<T>(url: string, body?: unknown): Promise<T>
export async function apiPut<T>(url: string, body: unknown): Promise<T>
export async function apiPatch<T>(url: string, body?: unknown): Promise<T>
```

## Rust 端变更

```rust
// crates/tdx-maintain-server/src/main.rs
// BEFORE:
Html(include_str!("index.html"))
// AFTER:  
Html(include_str!("../tdx-web/dist/index.html"))
```

## 构建流程

```
make build:                # New Makefile target
  cd crates/tdx-web && npm install && npm run build
  cargo build --release
```
