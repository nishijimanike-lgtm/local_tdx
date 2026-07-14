# Tasks: 前端重构 — Vue 3 + Vite

## Wave 1: 项目脚手架

- [ ] **1.1 初始化 Vite + Vue 3 + TS 项目**
  - `npm create vite@latest tdx-web -- --template vue-ts` 在 `crates/tdx-web/`
  - 安装依赖：`vue`, `typescript`, `@vitejs/plugin-vue`
  - **验证**: `npm run dev` 启动成功

- [ ] **1.2 配置 Tailwind CSS 4**
  - 安装 `tailwindcss @tailwindcss/vite`
  - 配置 `vite.config.ts` 插件
  - 迁移现有自定义 CSS（glass-panel, glow-indigo, 滚动条, 动画）
  - **验证**: 样式渲染正确

- [ ] **1.3 创建 TypeScript 类型定义**
  - `src/types/index.ts`: Dashboard, TaskProgress, Settings, Alert, CalendarDay, ParquetStats
  - 从原 `index.html` 中提取所有数据结构
  - **验证**: `tsc --noEmit`

## Wave 2: 组件开发

- [ ] **2.1 App.vue + Sidebar.vue**
  - 侧边栏导航（5 个 tab），activeTab 状态
  - 服务器连接状态指示器
  - 时钟显示
  - **验证**: 页面布局正常，tab 切换正常

- [ ] **2.2 DashboardTab.vue**
  - 统计卡片（6 项指标）
  - Parquet 存储统计折叠区域
  - 日线数据范围显示
  - API 调用：`/api/dashboard`, `/api/parquet/stats`
  - **验证**: 仪表盘数据正确展示

- [ ] **2.3 TasksTab.vue + TaskProgress.vue + ConsoleLog.vue**
  - 任务触发按钮（6 个任务）
  - 进度条 + 状态徽章（暂停/运行/完成）
  - SSE 实时进度订阅 (`/api/tasks/progress`)
  - 暂停/恢复/中止控制按钮
  - 控制台日志输出
  - **验证**: 触发任务 → 进度更新 → 控制按钮生效

- [ ] **2.4 CalendarTab.vue**
  - 日期范围选择器
  - 交易日历表格
  - API 调用：`/api/calendar`
  - **验证**: 日历数据正确展示

- [ ] **2.5 AlertsTab.vue**
  - 告警列表 + 未读计数
  - 确认按钮
  - API 调用：`/api/alerts`, `/api/alerts/{id}/acknowledge`
  - **验证**: 告警列表正确，确认生效

- [ ] **2.6 SettingsTab.vue**
  - 完整设置表单（9 个配置段）
  - 保存按钮 → API 调用：`/api/settings` (PUT)
  - 重启提示
  - **验证**: 设置保存后读取正确

- [ ] **2.7 Toast.vue**
  - 全局 toast 通知系统
  - 3 秒自动消失
  - **验证**: toast 正确弹出和消失

## Wave 3: 构建集成

- [ ] **3.1 Vite 构建配置**
  - `base: '/'`，`build.outDir: 'dist'`
  - 确保 `index.html` 无 CDN 外部引用
  - **验证**: `npm run build` 成功

- [ ] **3.2 Rust 集成**
  - 修改 `main.rs` 中 `include_str!` 路径
  - 验证编译：`cargo build`
  - **验证**: 启动服务 → 前端正常渲染

- [ ] **3.3 删除旧前端**
  - 删除 `crates/tdx-maintain-server/src/index.html`
  - 更新 `.gitignore`（添加 `crates/tdx-web/dist/`, `crates/tdx-web/node_modules/`）

## 依赖图

```
1.1 → 1.2 → 1.3
              │
              ├→ 2.1 → 2.2
              │       2.3
              │       2.4
              │       2.5
              │       2.6
              │       2.7
              │
              └→ 3.1 → 3.2 → 3.3
```

## 工作量估算

| Wave | 预计工时 | 风险 |
|------|---------|------|
| 1. 脚手架 | ~30min | 低 |
| 2. 组件开发 | ~4h | 中 — 像素级还原 |
| 3. 构建集成 | ~30min | 低 |
| **合计** | **~5h** | |
