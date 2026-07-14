# Execution Contract: 前端重构 — Vue 3 + Vite

## Intent Lock

将当前 1292 行单文件 CDN 前端拆分为独立 Vue 3 + Vite + TypeScript 项目，消除 CDN 依赖，构建后嵌入 Rust binary。

**范围围墙**：
- ✅ 新建 `crates/tdx-web/` 独立前端项目
- ✅ 10 个 Vue 组件 + 7 个 API 模块
- ✅ TypeScript 类型定义
- ✅ Tailwind CSS 4（Vite 插件，非 CDN）
- ✅ 构建产物通过 `include_str!` 嵌入
- ✅ 删除旧 `index.html`
- ❌ 不改变任何后端 API 路由
- ❌ 不改变前端功能、UI 布局
- ❌ 不改变 Rust 业务逻辑

## 工件检查清单

| 工件 | 状态 |
|------|------|
| `proposal.md` | ✅ |
| `design.md` | ✅ |
| `tasks.md` | ✅ |
| `specs/` | 🚫 跳过 |

## 执行计划

**Wave 1**: 脚手架（30min）→ `npm run dev`
**Wave 2**: 组件开发（4h）→ 5 个 tab 完整可用
**Wave 3**: 构建集成（30min）→ `cargo build` 包含前端

## DP-3: 契约批准

---

**状态**: ⏳ 待批准
