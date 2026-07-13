# 集成 tdxrs.downloader 实现真实日线数据同步与更新

将目前 Mock（占位/生成伪造数据）的日线更新任务，重构为基于 Python `tdxrs.downloader` 模块的真实网络下载。这样可以通过通达信行情网络协议拉取最新交易日的 K 线，自动补齐本地通达信数据文件（`.day` 格式）。

## 用户审核要求

> [!IMPORTANT]
> - 本次修改将引入 Python 的 `tdxrs` 库依赖，数据拉取会直接消耗网络流量并建立与通达信服务器的连接。
> - 在增量更新过程中，系统会严格遵守频控设置（默认盘中 15 rps，盘前后 30 rps，休市 60 rps）以免触发官方防护。
> - 数据更新任务将改为异步调用 Python 子进程执行，由于更新全市场数千只股票需要耗费数分钟至数十分钟（取决于增量天数），我们将在前端支持进度的实时显示和日志查看。

## 方案设计

### 1. 新增 Python 下载桥接脚本

#### [NEW] [download_data.py](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/downloader/download_data.py)
编写专门的 Python 脚本，接受 Rust 端传入的参数：
* `--tdx-dir`：通达信本地数据目录。
* `--mode`：`full`（全量）或 `incremental`（增量）。
* `--rps`：当前限流大小。

在 Python 脚本中：
```python
import sys
from tdxrs.downloader import Downloader

# 伪代码：初始化并执行下载
# 打印特定格式的输出便于 Rust 捕获进度
# print("[PROGRESS] processed: 100/4900")
```

### 2. 修改 Rust 核心 Downloader 模块

#### [MODIFY] [mod.rs](file:///d:/gp/local_tdx/crates/tdx-maintain-core/src/downloader/mod.rs)
* 修改 `run_daily_update` 函数：
  * 不再由 Rust 进行慢速的单线程循环和占位计算。
  * 启动 Python 子进程执行 `download_data.py`。
  * 使用异步流式读取 stdout。解析类似 `[PROGRESS] processed: X/Y` 格式的日志。
  * 实时更新进度，通过 `on_progress` 回调传递给前端。
  * 处理更新完成状态与异常。

---

## 验证计划

### 自动化验证
* 运行 `cargo check --workspace` 确保编译无误。
* 确保本地 Python 成功安装了 `tdxrs`。

### 手动验证
* 启动服务器并打开网页控制台。
* 运行 `daily-increment` 增量更新任务。
* 观察日志与终端输出中是否包含 `tdxrs.downloader` 拉取远程服务器的记录。
* 任务结束后，检查 `D:\new_tdx64\vipdoc\sh\lday\sh000001.day` 文件的最后修改时间和最新行日期，确认是否已被网络下载的 7 月 13 日（周一）数据覆盖。
