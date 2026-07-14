# Proposal: 前端重构 — 独立 Vue 3 + Vite 项目

## 概述

将当前 1292 行单文件 `index.html`（Vue 3 CDN + Tailwind CDN）重构为独立的 **Vue 3 + Vite + TypeScript** 前端项目，构建后生成静态文件嵌入 Rust binary。

## 动机

- **单文件膨胀**：1292 行 HTML/CSS/JS 混在一起，难以维护和协作
- **离线不可用**：依赖 CDN（Tailwind、Vue、Google Fonts），断网时前端白屏
- **零工具链**：无 TypeScript 类型检查、无热更新、无 ESLint
- **模块化缺失**：5 个 tab 的逻辑全在单一 `setup()` 函数中

## 变更范围

### 1. 新建 Vue 3 + Vite 项目

- **目录**: `crates/tdx-web/`（独立前端项目）
- **技术栈**: Vue 3.5 + Vite 6 + TypeScript + Tailwind CSS 4
- **构建输出**: `dist/` → `index.html` 通过 `include_str!` 嵌入 Rust

### 2. 组件拆分

| 组件 | 对应原区域 |
|------|----------|
| `App.vue` | 根布局（侧边栏 + 主内容） |
| `DashboardTab.vue` | 仪表盘：统计卡片、Parquet 状态、日线范围 |
| `TasksTab.vue` | 任务调度：触发按钮、进度条、控制台、SSE |
| `CalendarTab.vue` | 交易日历：日期范围查询、表格 |
| `AlertsTab.vue` | 告警看板：告警列表、确认操作 |
| `SettingsTab.vue` | 全局设置：表单、保存/重启提示 |
| `TaskProgress.vue` | 可复用进度条组件 |
| `ConsoleLog.vue` | 可复用终端日志组件 |
| `Toast.vue` | 可复用 Toast 通知组件 |
| `StatusIndicator.vue` | 连接状态指示灯 |

### 3. API 层抽象

- `src/api/` 目录：`dashboard.ts`, `tasks.ts`, `calendar.ts`, `alerts.ts`, `settings.ts`, `parquet.ts`
- 封装所有 `/api/*` 调用，统一错误处理
- 使用 `fetch`（无需引入 axios，减少依赖）

### 4. 构建集成

- Vite 配置 `base: '/'`，输出到 `dist/`
- Rust 端：`include_str!("../tdx-web/dist/index.html")` 替换原有 `include_str!("index.html")`
- 构建脚本：`build.rs` 或 Makefile 中先 `npm run build` 再 `cargo build`

## 不受影响的部分

- 后端 API 路由、数据结构完全不变
- 前端功能、UI 布局保持不变（像素级还原）
- Rust 代码不变（仅 `main.rs` 中 `include_str!` 路径变更）

## 验证计划

1. `npm run dev` → 前端独立开发服务器正常
2. `npm run build` → `dist/` 产出静态文件
3. `cargo build` → Rust binary 包含前端构建产物
4. 启动服务，手动验证 5 个 tab 功能正常
5. 断网测试 → 前端应在无 CDN 的情况下完全可用
