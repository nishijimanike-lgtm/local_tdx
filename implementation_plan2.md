# 下载器增强 — Python→Rust 重写 + 稳定性增强 ✅ 已完成

> **Status**: 本计划已实现。Python 子进程下载器已完全替换为 Rust 原生实现（使用 `rustdx` crate）。任务控制从 stdin 管道改为了 `Arc<AtomicU8>` 共享状态。

## 用户审核要求

> [!IMPORTANT]
> - **控制机制**：任务的暂停与恢复仅针对耗时较长（分钟到小时级）的**日K线更新任务**。日历同步、除权同步等秒级完成的任务仅支持中止。
> - **进程通信**：Rust 任务管理器与 Python 数据下载子进程之间将采用标准的管道输入（`stdin`）传递控制信令（`PAUSE`、`RESUME`、`ABORT`），无平台限制且极为稳定。
> - **资源释放**：中止任务时，Rust 会即刻强制终止 Python 子进程以释放网络及 CPU 资源。

## 方案设计

### 1. Python 下载脚本自定义循环与管道监听

#### [MODIFY] [download_data.py](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/downloader/download_data.py)
- **多线程指令监听**：启动后台线程读取标准输入（`sys.stdin`）。解析 `PAUSE`、`RESUME`、`ABORT` 命令并修改全局状态字典。
- **逐只股票循环控制**：不再直接调用 `dl.run()`，改为在 Python 中自己循环股票列表：
  1. 调用 `dl._fetch_stock_list()` 获取股票代码列表。
  2. 迭代下载每只股票，在下载单只股票前检查暂停和中止状态。
  3. 暂停时，进行线程 `sleep(0.5)` 挂起。
  4. 中止时，打印摘要并调用 `sys.exit(0)` 退出。

### 2. Rust 核心任务管理器增强

#### [MODIFY] [mod.rs (task)](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/task/mod.rs)
- **任务状态存储**：在 `TaskQueue` 中保存当前运行任务的控制句柄：
  ```rust
  pub struct ActiveTask {
      pub task_id: i64,
      pub kind: TaskKind,
      pub stdin: Option<tokio::process::ChildStdin>,
      pub paused: bool,
  }
  ```
- **控制管道建立**：生成子进程时使用 `.stdin(Stdio::piped())` 并在 `TaskQueue` 中维护 stdin 写句柄。
- **任务控制 API**：实现 `pause()`、`resume()`、`abort()` 方法，向 stdin 写入对应指令并更新全局状态及 SSE 数据。

### 3. Axum 路由接口扩展

#### [MODIFY] [main.rs](file:///d:/gp/local_tdx/crates/tdx-maintain-server/src/main.rs)
- 新增 `POST /api/tasks/pause` 接口。
- 新增 `POST /api/tasks/resume` 接口。
- 新增 `POST /api/tasks/abort` 接口。

### 4. 网页端控制台界面（Vue3）

#### [MODIFY] [index.html](file:///d:/gp/local_tdx/crates/tdx-maintain-server/src/index.html)
- 在“当前执行任务”面板中添加“暂停/恢复”及“中止”按钮。
- 在任务暂停时，进度条动画变为黄色呼吸灯脉冲，并显示“已暂停”徽标。

---

## 验证计划

### 自动化验证
* 运行 `cargo check --workspace` 确保编译无误。

### 手动验证
1. 启动服务，点击“增量更新”开始同步。
2. 看到数值增加后，点击“暂停”，确认数量暂停增长，状态栏转为黄色。
3. 点击“恢复”，确认下载继续，数量重新增长。
4. 点击“中止”，确认后台 Python.exe 进程立即被杀掉，控制台任务状态变为失败/中止。
