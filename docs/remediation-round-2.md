# 第二轮修复 Phase 0 与可靠性/安全检查点实施记录

> 本文记录第二轮整改计划 Phase 0 的工程质量基础设施，以及后续结构化错误、共享设置、凭据/CSP、数据/搜索、可访问性、WebDAV v1/v2、SQLite v3/v4 和发布链路硬化检查点。它不替代第一轮 [数据正确性与可靠性修复记录](remediation-2026-07-31.md)。

## 1. 范围

本阶段只处理开发与验证基础设施：

- 校正仓库级开发指南中已被第一轮修复改变的事实。
- 建立前端 typecheck、test、lint 和受控 format gate。
- 建立可复用 Tauri API 测试 mock 与一个有实际行为覆盖的组件 smoke test。
- 建立 Markdown 相对链接/anchor 和当时的三文件应用版本一致性检查；后续发布链路硬化已将版本门禁扩展到 Cargo.lock、Vite UI 版本和 release tag。
- 为普通 push/pull request 增加 Linux full gate。
- 同步开发指南和已知限制。

本阶段没有执行发布。Phase 0 本身没有改变用户功能、IPC、持久化 schema、权限或 WebDAV 协议；本文件后半另行记录已实现的可靠性检查点。

## 2. 前端质量基础设施

[package.json](../package.json) 新增以下命令：

- `typecheck`
- `test`
- `test:run`
- `lint`
- `format:check`
- `docs:check`
- `versions:check`

测试栈使用 Vitest、jsdom、React Testing Library、`user-event`、jest-dom 和 `jest-axe`。ESLint 使用 TypeScript、React、Hooks 和 Vite fast-refresh 插件。Prettier 当前只检查本阶段维护的测试、工具、配置和普通 CI workflow，避免为启用 gate 广泛改写既有业务源码或文档。

[setup.ts](../src/test/setup.ts) 集中提供：

- Tauri core `invoke` mock。
- Tauri event `listen` / `once` / `emit` / `emitTo` mock。
- clipboard `readText` / `writeText` mock。
- window `getCurrentWindow` 及窗口方法 mock。
- Testing Library cleanup、jest-dom 和 axe assertion。

[Titlebar.test.tsx](../src/test/Titlebar.test.tsx) 是当前最小 smoke harness。它验证最小化、最大化/还原、关闭、resize 状态更新、listener cleanup，并执行 axe 扫描；不是全应用 E2E，也不代替真实 Tauri 窗口验证。

## 3. 仓库完整性检查

[check-doc-links.mjs](../scripts/check-doc-links.mjs) 不引入额外 parser 依赖，检查：

- 根 [README](../README.md)。
- 根 [CLAUDE.md](../CLAUDE.md)。
- `docs/` 下所有 Markdown 文件。
- 相对文件目标是否存在。
- 指向 Markdown 的本地 anchor 是否匹配 GitHub 风格 heading slug。

它有意不请求外部 URL，也不渲染 Mermaid。

[check-versions.mjs](../scripts/check-versions.mjs) 比较：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Phase 0 当时不校验 release tag、Settings UI 硬编码版本或 Cargo.lock package entry；这些门禁已在第 16 节发布链路硬化中扩展。

## 4. 普通 CI

[ci.yml](../.github/workflows/ci.yml) 在普通 push 和 pull request 上运行 Ubuntu 22.04 full gate：

1. `npm ci`
2. `npm run format:check`
3. `npm run lint`
4. `npm run typecheck`
5. `npm run test:run`
6. `npm run build`
7. `npm run docs:check`
8. `npm run versions:check`
9. Rust `fmt --check`
10. Rust `check`
11. Rust `clippy --all-targets -- -D warnings`
12. Rust `test`

workflow 使用只读 contents 权限，不构建发布包、不签名、不上传 artifact、不创建 release。tag/manual 发布仍由 [release.yml](../.github/workflows/release.yml) 负责。

## 5. 仓库事实校正

[CLAUDE.md](../CLAUDE.md) 已与当前源码对齐：

- 当前有 13 个 Rust 单元测试和最小前端测试 harness。
- 普通片段时间戳由 Rust 成功写入路径生成并返回权威片段。
- `minimize_to_tray` 与自动同步开关/间隔运行时动态读取。
- SQLite 已有 transactional schema-v1 migration baseline。
- WebDAV remote-only 数据采用保守下载/不推断删除，manifest 从最终状态重建。
- 所有同进程同步入口共用 process-level mutex；跨设备 ETag/锁仍未实现。

原有 Documentation Gate 保留，并因新增开发命令/验证流程同步更新了 [开发指南](development.md) 和 [已知限制](known-limitations.md)。

## 6. Phase 0 验证状态

实现完成后已运行并通过：

- `npm run format:check`
- `npm run typecheck`
- `npm run test:run`（1 个文件，2 个测试）
- `npm run build`
- `npm run docs:check`（Phase 0 当时的快照：9 个 Markdown 文件、216 个相对链接）
- `npm run versions:check`（三处均为 `2.1.0`）
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml`（13 项通过）

`npm run lint` 已运行且退出成功，但仍报告 3 个既有 warning：一个 CodeMirror callback dependency warning，以及 `main.tsx` 两个 fast-refresh export warning。

`npm run build` 已通过，同时报告 Tauri window 模块被动态和静态导入，以及 editor chunk 超过 500 kB 的 warning。最终 `git diff --check` 结果以本次交付报告为准。真实 Tauri 桌面应用未运行，因为本阶段未改变用户流程、系统 API、权限或后端业务行为。

## 7. 可靠性检查点：结构化 IPC 错误与操作反馈

本检查点在 Phase 0 测试/CI 基线上实现：

- 新增 [error.rs](../src-tauri/src/error.rs) 的可序列化 `CommandError`，稳定 code 为 validation、not_found、database、settings、network、sync_busy、import、export、autostart、unknown。
- 所有活跃 fallible Tauri command 在 [commands.rs](../src-tauri/src/commands.rs) 边界映射该协议；内部原因只写原生日志，公开 fallback/details 不包含 credential、完整敏感 URL、服务端 body、本地路径或 SQLite 原始诊断。
- 新增 [commandErrors.ts](../src/utils/commandErrors.ts)，同步中英文 code 文案；未知 object、字符串、`Error` 和 malformed JSON rejection 安全降级 `unknown`。
- `useSnippets.load()` 返回权威数组，失败保留已有列表、记录错误并重新抛出；初始错误与空库分离并提供 Retry。
- delete/favorite/import/export/clipboard/settings load/save/sync 增加可见、安全、本地化反馈。mutation 成功后 reload 失败使用单独文案，不误报 mutation 失败；import 只 reload 一次。
- App 共享 reload/reconcile：干净选择采用权威片段，dirty editor 不被覆盖并显示非模态 stale status；保存 request/target/form race guard 保留。
- Tauri v2 窗口事件从不存在的 `Window.on(...)` 改为 `Window.listen(...)`，保留异步注册和 unlisten cleanup。
- 新增 Vitest/RTL 覆盖初始 load error vs empty、retry success、delete/favorite rejection state、unknown structured error fallback 和 clean/dirty reconciliation；Rust 覆盖序列化 shape、not-found 映射与敏感 source 不回显。

本检查点当时没有改变持久化 schema、Tauri capability/CSP、WebDAV merge 协议和设置 secret 暴露方式；这些安全缺口后来由 [凭据/设置安全与 CSP 检查点](#9-凭据设置安全与-csp-检查点) 解决。后台自动同步 event/provider 的整体重设计则由下一检查点完成。

本检查点完成后的文档链接检查当前覆盖 9 个 Markdown 文件、231 个相对链接；其余完整验证结果见本次交付报告。

## 8. 共享设置与同步完成协调检查点

本检查点在既有结构化错误与片段 reconciliation 基线上完成：

- [main.tsx](../src/main.tsx) 在根部挂载唯一 [SettingsProvider](../src/hooks/useSettings.ts)，并复用单次 boot settings promise 初始化设置、主题和语言；provider 本身不依赖 Theme/Language Context。
- 设置 provider 接受 injectable `SettingsApi`，通过 reload/save request ID 防止旧 reload 覆盖成功保存，并统一 settings、saving/syncing、history 和 sync status。
- [Settings.tsx](../src/components/Settings.tsx) 使用排除 `last_sync_at` 的 draft/baseline。clean draft 自动采用权威更新；dirty draft 保留并显示非模态外部更新状态。
- Settings X、backdrop 与 Escape 共用异步 Save/Discard/Cancel guard；保存失败保持面板和 draft，Save 在 clean/saving 时禁用。
- WebDAV/自动同步 draft 与 persisted baseline 不一致时禁用 Sync Now，并显示可访问、本地化的先保存说明；UI 明确首次 worker 尝试与后续配置间隔的差异。
- [App.tsx](../src/App.tsx) 统一处理 `toolbar | settings | tray | background` 来源和 `result | error | busy` 状态。toolbar/settings 使用 direct command；tray/background 使用 typed Rust event，避免 duplicate reload/Dialog。
- 所有成功来源只进入一次 `refreshAfterSync()`，并行刷新 snippets、共享 settings 和 history，再复用 dirty editor 保留/clean selection 更新逻辑。tray 可显示 modal，background 只更新 `aria-live`。
- 新增 [sync.rs](../src-tauri/src/sync.rs)：worker 接收 cloned `AppHandle` 并 emit typed event；busy 15 秒重试，其他失败 15/30/60 秒指数退避并封顶 15 分钟，成功后恢复配置间隔，有效配置保持下一 poll window 首次尝试语义。
- 新增 [tray.rs](../src-tauri/src/tray.rs)：集中托盘句柄、菜单、handler 和刷新。设置入口改为 `open-settings` event；Settings 保存或 tray toggle 后都刷新 autostart checked state。
- WebDAV secret 存储与 WebView 暴露、CSP、capability 在此检查点仍未改变；这些历史缺口已由 [第 9 节](#9-凭据设置安全与-csp-检查点) 后续检查点处理。WebDAV tombstone/ETag 协议的后续当前状态见 [已知限制](known-limitations.md#1-webdav-v2-协议与多设备并发)。

测试新增共享 provider/save race、dirty draft 外部更新、三个关闭入口、保存失败保留、save-before-sync、Settings sync coordinator、background no-modal refresh、Settings sync 单次主列表 refresh、scheduler/backoff/event serialization 和 tray checked-state pure logic。共享默认设置 fixture 位于 [settingsFixtures.ts](../src/test/settingsFixtures.ts)，避免从 test module 互相 import 导致重复收集。

本检查点最终通过：`npm run format:check`、`npm run lint`（退出成功，保留 3 个既有 warning）、`npm run typecheck`、`npm run test:run`（7 个文件、21 项）、`npm run build`、`npm run docs:check`（9 个 Markdown 文件、252 个相对链接）、`npm run versions:check`、Rust fmt/check/clippy/test（21 项）和 `git diff --check`。Vite 仍报告 window 模块动静态导入与 editor chunk 大小 warning；这些没有在本检查点扩大处理范围。真实 Tauri/WebDAV smoke 未执行，单元测试不作为其替代。

## 9. 凭据、设置安全与 CSP 检查点

本检查点在共享 `SettingsProvider`、结构化 `CommandError` 和 source-tagged 同步/托盘模块基础上完成：

- 新增 [credentials.rs](../src-tauri/src/credentials.rs) 的可注入 `CredentialStore`。生产实现通过 `keyring` 使用稳定 service `cn.rainss.snipvault.webdav` / account `default`，对应 Windows Credential Manager、macOS Keychain 与 Linux Secret Service；单元测试只用内存/失败 fake。
- [settings.rs](../src-tauri/src/settings.rs) 的当前 `Settings` serializer 不再包含 `webdav_password`。只读 `SettingsView` 只返回非敏感设置、`webdav_secret_configured` 和安全的凭据/设置恢复状态；写入使用非敏感 `SettingsInput` 与 tagged `Keep | Replace(value) | Clear`。
- [Settings.tsx](../src/components/Settings.tsx) 的 password 输入始终空白，用户只有键入时才产生 Replace，且可显式 Clear 或恢复 Keep；provider/draft/fixtures/snapshots 不包含 persisted secret。
- 启动时使用 legacy compatibility struct 读取旧 JSON。只有平台凭据写入成功后才原子重写无 secret JSON；失败保留旧文件，不加载/暴露 legacy secret，阻止凭据读取和持久自动同步，并要求 Replace/Clear。
- 损坏主 JSON 会隔离到唯一、非覆盖 `.corrupt` 文件，再尝试有效 `.bak` 或写入 defaults；UI 显示 backup-restored/defaults-loaded，并能通过受控命令打开数据目录。公开错误不返回绝对路径。
- `save_settings` 按 secret action → autostart → 无 secret settings 持久化排序，后续失败时恢复 autostart/旧凭据；补偿失败返回结构化 `recovery` 错误和 action-required 状态。`last_sync_at` 继续由后端所有，scheduler 只保存 `credential_revision`。
- [commands.rs](../src-tauri/src/commands.rs) 只提供固定仓库 URL 与后端派生 data/export enum 目录打开。前端和 Rust Shell dependency/plugin、`shell:allow-open`、广泛 regex、无用窗口与直接 WebView autostart 权限均已移除；导出 IPC 不再返回文件/目录绝对路径。
- [tauri.conf.json](../src-tauri/tauri.conf.json) 设置 `withGlobalTauri: false` 和限制性 CSP：本地 script、精确 IPC scheme、当前 image/font scheme，无 `unsafe-eval`；CodeMirror/React 所需的 `style-src 'unsafe-inline'` 是保留的最小现状例外。
- [index.html](../index.html) 的 inline boot CSS/script 移至 [boot.css](../src/boot.css) 和 [boot.ts](../src/boot.ts)，保留主题 anti-flash、splash 和 frontend-ready 时序。
- WebDAV URL 生产配置强制 HTTPS，只有 `localhost`、`127.0.0.1`、`::1` 允许 HTTP 测试；userinfo、query、fragment 继续拒绝。
- 新增/更新 Rust、RTL 和 [SecurityConfig.test.ts](../src/test/SecurityConfig.test.ts) 覆盖无 secret 序列化、迁移成功/失败、损坏恢复、Keep/Replace/Clear、事务回滚/补偿、URL 策略、CSP、global Tauri、capability 与 Shell 移除契约。

剩余边界已收窄到平台凭据库可用性/授权、补偿失败跨崩溃标记、inline style CSP 例外、loopback HTTP 测试和缺少真实跨平台 keyring/Tauri smoke，见 [已知限制](known-limitations.md#4-安全与隐私)。本检查点未改变 WebDAV tombstone/ETag 数据协议。

本检查点最终通过 frontend format/lint（0 error、3 个既有 warning）/typecheck/26 tests/build、Markdown links、三文件版本、Rust fmt/check/clippy/31 tests 和 `git diff --check`。Vite 仍报告 window API 动静态导入与 editor chunk 超过 500 kB。Windows `tauri:dev` 启动 smoke 已到达有标题且响应正常的真实窗口；未完成设置页各按钮、编辑器/minimap、受控 opener、跨平台凭据库与专用 WebDAV 目录的交互式全流程，因此这些 focused/unit/static checks 不作为 E2E 替代。

## 10. 数据与搜索可扩展性检查点

本检查点在不改变 WebDAV v1 远端协议、脱敏 `SettingsView`、结构化 `CommandError` 和受控目录打开边界的前提下完成：

- [db.rs](../src-tauri/src/db.rs) 的 `Snippet`、`SnippetSummary` 与 `SyncVersion` 使用 fallible decoder，严格检查必需类型、0/1 boolean、tags JSON string array、字段边界和 RFC 3339；诊断不包含完整正文。
- 导出改为 `snipvault.snippets` schema v1 envelope，包含 app version、exported time 和 snippets；导入兼容旧顶层数组，并在任何 write 前拒绝未知 format/schema 或 malformed metadata，保留大小/条目/整批校验和原子 merge。
- 文件分配使用 `OpenOptions::create_new(true)`，同秒冲突按 `-1`、`-2` 递增，永不覆盖；IPC 仍只返回 Downloads/fallback flag，不暴露绝对路径。
- SQLite 升为 schema v2。磁盘 v1 真正升级前使用 online backup 创建并验证唯一 preflight backup；严格 backfill 或 migration/open 失败会 rollback、保留失败副本并恢复/验证 v1。新库、v2 重开、未来版本拒绝不创建升级备份。
- v2 使用 FTS5 external-content table 和 insert/update/delete triggers，覆盖 CRUD、收藏、导入与 WebDAV merge。trigram 可用且查询至少 3 字符时使用字面量 MATCH，否则以转义 LIKE 保留子串/CJK；不做相关度排序。
- 新增 `SnippetSummary` cursor query、`get_snippet(id)` 与 distinct tags IPC。列表页限制正文 preview，按 `(updated_at, id)` 稳定排序；query/language/favorite/exact-tag 组合，通配符和反斜线按字面值。
- [useSnippets.ts](../src/hooks/useSnippets.ts) 使用 generation/query/cursor 与独立 append request guards、独立首屏与追加状态、100 条 Load More；并立即阻止同一 cursor 的重复在途追加。[App.tsx](../src/App.tsx) 在选择后懒加载详情，提供详情 loading/error/retry，mutation/sync/import 只做一次权威摘要刷新，并继续保护 dirty editor。
- Rust tests 覆盖 decoder、format compatibility、collision、v1/v2/future/recovery、FTS trigger、分页、literal wildcard 与 CJK；前端覆盖 stale response、pagination reset/成功追加/重复追加 guard/load-more、lazy detail/stale detail/retry 和 dirty reconciliation。另有 ignored 1k/10k 内存 benchmark，运行方式见 [开发指南](development.md#85-查询fts-与摘要详情协议)。

剩余搜索边界是无 trigram tokenizer 时为保持语义而退化为 LIKE，见 [已知限制](known-limitations.md#31-无-trigram-tokenizer-时搜索退化为-like)。

## 11. 前端可访问性、语言扩展与确认清理检查点

本检查点在现有 SettingsProvider、脱敏凭据、分页摘要/详情和测试基线上完成：

- 新增 [ModalSurface.tsx](../src/components/ModalSurface.tsx) 的共享模态栈。Settings 与 Promise Dialog 使用 `dialog`/`alertdialog`、`aria-modal`、稳定 label/description、确定性初始焦点、topmost Tab/Escape、背景 inert/ARIA 和关闭后焦点恢复；Settings Save/Discard/Cancel guard 保持不变。
- [SnippetList.tsx](../src/components/SnippetList.tsx) 改为语义 list/listitem；选择、收藏、删除成为同级 button，支持原生 Enter/Space，并用包含片段标题的中英文名称与 `aria-pressed` 表达状态。Toolbar、Titlebar、Settings、Dialog、Editor 和 context menu 的按钮统一显式 type/名称，异步状态增加 busy/live 反馈。
- 标签输入改为 combobox/listbox/option + active-descendant，支持 ArrowUp/ArrowDown、Enter 活动项优先、Escape 和协调鼠标点击，不再依赖 blur timeout。文本菜单只接管支持的文本编辑目标，其他区域保留原生菜单，并支持首项焦点、Arrow/Home/End/Escape 与恢复焦点。
- [LanguageProvider.tsx](../src/context/LanguageProvider.tsx) 从 `main.tsx` 启动副作用中分离，同步 HTML `lang="zh-CN" | "en"`。Canvas minimap/viewport 对辅助技术隐藏，CodeMirror 保持唯一 `EditorView.scrollDOM` 和键盘滚动路径。
- 新增 [languageExtensions.ts](../src/components/languageExtensions.ts) 的 exhaustively typed editor-only factory；metadata 保持无编辑器 import。HTML、Go、C#、Elixir 使用对应维护包；Ruby、Swift、Kotlin、Bash、Dockerfile、TOML、Lua、R、Scala 使用 legacy `StreamLanguage`。Stream mode 只提供语法着色，明确不是完整 Lezer parser；plaintext 为有意 fallback。
- CSS 增加共享 focus-visible、reduced-motion、双主题缺失变量和明确系统 UI/monospace stack；删除确认失效的 Shadow DOM/`.cm-editor-wrap` 规则，不触碰不确定动态 class 或未跟踪字体目录。
- 移除未使用的 `date-fns`、`@replit/codemirror-minimap`、Rust 直接 `tokio` 和未导出/未引用的 `models.rs`；保留并实际使用 HTML package，保留 `sharp`。未执行 `npm audit fix --force`。
- 新增 RTL/user-event/axe 与 parser coverage：嵌套模态/恢复、Settings guard、语义列表、Toolbar 名称、标签 combobox、文本菜单范围/导航、HTML lang 和语言分类/解析树。

剩余边界收窄为 StreamLanguage 非完整 parser、自研 minimap 增强操作无键盘等价路径、真实 CodeMirror/Tauri 桌面 E2E、真实第三方 WebDAV 与跨平台 keyring integration；见 [已知限制](known-limitations.md)。

## 12. WebDAV 可测试同步引擎基础检查点

本检查点在不改变远端 layout/version（仍为 manifest v1 + 每片段 JSON）、当时的 SQLite v2、凭据库、IPC `SyncResult` 字段和 source-tagged completion 协议的前提下完成：

- [webdav.rs](../src-tauri/src/webdav.rs) 收窄为稳定 facade、process lock、设置/凭据装配、成功 history/`last_sync_at` 提交；实现拆分到 [protocol.rs](../src-tauri/src/webdav/protocol.rs)、[transport.rs](../src-tauri/src/webdav/transport.rs)、[engine.rs](../src-tauri/src/webdav/engine.rs)、[store.rs](../src-tauri/src/webdav/store.rs) 与 [error.rs](../src-tauri/src/webdav/error.rs)。
- `RemoteTransport`、`SyncStore`、`Clock` 和 `RetryPolicy` 可注入。生产 store 的 snapshot/merge/history 每次独立获取并释放 SQLite mutex；HTTP 不持有全局 DB guard。
- Engine 使用纯 reconcile decision，最多 4 个本地稳定化 round、manifest PUT 后最多 2 个完整远端验证 round，以及 5 分钟总 deadline。每个验证 round 重读 manifest 并 GET 它引用的全部 payload，要求 payload 存在、JSON/字段有效、ID/`updated_at` 匹配；耗尽返回 retryable failure，不发布 success，也没有声称 v2 outbox。
- 同 ID、同 `updated_at` 不再只检查文件存在：engine GET 并验证远端 payload，以固定字段 `RemoteSnippet` 的确定性紧凑 JSON 字节全序选择赢家。远端赢家通过同步专用等时间替换 transaction 写回 SQLite/FTS，本地赢家重传 payload；v1 wire layout 没有增加 hash、revision 或 device 字段。
- GET/PUT 只对 transport failure 与 408/429/500/502/503/504 做最多 3 次幂等 retry；指数 backoff、`Retry-After`、retry sleep 的剩余 deadline 检查和 request/sync deadline 均有上限。认证、授权、validation 与普通 4xx 不盲重试。
- HEAD 保留 2xx exists、404 absent；认证/策略状态显式失败，405/501 和其他非认证/非策略歧义响应 fallback bounded GET。等时间 reconcile 探测存在后仍 GET/校验完整 payload，最终完整性验证也始终 GET，不依赖 HEAD 证明内容一致。
- `RemoteSnippet` 显式固定 v1 远端 payload 字段，并在 transport 边界与本地 `Snippet` 转换；下载及发布验证 payload 的 ID/`updated_at` 必须与 manifest 条目一致，防止本地模型演进或远端元数据分裂静默改变协议语义。
- `SyncError` / `SyncFailure` 将 busy 与 retryable 元数据保留到 `CommandError`，避免 authentication/configuration/validation 被字符串 fallback 误标为可重试，同时继续不暴露 credential、URL/path、响应体或片段正文。
- 新增 `tiny_http` dev dependency 和只绑定 `127.0.0.1:0`、专用 `/dedicated-test-root/snipvault/`、内存 store 的 integration suite，覆盖 MKCOL、manifest/payload GET/PUT、HEAD fallback、None/Basic/Digest/Bearer/Auto、missing/oversized/malformed 数据、manifest/payload 版本不一致、认证/validation 4xx 单次请求、部分失败、busy source event、多次 invocation 收敛、manifest PUT 后 payload 消失/损坏拒绝，以及两设备同时间不同内容的确定性收敛。没有访问真实 WebDAV、真实凭据库或用户数据库。

该检查点解决了“无 WebDAV mock server integration”“部分非标准 HEAD 状态不 fallback”“manifest map 相等但引用 payload 已消失/损坏仍可能成功”和“同时间不同正文永久分叉”的旧缺口。当时仍缺少 tombstone/outbox 与 future-v2 DB seam；这些本地基础后来由 [SQLite v3 检查点](#13-sqlite-v3-revisiontombstoneoutbox-foundation-检查点) 实现。production WebDAV 仍保留删除传播、跨设备 ETag/CAS 与验证后竞态、冲突 UI、真实第三方服务器兼容性与 Tauri desktop automation 边界，见 [已知限制](known-limitations.md)。

## 13. SQLite v3 revision/tombstone/outbox foundation 检查点

本检查点在不改变 production WebDAV 远端 layout/version（仍为 v1 manifest + 固定字段逐片段 JSON）的前提下，建立后续版本化同步所需的本地事务基础：

- [db.rs](../src-tauri/src/db.rs) 将 schema 顺序扩展为 `v0→v1→v2→v3`。所有既有磁盘 v0/v1/v2 来源在迁移链前创建并验证一个 `pre-v3` online backup；失败保留失败副本并恢复/验证原来源版本。新库、重复 v3 打开和 future version 拒绝不创建备份。
- v3 不改变 `snippets` 或既有 FTS external-content/triggers，新增稳定数据库 `sync_identity`、当前 `snippet_heads`（含 tombstone）、immutable `revision_outbox`、`sync_remote_state` 与 `sync_conflicts`，并扩展 sync history 的 deletion/conflict/protocol/generation 字段。
- v2 live rows 以固定 canonical payload SHA-256、ID 和 `updated_at` 生成确定性 `legacy-<sha256>` head；设备 marker 为 `legacy-v2`，不批量加入 outbox。数据库 device UUID 在重复初始化中保持稳定。
- create/update/favorite/delete/import winner 现在在单 transaction 中维护 live row、既有 FTS trigger、head 与 outbox；delete 移除 live row/FTS 并写 tombstone。update 在同一 transaction 比较前端 `base_revision_id`，过期返回结构化 `stale_revision`；[App.tsx](../src/App.tsx) 获取最新 detail/base revision，但保留 dirty form/draft。
- Revision canonical payload 使用固定字段 DTO 和 SHA-256，不直接依赖可演进 domain serialization。Pending outbox 上限为 10,000 条、64 MiB payload，总量外另有单 payload 上限；达到边界返回 `outbox_full` 并整体回滚。
- 新增 future-v2 DB seam：短锁一致 snapshot；完整验证后、无 echo 的 remote-plan transaction；按 snippet + 排序 revision pair 生成的确定性 conflict index/copy；remote state/history 与确切 revision-ID acknowledgement 原子 publish commit，保留 snapshot 后的 later edit。
- [protocol.rs](../src-tauri/src/webdav/protocol.rs) 继续通过显式 `RemoteSnippet` 隔离 v1 wire，测试证明 revision/device/tombstone 不泄漏；v1 manifest version 保持 1，现有 loopback integration suite 保留。
- Rust/前端测试覆盖 v1/v2 磁盘恢复、future/repeat、稳定 identity/确定性 backfill、mutation/FTS/tombstone rollback、stale base、pending count/byte 边界与 outbox immutability、strict decoding、validated plan rollback/no-echo、conflict retry、exact/repeated ack、later-edit preservation、错误 normalization 与 base revision IPC。

该检查点解决了“本地没有 revision head/tombstone/durable pending event”“update 无事务 CAS”“后续 v2 engine 无安全数据库提交 seam”的基础缺口，但**没有**上线 WebDAV v2。当前 v1 不消费或确认 outbox，不传播 tombstone，远端旧 payload 仍可恢复本地删除；pending 可能增长到安全上限。后续 activation 后的完整残余见 [已知限制](known-limitations.md#1-webdav-v2-协议与多设备并发)。

## 14. 当时的剩余限制（SQLite v3 foundation 检查点）

以下列表保留第 13 节完成时的历史判断；后续第 15 节已改变其中 WebDAV v1 条目：

- 当前 production WebDAV v1 不消费/确认 v3 outbox，也不传播 tombstone；高频 mutation 可能触发 pending safety limit。
- SQLite v3 conflict copy/index 和 remote publish seam 尚无 v2 transport、bootstrap 或冲突解决 UI；数据库 clone 会复制 stable device ID。
- 前端已有 focused accessibility integration tests，但仍缺少真实 CodeMirror、Tauri desktop event 和完整 E2E。
- 已有 loopback-only WebDAV mock integration；仍缺少真实跨平台凭据库测试、真实第三方 WebDAV compatibility 和 Tauri desktop smoke automation。
- 普通 CI full gate 只有 Linux，没有 Windows/macOS 普通 compile job。
- ESLint 为避免 Phase 0 业务重构关闭若干 compiler-oriented 规则，并允许现有 warning。
- Prettier 不是全仓库格式门禁。
- Markdown checker 不验证外部 URL 或 Mermaid 渲染。
- Phase 13 当时的 version checker 不覆盖 tag、Cargo.lock 和 Settings UI 版本；这些门禁已在第 16 节发布链路硬化中扩展。
- `npm install` 报告的依赖审计问题需单独评估；本阶段未执行破坏性 `npm audit fix --force`。

## 15. WebDAV protocol-v2 activation 检查点

本检查点在保留第 12、13 节历史陈述的前提下，把 production WebDAV facade 从 v1 engine 切换到 [engine_v2.rs](../src-tauri/src/webdav/engine_v2.rs)，并激活此前的 SQLite v3 revision/tombstone/outbox seams：

- 新远端布局为 `snipvault/protocol-v2.json`、同路径 v2 `snipvault/manifest.json` 与不可变 `snipvault/objects/<revision_uuid>.json`。Marker 只保存 protocol/vault identity；manifest 保存 generation 与 head revisions；objects 保存 live 或 tombstone revision、parent/device/hash/conflict metadata。
- Cutover 为单向 v1→v2。Fresh remote 创建 v2；legacy remote 先以 strong ETag 读取/校验 v1 manifest 及全部旧 payload，生成确定性 legacy root revisions，再条件替换 manifest 并创建 marker。旧逐片段 v1 文件不删除、不修改，激活后忽略；旧客户端不能继续共享该目录，也没有 downgrade/双写。
- Bootstrap matrix 对 marker 无 manifest、marker 配 v1、vault ID 不一致，以及本地已提交 v2 后远端整体消失/退回 v1执行 hard-stop；不猜测或 downgrade。V2 manifest 已发布但 marker 缺失按 interrupted activation 处理：要求 strong ETag，如已有本地 vault identity 则要求一致，条件发布下一代后补建 marker。
- [transport.rs](../src-tauri/src/webdav/transport.rs) 要求 manifest GET 的 strong ETag，使用 `If-Match` 和 `If-None-Match: *` conditional PUT。412 触发 bounded CAS re-observe；缺失/weak ETag 或服务器拒绝条件写会失败，不回退无条件覆盖。
- 发布顺序为 immutable objects → conditional manifest → conditional marker → exact manifest bytes/hash/strong ETag 与 marker reread → local remote-state/history/exact revision-ID commit。最多 4 个 CAS rounds，整个 invocation 受 5 分钟 deadline 限制。
- Reconcile 先比较 ancestry；无祖先关系时保留已发布的 remote original 作为 head。Remote winner 覆盖 live local branch 时通过 v3/v4 seam 生成幂等 conflict copy/index；落败 tombstone 没有 live copy，但其 immutable revision 仍上传并精确确认。没有 semantic merge 或完整冲突解决 UI。
- Tombstone 进入正常 ancestry/publish path并跨设备传播。Remote immutable revisions 和 tombstones 无限期保留，当前无 GC/compaction。Schema v4 增加本地 durable `revision_objects` archive；DB outbox 只确认精确发布且仍 pending 的 revision IDs，确认时只删除 outbox row，不删除 durable object，保留 snapshot 后 later edits 和已确认 ancestry。
- 即时 `SyncResult` 契约使用 `protocol_version` 与 `manifest_generation`，并携带 uploaded/downloaded/deleted/conflict/pending/total；同步 history 使用 `protocol_version` 与 `generation`，保存 total/uploaded/downloaded/deleted/conflict（不保存 pending）。

验证范围中，production-v2 现有自动化包含 protocol/transport/engine/database 的 synthetic/pure unit/fake tests，覆盖 canonical v2 DTO/hash/ancestry、strong ETag validator、pending ancestry、exact acknowledgement/later edit 和 tombstone/conflict reconcile；`tiny_http` loopback 只绑定 loopback、使用专用路径和 fake/in-memory state，除 retained v1 cases 外，已覆盖 v2 精确 marker/object/manifest wire、条件请求头、parsed metadata、不可变对象碰撞恢复与 fresh engine bootstrap/exact acknowledgement。**没有执行真实第三方 WebDAV 服务测试，也没有执行 production Tauri desktop smoke。**当前残余集中在跨系统非原子事务、server compatibility hard requirement、无限 object retention、冲突 UI、database clone identity、请求/带宽成本及缺少更宽 cutover/hard-stop/CAS/crash/concurrency engine-loopback、实服与桌面验证，见 [已知限制](known-limitations.md#1-webdav-v2-协议与多设备并发)。

## 16. 发布链路硬化检查点

本检查点处理发布、图标和供应链门禁中可在仓库内完成的部分；签名、公证和 updater 仍保持外部门禁，不伪造启用状态：

- 图标链路统一为 [assets/app-icon.png](../assets/app-icon.png) canonical source；`npm run icons` 调用 Tauri icon generator 生成 [src-tauri/icons/](../src-tauri/icons/)，删除旧 `gen-icon.cjs`、`scripts/generate-icons.cjs`、`scripts/src-tauri/`、`public/icon-32.png`、`public/icon-128.png` 和 `src-tauri/icons/generate-icons.cjs`。标题栏通过 Vite asset import 使用同一 source。
- 新增 `npm run icons:check`，校验 canonical/source generated PNG 尺寸、PNG/ICO/ICNS magic、`tauri.conf.json` 图标引用和旧重复图标链路是否残留，避免 `.icns` 再退化为 PNG bytes。
- `npm run versions:check` 扩展到 `package.json`、[Cargo.toml](../src-tauri/Cargo.toml)、[Cargo.lock](../src-tauri/Cargo.lock)、[tauri.conf.json](../src-tauri/tauri.conf.json) 和 [vite.config.ts](../vite.config.ts) 注入的 Settings/UI 版本；release workflow 设置 `SNIPVAULT_RELEASE_TAG` 时还校验 tag 与内部版本一致。Settings About/footer 不再硬编码版本，而是使用 `import.meta.env.VITE_APP_VERSION`。
- 普通 [ci.yml](../.github/workflows/ci.yml) 增加 icon check。手动 [release.yml](../.github/workflows/release.yml) 改为 dry-run build/校验，不创建 GitHub Release；tag 发布先 validate tag/version/icons。
- Release workflow 现在要求 Windows、macOS、Linux 三个平台全部成功后才进入 release job；Windows 校验 MSI/NSIS/portable，Linux 校验 DEB/AppImage，macOS 构建 `universal-apple-darwin` DMG 并用 `lipo -info` 校验 `x86_64` + `arm64`。
- Release job 下载完整 artifacts，拒绝缺失平台产物和 `.app` asset，生成 `SHA256SUMS`；tag 发布时调用 GitHub artifact attestations，然后只发布 bundle 文件和 checksum。

剩余风险：没有安装 Tauri updater plugin、没有 updater endpoint/public key/latest.json 或 UI；没有 Windows Authenticode、macOS Developer ID signing/notarization 或真实签名产物验证；release workflow 的 macOS/Linux/attestation 分支尚未在 GitHub Actions 中实跑。对应限制保留在 [known-limitations.md](known-limitations.md#8-发布与平台)。

本检查点收尾验证通过：`npm run icons:check`、`npm run versions:check`、`npm run docs:check`、`npm run format:check`、`npm run lint`（0 error、1 个既有 fast-refresh warning）、`npm run typecheck`、`npm run build`（保留 Tauri window 动静态导入与 editor chunk size warning）、`cargo fmt --manifest-path src-tauri/Cargo.toml --check`、`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` 和 `git diff --check`（仅 Git 在 Windows 上提示 LF/CRLF 转换 warning）。本检查点没有新增或修改单元测试相关代码；收尾验证未运行 frontend/Rust unit test suite，未执行真实 GitHub release workflow、真实第三方 WebDAV 服务测试或 production Tauri desktop smoke。
