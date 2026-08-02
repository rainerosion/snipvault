# 潜在问题修复记录（2026-07-31）

> 本文记录 v2.1.2 架构审查后已落实的第一轮数据正确性与可靠性修复。每项包含原问题、影响、根因、方案、兼容行为与验证；尚未解决的风险仍保留在 [已知限制](known-limitations.md)。

## 1. 片段保存与时间戳

### 原问题与影响

普通编辑把前端持有的旧 `updated_at` 写回数据库；保存失败时，未保存提示的调用者仍可能继续切换、新建或清空；新建成功后编辑器又回到空状态。

这会导致排序和 WebDAV 新旧判断失真，也可能在 IPC 保存失败后丢失表单内容。

### 根因

- 前端承担了持久化版本时间的传入职责。
- `handleSave()` 没有向调用者返回明确的成功/失败结果。
- 创建/更新 IPC 不返回数据库采用的权威片段。

### 解决方案

- `create_snippet` 和 `update_snippet` 由 Rust 生成 RFC 3339 时间并返回最终 `Snippet`。
- 更新命令先读取旧记录以保留 `created_at`，不再接受前端 `updated_at`。
- Hook 使用后端返回值更新列表；收藏切换后也重新读取权威记录，不在 WebView 中伪造时间。
- `handleSave(): Promise<boolean>` 只在持久化成功后返回 `true`；切换、新建和取消仅在 `true` 时继续。
- 保存请求提交时记录表单与目标快照；IPC 等待期间若用户继续编辑或导航，完成回调不会用旧快照覆盖较新的 UI 状态。
- 保存成功后将返回片段设为当前选择并重建表单快照。
- 设置层打开时屏蔽片段的 `Ctrl/Meta+N/S/E`，避免背景操作。

### 兼容行为

Snippet JSON 仍保留原字段。创建/更新 IPC 的响应由空值改为 `Snippet`，只影响本仓库内调用方；导入数据仍可携带历史时间，用于备份合并。

### 验证

- `npm run build` 通过。
- Rust CRUD/合并代码通过 `cargo check` 和单元测试。

## 2. SQLite 初始化、搜索与导入

### 原问题与影响

- 用户删除全部片段后，重启会重新插入示例。
- 数据库没有 schema 版本基线。
- 后端 `tag_filter` 参数没有真正过滤标签。
- 导入逐项覆盖、无事务、无输入上限；中途错误可能留下部分写入。
- `INSERT OR REPLACE` 会以删除再插入语义替换行。

### 根因

初始化以 `COUNT(*) == 0` 判断“首次运行”；查询没有展开 tags JSON；导入在校验前直接逐条写入。

### 解决方案

- 使用 `PRAGMA user_version`，建立 schema v1 迁移入口。
- 仅当迁移前不存在 `snippets` 表时插入示例；既有空表保持为空。
- 标签过滤使用 SQLite JSON1 的 `json_valid()` + `json_each()` 做精确值匹配。
- 对片段 ID、标题、正文、描述、语言、标签数量/长度和 RFC 3339 时间统一校验。
- 限制导入文本为 25 MiB、条目数为 10,000，并拒绝同一批次重复 ID。
- 全批校验后在单个 SQLite transaction 中合并；使用 `ON CONFLICT DO UPDATE`。
- 结果改为 `inserted / updated / skipped / input_count`，前端成功数采用新增加更新。
- 更新、删除、收藏检查受影响行数，不再让不存在 ID 静默成功。

### 兼容行为

现有 schema 会被标记为 v1，不重建数据。导入仍接受原有顶层 Snippet 数组，没有引入新的外层格式版本；不合法或超限文件现在会整体拒绝。

### 验证

单元测试覆盖：

- 真正新库插入示例，既有空 v0 表不插入。
- merge 新增/更新/跳过计数。
- 非法批次不会产生部分写入。
- 含 `%` 等 JSON 字符的标签精确匹配。

## 3. 设置持久化与自启动一致性

### 原问题与影响

设置先改内存、再直接覆盖 `settings.json`；写入失败可造成内存/磁盘分叉，崩溃可留下截断 JSON。OS 自启动切换成功但配置保存失败时，两侧状态也可能不一致。

### 根因

没有 candidate-before-commit、临时文件替换、恢复备份和 OS 状态补偿。

### 解决方案

- 在 clone 的候选设置上修改并校验，磁盘替换成功后才更新内存。
- 写入唯一临时文件，执行 `write_all` / `sync_all`，将旧文件改名为 `.bak` 后安装新文件。
- 启动时在目标文件缺失而备份存在时恢复；新文件已安装后，备份清理失败只记录 warning，不把一次成功提交误报为失败。
- 校验 theme、language、auth mode、超时、自动同步间隔、字段长度和 `last_sync_at`。
- 兼容旧配置：自动同步关闭时允许间隔 `0`；启用时必须为 1–1440 分钟。
- `save_settings` 返回后端最终设置；`last_sync_at` 始终由后端保留。
- 保存涉及自启动时，先应用 OS 状态；持久化失败则尝试恢复旧 OS 状态，并报告补偿失败。

### 兼容行为

`#[serde(default)]` 继续为旧配置补字段。损坏 JSON 当前仍会在启动日志报错并使用默认设置；自动隔离损坏文件尚未实现。

### 验证

单元测试覆盖设置完整写入/读取、备份恢复和枚举/间隔校验。

## 4. 窗口与自动同步运行时设置

### 原问题与影响

`minimize_to_tray` 和自动同步间隔在启动时捕获；运行中保存开关或间隔通常需要重启，关闭后的旧同步线程也无法恢复。

### 根因

窗口事件闭包捕获布尔值，自动同步只按启动配置创建一次固定 sleep 线程。

### 解决方案

- `CloseRequested` 每次触发时读取最新后端设置。
- 启动一个常驻轻量 worker，每 15 秒读取一次当前设置。
- 关闭自动同步、URL 为空或间隔无效时重置调度；重新启用后自动恢复。
- 间隔变化由下一轮轮询采用，无需重启。
- 自动同步与手动/托盘同步共用进程级同步锁；已有任务运行时跳过本轮，而不并发覆盖 manifest。

### 兼容行为

重新启用或应用启动时，满足配置的自动同步会在最多约 15 秒内首次尝试；这是从“先等待完整间隔”调整为“worker 检测后执行”。后台同步仍只写日志，不会主动刷新已打开的前端列表。

### 验证

`cargo check`、`cargo test` 通过；运行时线程行为仍需桌面端人工 smoke test。

## 5. WebDAV 数据安全与同步一致性

### 原问题与影响

旧流程会把远端独有片段下载后又按同步前快照删除，并用旧快照覆盖 manifest；多个入口可并发同步。URL/ID 拼接、响应大小、manifest 内容和错误日志也缺少防护。

### 根因

v1 协议没有 tombstone，却把“本地不存在”解释为远端删除；没有完整同步锁、最终状态重读、URL 结构化构建和远端数据边界校验。

### 解决方案

- 采用保守合并：v1 不再从“某端缺失”推断删除，远端独有数据只下载，不删除。
- 所有入口通过进程级 `SYNC_LOCK` 串行化；并发请求返回“已有同步任务”。
- 完成下载/合并后重新读取数据库，按最终状态生成 manifest 和总数。
- 统计使用 merge 的 `inserted + updated` 作为下载变更数。
- 使用 `reqwest::Url` 和 path-segment API；`<id>.json` 作为一个段编码，避免斜杠、查询或片段改变目标。
- 本轮当时将 Base URL 限制为 HTTP(S)，拒绝 URL userinfo、query 和 fragment，并为 HTTP 成功结果附带明文传输警告；后续 [凭据/设置安全与 CSP 检查点](remediation-round-2.md#9-凭据设置安全与-csp-检查点) 已进一步改为远端 HTTPS 强制、只允许严格 loopback HTTP 测试。
- 校验 manifest version、条数、重复 ID、时间戳、片段 ID/字段和 payload ID 一致性。
- manifest 与片段响应有大小上限；生成 manifest 时也在 PUT 前执行相同 5 MiB 上限，避免上传下一轮无法读取的对象；错误不回显 URL、凭据或服务端 body。
- 本轮最初对同时间版本通过 HEAD（不支持时回退 GET）检查 payload 是否存在并补传缺失文件；后续 [WebDAV 可测试同步引擎基础检查点](remediation-round-2.md#12-webdav-可测试同步引擎基础检查点) 已改为始终 GET/校验等时间 payload，以固定 v1 DTO 的规范字节序确定性选择内容赢家，并在 manifest PUT 后验证全部引用 payload。
- 写 manifest 前循环重读最终数据库版本并补传变化中的 payload，直到连续快照版本一致，避免编辑与同步并行时 manifest 声明新时间但远端仍是旧内容。
- 同步历史和 `last_sync_at` 持久化失败不再静默忽略。

### 兼容行为

远端布局与 manifest v1 格式不变。删除传播被有意停用：本地删除不会删除远端，后续同步可能把远端副本重新下载。这是无 tombstone 协议下避免误删的安全取舍。

### 仍未解决

- 没有 ETag / `If-Match`，不同设备同时写 manifest 仍可能最后写入者覆盖。
- 远端 v1 wire 仍没有 tombstone、revision、设备 ID、冲突副本和明确删除传播。本地 revision/tombstone/outbox 与 conflict DB seam 后来已由 [SQLite v3 检查点](remediation-round-2.md#13-sqlite-v3-revisiontombstoneoutbox-foundation-检查点) 建立，但 v1 不消费这些 metadata。
- HTTP 操作与本地 SQL 无法组成跨系统原子事务；失败后依靠幂等重试收敛。
- 尚无 mock WebDAV 端到端集成测试的问题已由 [WebDAV 可测试同步引擎基础检查点](remediation-round-2.md#12-webdav-可测试同步引擎基础检查点) 解决；仍无真实第三方服务器兼容性测试。

### 验证

单元测试覆盖 URL 策略、路径段编码、manifest 内容/体积校验和错误脱敏；`cargo test --locked --offline` 共 13 项通过。

### 后续 protocol-v2 activation（2026-08-02）

本节以上内容保留第一轮修复完成时的 v1 历史状态。后续 [WebDAV protocol-v2 activation 检查点](remediation-round-2.md#15-webdav-protocol-v2-activation-检查点) 已将 production facade 切换到 v2：使用 `snipvault/protocol-v2.json`、v2 `manifest.json` 与不可变 `snipvault/objects/<revision_uuid>.json`，通过 tombstone revision 传播删除，并要求 strong ETag 和 conditional PUT 进行 manifest CAS。该升级是单向 v1→v2；旧逐片段 payload 保留但激活后忽略。

因此本历史节“没有 ETag / tombstone / v2 wire”的条目不再是当前生产行为；HTTP/SQLite 非跨系统事务、无限期 revision/tombstone retention、冲突解决 UI 和真实第三方服务兼容性仍属于 [当前已知限制](known-limitations.md#1-webdav-v2-协议与多设备并发)。v2 自动化仅使用 synthetic/pure unit/fake；现有 loopback HTTP suite 仍覆盖 retained legacy-v1 transport/engine，尚无 v2 marker/object/conditional-CAS wire integration。没有真实服务测试，也没有 production Tauri desktop smoke。

## 6. 验证摘要

本轮完成时执行：

- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --locked --offline --manifest-path src-tauri/Cargo.toml`

最终完整检查结果以本次交付说明为准。
