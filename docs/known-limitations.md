# SnipVault 已知限制与技术债

> 本文记录 v2.5.1 中仍可验证的限制。已解决问题及方案见 [第一轮问题修复记录](remediation-2026-07-31.md) 与 [第二轮修复记录](remediation-round-2.md)。本文不是修复承诺。

## 优先级概览

| 优先级 | 主要未解决问题 |
|---|---|
| P0 同步协议 | WebDAV v2 的 HTTP/SQLite 操作仍不是跨系统原子事务；strong ETag/conditional PUT 是硬服务器要求；immutable revisions/tombstones 没有 GC；冲突没有完整解决 UI |
| P1 安全与隐私 | 平台凭据库可用性/授权与补偿恢复仍有平台和崩溃边界；loopback 测试例外允许 HTTP；CSP inline style 例外仍存在 |
| P2 产品与工程 | WebDAV v2 已有 transport wire 与 fresh engine bootstrap 的 dedicated loopback coverage，但更宽的 cutover/hard-stop/CAS/crash/concurrency engine-loopback 矩阵及真实服务 compatibility、production Tauri smoke 仍缺；版本历史比较不是语义/三方 diff，tombstone 不可恢复且没有 revision GC；本地快照只在运行中的应用内按有限策略执行；搜索 fallback 在无 trigram 时退化为 LIKE；StreamLanguage 语言不是完整 Lezer parser；还缺桌面端 E2E 与真实跨平台凭据库测试；普通 CI 仅有 Linux full gate |
| P3 发布 | 无应用内更新、无完整 Windows/macOS 签名/公证与真实签名产物验证；release dry-run/tag workflow 仍未在 GitHub Actions 中实跑验证 |

## 1. WebDAV v2 协议与多设备并发

### 1.1 同步不是跨系统事务

一次 v2 同步包含多个 HTTP 请求和本地 SQLite transaction，不能组成单个原子提交。当前按 immutable revision object → conditional manifest → marker → exact reread verification → local remote-state/history/exact acknowledgement 排序，并由 durable outbox 和幂等重试收敛；但失败时远端可能已经存在尚未被 manifest 引用或尚未在本地确认的对象，本地也可能已经应用 validated remote plan。

影响：本轮会明确失败而不是伪装成功，后续同步通常可以收敛；它不提供跨 WebDAV 与 SQLite 的 ACID rollback。不要把 WebDAV 作为唯一备份。

### 1.2 Strong ETag 与 conditional PUT 是硬兼容要求

Production v2 要求 manifest GET 返回 strong ETag，并要求服务器正确实现 `If-Match` 和 `If-None-Match: *` 的 conditional PUT。缺失/weak/malformed ETag、HTTP 428 或忽略条件头都不会降级为无条件覆盖。

影响：一些 WebDAV 服务、反向代理、对象网关或非标准 ETag 实现不兼容。该 hard-stop 是跨设备 CAS 安全边界，不是可通过“最后写入者获胜”规避的小问题。当前没有经过真实 Nextcloud、ownCloud、Apache mod_dav 或其他服务的兼容矩阵；loopback HTTP 已覆盖 v2 transport wire 与 fresh engine bootstrap/exact acknowledgement，但不能证明第三方服务兼容。

### 1.3 Remote revisions 与 tombstones 无限期保留

`snipvault/objects/<revision_uuid>.json` 是不可变 live/tombstone 对象。manifest 只指向当前 heads，但 ancestry 验证会依赖 parent objects；当前没有 reachability-based GC、retention window、compaction manifest 或服务端清理命令。v1 cutover 后的旧逐片段文件也有意保持不动。

影响：长期编辑、冲突和删除会持续增加远端对象数与空间；用户不应手工删除 objects 或 tombstones，否则可能破坏 ancestry 并导致 hard-stop。任何未来 GC 必须是版本化协议能力，覆盖并发 reader、离线设备和失败恢复。

### 1.4 Ancestry 遍历会增加请求与带宽

Ready v2 同步会从每个 manifest head 沿 parent links GET immutable objects 并校验完整 chain。大型历史库、深链或高延迟服务会增加请求数、流量和总耗时，最终受五分钟 invocation deadline 限制。

影响：合法但很深的历史可能超时；当前没有 batch multi-get、packfile、增量 ancestry cache 或 compaction。deadline/CAS rounds 有界只保证停机，不是性能 SLA。

### 1.5 Conflict copy 不是完整冲突解决流程

并发分支先按 ancestry；无祖先关系时，已经发布的远端 original 确定性胜出。远端覆盖 live 本地分支时，SQLite 会创建幂等 conflict copy/index；没有属性级或正文 semantic merge。

剩余边界：应用没有冲突列表、用于冲突解决的双方/祖先 diff、选择赢家、合并、标记已解决或归档 UI；副本标题目前使用固定英文 suffix。已实现的同片段历史并排比较不等于冲突解决流程或 semantic merge。若本地 tombstone 是落败分支，没有正文可生成 conflict copy。确定性排序保证收敛，不表达用户业务优先级。

### 1.6 数据库复制会复制 device ID

`sync_identity` 是数据库级稳定 singleton。直接复制整个 `snippets.db` 到另一台设备会复制相同 device ID，v2 revision metadata 因而无法仅凭该字段区分克隆来源。当前没有“重新生成此设备身份”命令或 clone detection；恢复/克隆数据库后在两处继续独立编辑前，应先保留备份并理解该审计边界。

### 1.7 Bootstrap 与旧客户端兼容边界

v1→v2 activation 是单向操作。Marker 配 v1/缺失 manifest、vault ID 不一致、或某个本地已提交 v2 的 remote 整体消失/退回 v1时会 hard-stop；应用不会猜测、downgrade 或自动重建。V2 manifest 已写但 marker 缺失被明确视为 interrupted activation：要求 strong ETag，如已有本地 vault identity 则要求一致，engine 会条件发布下一代并补建 marker。Legacy v1 payload files 在 activation 后保留并忽略，旧版客户端如果继续写同一目录会与 v2 ownership 冲突。

影响：激活前需要备份远端目录，并确保所有共享该目录的客户端都升级。当前没有 rollback/downgrade 工具，也没有从损坏/部分人工修改的 bootstrap 状态自动恢复的向导。

## 2. 自动同步和托盘运行时反馈

### 2.1 自动同步轮询与退避精度

worker 每 15 秒观察一次设置。有效配置首次出现时会在该次观察中尝试，成功后按配置间隔运行；busy 约 15 秒重试，其他失败指数退避并封顶 15 分钟。

影响：这些时间都受 15 秒 poll 粒度、同步本身耗时和线程调度影响，不是精确 wall-clock 定时；应用进程退出期间也不会补跑。当前没有持久化 scheduler/backoff 状态或 OS 后台任务。

### 2.2 后台反馈保持非模态，但结果可持久追溯

后台成功、busy 和失败都会 emit typed `sync-complete`；成功时已打开前端会刷新片段、共享设置、同步历史和 notification inbox，dirty editor 保留。为避免后台流程打断用户，background 反馈只进入 `aria-live` 区域，不弹 Dialog；但每个同步终态会先写入去标识化的本地通知中心，用户可通过工具栏铃铛稍后检查。

影响：通知中心不保存服务器地址、凭据、片段内容、revision 标识、路径、远端响应或自由文本消息；它只保存固定来源/状态/类别、稳定错误码、可重试性、聚合计数、protocol 元数据与 read/dismiss 状态。inbox 与最近 20 条成功技术同步历史分离，只保留最新 200 条，默认显示 50 条、最多 100 条，且作为完整 SQLite vault snapshot 的一部分恢复；应用退出期间不会补跑同步或产生后台记录。

## 3. 搜索、语言与规模边界

### 3.1 无 trigram tokenizer 时搜索退化为 LIKE

主列表现已通过后端有界摘要页和详情懒加载避免把全部正文送入 WebView。SQLite bundled 构建优先使用 FTS5 trigram，至少 3 字符查询走字面量化 MATCH；如果运行时 SQLite 不支持 trigram，v2 会安全建立 `unicode61` FTS，但为了保持现有任意子串和 CJK 语义，当前所有非空查询改走转义 LIKE。

影响：fallback 模式保持正确性和字面量 wildcard 安全，但正文搜索仍可能扫描较多行；`updated` 固定按 `updated_at DESC, id DESC`，而 `recent` 只在本机按 usage 时间稳定排序。两者均不实现相关度排序，开发 benchmark 是可重复 checkpoint，不是 SLA。

### 3.2 StreamLanguage 语言不是完整 Lezer parser

可选语言现在都有明确扩展策略：Go、HTML、C# 和 Elixir 使用对应 parser-backed 包；Ruby、Swift、Kotlin、Bash、Dockerfile、TOML、Lua、R、Scala 使用 `@codemirror/legacy-modes` 的 `StreamLanguage`；plaintext 有意保持空扩展。

Stream mode 能提供对应语言的词法语法着色，但不构建完整 Lezer 语法树。因此这些语言不能保证具备与 parser-backed 语言相同的结构折叠、语法树导航、结构选择或未来语言服务能力。Canvas codeglance 复用该语言扩展产生的 token 范围和编辑器的实际高亮颜色，所以它与编辑区匹配 StreamLanguage 的词法着色，但不会由此获得完整 Lezer 结构。语言元数据与编辑器扩展已分层，并由 exhaustively typed test 防止新 ID 静默落空。

## 4. 安全与隐私

### 4.1 平台凭据库可用性与授权依赖

持久 WebDAV secret 已从当前 `settings.json` 和 `SettingsView` 移除，并通过 `keyring` 使用 Windows Credential Manager、macOS Keychain 或 Linux Secret Service。该安全边界仍依赖操作系统后端存在、会话已解锁、用户授权和 entry 唯一性。

影响：凭据库 unavailable、denied、invalid 或 ambiguous 时，设置页会显示安全状态并阻止凭据相关同步；SnipVault 不会退回明文。Linux headless/minimal desktop、被锁定 keychain 或企业策略可能要求用户先修复系统凭据服务，再 Replace/Clear。

当前 Rust 单元测试只使用内存/失败 fake，避免访问开发机凭据库；普通 CI 没有跨 Windows/macOS/Linux 的真实 keyring integration test。

### 4.2 凭据库和补偿恢复仍依赖平台与进程状态

旧明文迁移只有在安全凭据写入成功后才净化 JSON；失败会保留遗留文件但不加载/暴露 secret，并要求 Replace/Clear。credential → autostart → settings 事务也会回滚已完成步骤。

剩余边界：如果后续设置持久化失败且凭据或 autostart 补偿本身也失败，`compensation_required` 当前可靠地保存在运行中内存并阻止 Keep/同步，但无法保证在设置文件本身不可写或进程随即崩溃时跨重启持久记录。用户应立即在当前设置页 Replace 或 Clear，并核对 OS 自启动状态；未来可设计不含 secret 的独立、原子 recovery marker。

### 4.3 CSP 仍允许 inline style

CSP 已启用本地 script、精确 IPC scheme，并禁用 `unsafe-eval`、远端脚本、object/frame/base/form；启动 inline script/style 也已移入 Vite 资产。但 `style-src 'unsafe-inline'` 暂时保留，因为 CodeMirror 和现有 React 组件会生成运行时 inline style。

影响：这比允许 inline script 的风险低，但仍弱于 nonce/hash 或完全无 inline style 的策略。新增代码不应扩大该例外。

### 4.4 Loopback WebDAV 测试允许 HTTP

生产地址强制 HTTPS，只有 host 精确为 `localhost`、`127.0.0.1` 或 `::1` 时允许 HTTP，以支持本机 mock/test server。userinfo、query 和 fragment 仍被拒绝。

影响：本机其他进程或 hosts/代理环境仍需纳入测试威胁模型；不要把 loopback HTTP 当成远端部署方案，也不要把测试服务器绑定到非 loopback 地址。

### 4.5 设置恢复和受控目录打开仍需人工判断

损坏 `settings.json` 会被隔离为唯一 `.corrupt` 同级文件，并尝试 `.bak` 或 defaults；UI 显示恢复结果并能通过后端受控命令打开数据目录。应用不会自动合并损坏 JSON，也不会把绝对路径放入错误。

影响：用户仍需人工判断/备份被隔离文件；默认恢复可能重置非敏感偏好和 WebDAV URL/用户名。打开目录依赖平台 opener 成功，失败只返回安全通用错误。

## 5. 持久化与数据格式

### 5.1 数据库升级备份是保留的恢复工件

任何既有磁盘 v0/v1/v2/v3/v4/v5/v6 数据库升级到当前 schema v7 前都会创建并验证唯一 `pre-v7` online backup；迁移链任一步失败时恢复并重新验证原来源版本，同时保留失败数据库副本供人工诊断。成功升级后 preflight backup 也有意保留，不会由应用自动轮转或删除。

影响：多次历史升级可能留下额外同级备份文件，需要用户在确认新版本数据正常且另有备份后人工归档。公开 IPC/错误不会返回这些绝对路径。

### 5.2 没有通用 migrations 目录

当前已有 temp-DB 覆盖的 v0→v1→v2→v3→v4→v5→v6→v7 顺序路径、未来版本拒绝、重复初始化、transaction rollback，以及历史版本 preflight restore，但迁移代码仍集中在 [db.rs](../src-tauri/src/db.rs)，不是可声明式扩展的 migration framework。v5 的 `snippet_usage` 是有意本地化的 usage 元数据：它不进入 JSON、revision、outbox 或 WebDAV，既有片段升级后没有虚构 usage。v6 新建脱敏 `sync_notifications` 收件箱；v7 新建 `local_snapshots` catalog，实际 snapshot 文件由受控目录中的 [snapshots.rs](../src-tauri/src/snapshots.rs) 管理。每个未来 schema 版本仍必须新增专门步骤、全部历史 fixture、来源版本 backup/restore 和 import/WebDAV compatibility 测试。

### 5.3 版本历史已产品化，但没有合并或保留治理

Schema v4 的 `revision_objects` 会在 exact acknowledgement 删除 `revision_outbox` 后继续保留本地/远端/冲突 revision payload，使 WebDAV v2 后续同步仍可离线读取当前 head 与 pending ancestry。已保存片段现在可以分页浏览经过验证的 immutable metadata、查看 live/tombstone 内容状态、并排比较同一片段的两个 revision，并把历史 live 内容作为以当前 head 为 parent 的新 local descendant 恢复；恢复不重写历史并照常进入 outbox。tombstone 可检视/比较但不能恢复。

剩余边界：比较是本地有界的两路逐行 diff，不是 word-level/semantic diff、三方合并或冲突解决 UI。版本历史在独立原生工作区中使用紧凑时间线；宽度至少 1200px 时使用弹性双栏，历史窗口最小宽度的 1000–1199px 区间则以已加载的“比较基线 / 所选版本”单 pane 切换避免压缩非换行源代码。两种呈现都不会强制外层横向滚动，代码不会自动换行；只有真实超出可见 source pane 的长行会在该 pane 内横向滚动，详细双栏对齐才同步纵向位置。为防止大正文或高度差异版本阻塞 WebView，line-diff 对总字符/行数、矩阵/渲染行数和短计算时间设有上限；超过上限会明确退回为带行号、语法高亮且不自动换行的完整并排源代码。没有 revision retention window、compaction 或 reachability-based GC，也不提供面向用户的完整历史审计承诺。长期编辑、冲突和删除会继续增加本地与远端 immutable object 数量；未来清理必须设计独立 retention/compaction、迁移和并发安全协议。

### 5.4 本地快照只提供有限的设备内恢复

本地快照是经过 integrity、schema、device identity、live-count、size 和 SHA-256 验证的完整 SQLite checkpoint，不是 JSON export、云备份、加密 archive 或 WebDAV artifact。策略仅支持应用运行期间的 daily / weekly 检查和 7 / 30 / 90 保留值；worker 每 15 分钟轮询，创建失败的重试最多延后一小时，因此不提供精确时刻调度、离线补跑、跨设备同步或用户自定义保留期。

完整恢复会先建立 emergency checkpoint，并通过 SQLite backup 写入活动连接而非替换打开的文件；`settings.json` 和 OS credentials 保持不变。由于快照包含整个数据库，也会恢复 `sync_identity`、本机 usage、通知收件箱、同步状态和 catalog；恢复旧 checkpoint 可能回退这些本机状态。恢复后 automatic sync 会暂停，直到工具栏、设置或托盘的一次成功手动同步清除确认锁。当前没有快照加密、云归档、任意文件选择、已删除 snapshot 的恢复、跨平台 smoke 或新的自动化覆盖。

## 6. 前端交互与可访问性

### 6.1 界面配色仅支持经审查的精选 preset

当前主题系统将深浅模式与界面配色分离：可持久化的 `accent_preset` 只接受 `sky`、`violet`、`emerald`、`amber`、`rose`、`white`（界面名称“简约白”）。每个精选值在 dark/light 下均提供受控的完整 UI palette，覆盖背景、surface、文字层级、边框、标题栏、弹窗、控件、编辑器 chrome 和 codeglance；简约白会在各自深浅模式下复现初始版本的中性界面。语法 token、语言标签色和状态语义保持稳定。

影响：已支持完整的精选 surface skin，但仍不支持任意 HEX、CSS 字符串、用户主题导入/导出或 syntax palette 编辑器。这一限制防止未校验 CSS 注入，并避免无法保证深浅模式、填充按钮文字、焦点、选区和可访问性对比度的任意配色。若未来支持自定义颜色，必须引入版本化 schema、严格颜色解析、可访问性/OKLCH 派生、重置路径和迁移策略，不能直接把用户输入写入 style/CSS variable。

### 6.2 Vite 开发服务器中断仍需单独诊断

编辑器仍按需从 Vite dev server 懒加载；若 `http://localhost:1420` 已停止监听，首次打开片段或新建草稿的模块请求会被拒绝。`SnippetEditorLoadBoundary` 会将 [LazySnippetEditor.tsx](../src/components/LazySnippetEditor.tsx) 的 rejected import 或编辑器 render error 限制在右侧 pane，保留侧栏、工具栏和 App 持有的草稿，并允许用户在恢复 server 后手动 Retry；它不会自动重试、重启 Vite、轮询端口或确定进程退出的根因。

影响：运行中的 Tauri window 可能在 Vite 子进程结束后继续存在，生产构建不受 localhost dev server 依赖。开发者必须从 Vite 终端的 stdout/stderr 与 exit code 区分 port-owner、Node/Vite/esbuild、资源或 transform 问题；浏览器的 `ERR_CONNECTION_REFUSED` 只能证明当时 server 不可用。隔离诊断必须避免真实用户数据、凭据和 WebDAV 配置，并且 native 开发时只能让 `npm run tauri:dev` 拥有 1420 端口。

### 6.3 高频效率层仍需真实桌面验证

命令面板、原生全局/托盘快速捕获、批量选择和本机“最近使用”排序已接入现有业务链路，但本阶段遵循“不新增测试代码”的约束，没有为这些路径添加新的 focused unit、RTL 或 Tauri 自动化用例。既有测试与静态 gate 不能替代真实全局快捷键注册、系统剪贴板、托盘菜单、CodeMirror 焦点和窗口隐藏/最小化时序。

影响：不同桌面环境可能已占用 `Ctrl/Cmd+Shift+V`、禁止应用注册全局快捷键，或限制剪贴板访问；应用会保留托盘捕获入口并以非模态失败反馈，不应承诺全局快捷键一定可用。全局捕获只在显式快捷键/托盘动作中读取文本，不记录或向 WebView 发送正文；当前 completion fallback 只保留 listener 就绪前的最新一条结果，因此它不是隐藏窗口期间任意次数捕获的可靠队列。需要在隔离数据目录、无真实凭据/同步且使用非敏感剪贴板文本的实际 Tauri 窗口中完成 smoke，确认事件/日志脱敏、排序刷新、批量 dirty guard 和焦点恢复。

### 6.4 本地 usage 不等于跨设备近期语义

“最近使用”只由本机成功打开详情、复制已保存正文或快速捕获写入，不改变 `updated_at`，也不会通过 revision/outbox/WebDAV/JSON 传播。未使用项稳定排在已有 usage 项之后；它不是全文搜索相关度、跨设备最近打开记录或可审计的使用历史。

影响：不同设备、导入/导出恢复后和升级前既有片段会看到不同排序；删除后本地 usage 会一起清除。当前没有 usage 浏览、清空/编辑、跨设备合并或用户可配置的 usage retention 功能。

Canvas codeglance、viewport 和宽度分隔器是装饰性/增强型视觉导航，已从辅助技术树隐藏；CodeMirror 本体保持可键盘滚动和编辑，因此核心内容不依赖 minimap。但 minimap 的点击跳转、viewport 拖动和宽度拖动没有等价的键盘操作。当前把这些能力视为非必要增强；若未来把 minimap 操作设为核心功能，需提供明确键盘控制。

## 7. 工程保障

### 7.1 测试覆盖仍有限

当前数据库、设置/凭据、command transaction、scheduler/event/tray pure logic、前端协调与 accessibility 均有 focused tests。WebDAV v2 的 protocol/transport/engine/database synthetic/unit tests 覆盖 canonical DTO/hash/ancestry、strong ETag validator、pending ancestry、exact acknowledgement/later edit 和 tombstone/conflict reconcile；`tiny_http` loopback 另外覆盖 v2 精确 marker/object/manifest wire、条件请求头、parsed metadata、不可变碰撞恢复与 fresh engine bootstrap/exact acknowledgement，同时保留 v1 transport/engine cases。仍缺少：

- 完整 App、真实 CodeMirror、Tauri event 并发、revision-history 跨窗口目标/restore handoff 与桌面焦点管理的广泛 integration/E2E 测试。
- Tauri IPC/真实窗口/托盘/自启动 smoke automation；本次 v2 activation 和独立版本历史窗口都没有 production Tauri desktop smoke。
- 安装、升级、真实历史迁移、跨平台真实凭据库测试。
- WebDAV v2 legacy cutover、全部 ambiguous bootstrap hard-stop、412/428/CAS exhaustion、crash before/after CAS、concurrent local edit、missing/cyclic/aggregate-bounded ancestry 等完整 engine-level loopback 矩阵；现有 v2 loopback 只覆盖 transport wire 与 fresh bootstrap/exact acknowledgement。
- Nextcloud、ownCloud、Apache mod_dav 或其他真实第三方 WebDAV 测试/兼容矩阵；loopback mock 与 v2 unit tests 都不是实服兼容性证明。

### 7.2 普通 CI 仅有 Linux full gate

普通 push/PR 已通过 `.github/workflows/ci.yml` 自动运行 frontend format/lint/typecheck/test/build、Rust fmt/check/clippy/test、Markdown links、图标完整性，以及 package/Cargo/Tauri/Cargo.lock/Vite UI 版本一致性检查。当前 full gate 只运行于 Ubuntu；没有独立 Windows/macOS compile job，也没有在普通 CI 启动真实 Tauri 窗口。

### 7.3 Lint 和 format 门禁仍是保守基线

ESLint 已覆盖 TypeScript、React 和 Hooks，但为了避免本轮重写既有业务代码，关闭了几项会直接要求结构性重构的 compiler-oriented 规则，并允许现有 warning。Prettier 只检查维护中的工具、测试、配置与 `ci.yml`，没有将全部既有 `src/` 和 Markdown 纳入格式门禁。

### 7.4 内部错误分类剩余边界

Tauri command 边界统一为 `CommandError { code, message, retryable, details? }`，前端按稳定 code 本地化并对 malformed rejection 安全回退。WebDAV 现在用内部 `SyncError` / facade `SyncFailure` 将 busy 与 retryable 元数据保留到 IPC/event adapter，因此 authentication、authorization、configuration、validation 不再因字符串 fallback 被误标为可重试，且敏感 source 不进入公开错误。

剩余边界：公开 WebDAV 非 busy 失败仍统一使用 `network` code，不能在前端区分认证、远端格式、deadline 或本地成功标记持久化失败；db/settings/import 的内部错误仍有 `rusqlite::Error` / `String` 包装，部分 import validation 使用 SQLite `InvalidParameterName`。后续可扩展内部 domain categories 或稳定公开 code，但必须保持脱敏和兼容策略。

### 7.5 npm 依赖审计仍有告警

在当前 lockfile 上执行 `npm audit` 报告 4 项 advisory（1 low、3 high），涉及 transitive `@babel/core` / `postcss` 和 direct dev dependency `sharp` / `vite`。其中 Sharp 的建议版本是 semver-major 升级，其他修复也应经过 Node/Vite/Tauri 构建兼容验证；本轮没有执行 `npm audit fix --force`。

这些包当前用于构建、开发或图标工具，不表示已观察到 SnipVault 运行时利用，但在后续依赖维护中仍需升级并重新运行完整 gate。

## 8. 发布与平台

### 8.1 Release workflow 尚未实跑验证

Release workflow 已配置 Windows MSI/NSIS/portable、Linux DEB/AppImage、macOS `universal-apple-darwin` DMG、完整 artifact set 检查、`SHA256SUMS` 和 GitHub artifact attestations。手动 dispatch 现在只执行 dry-run，不创建 Release；tag 发布要求 tag 与内部版本一致。

剩余边界：这些 release 路径尚未在 GitHub Actions 的真实 tag 或 dry-run 中完成一次端到端验证；本地 Windows 环境也没有执行 macOS/Linux 打包或 attestation。若 workflow 依赖的 Tauri bundle 路径、GitHub attestation 权限或平台工具链变化，仍可能在远端失败。

### 8.2 没有应用内更新

没有 Tauri updater plugin、endpoint/public key、updater artifacts、`latest.json` 或前端更新 UI。用户只能从 GitHub Releases 手工下载。

### 8.3 签名、公证与 updater 外部门禁仍未完成

当前 release workflow 不宣称 Windows Authenticode、macOS Developer ID signing/notarization 或 Tauri updater 已启用。完整启用仍需要维护者提供真实证书、Apple team/notarization 凭据、Windows signing provider、updater key/secret、签名 artifact 验证与失败路径测试。

### 8.4 图标和版本门禁的剩余边界

图标链路已统一到 [assets/app-icon.png](../assets/app-icon.png)，`npm run icons` 生成 [src-tauri/icons/](../src-tauri/icons/)，`npm run icons:check` 校验 PNG/ICO/ICNS magic、关键尺寸、Tauri 配置引用和旧重复生成器/输出。版本门禁已覆盖 `package.json`、`Cargo.toml`、Cargo.lock、`tauri.conf.json`、Vite 注入的 Settings/UI 版本，并在 release workflow 中校验 tag。

剩余边界：图标生成依赖本地 Tauri CLI 和 `sharp`；`npm run icons:check` 验证格式/尺寸，不进行人工视觉评审。

## 9. 已完成的清理

本检查点只删除经静态引用确认的遗留项：未导出/未引用的 `models.rs`、未直接使用的 Rust `tokio` 依赖、未使用的 `date-fns` 和 `@replit/codemirror-minimap`。`@codemirror/lang-html` 现用于真实 HTML parser，`sharp` 继续保留用于仓库工具链。

CSS 已补全双主题 `--border-subtle`/`--accent-bg`，改用明确的系统 UI/monospace 字体栈，并移除确认失效的 Shadow DOM/`.cm-editor-wrap` 表述与规则；没有删除不确定的动态 class 或本地未跟踪字体目录。

## 10. 已解决问题索引

WebDAV v2 activation 已解决下列旧 P0 缺口；剩余约束见第 1 节：

- Production 同步已消费 v3/v4 revision/tombstone/outbox/revision-object seams，可发布 tombstone 删除并仅确认精确 pending revision IDs。
- 跨设备 manifest 发布现强制使用 strong ETag 与 conditional PUT CAS，不再无条件 last-writer overwrite。
- Revision ancestry 与确定性 conflict copy 已替代仅按时间戳的 v1 仲裁；当前未解决的产品缺口是完整冲突解决 UI。

以下前端可访问性与语言扩展问题也已解决：

- Settings/Dialog/命令面板共享嵌套模态栈，具备语义、topmost Tab/Escape、focus trap、背景隔离和焦点恢复，同时保留 Save/Discard/Cancel guard。
- 片段列表改为语义 list/listitem 与同级按钮；图标按钮补充名称/type，二态按钮暴露 pressed 状态，异步区域补充 busy/live 反馈。
- 标签建议采用完整 combobox/listbox/option 键盘模型；文本菜单只接管支持的编辑目标并支持菜单键盘导航/恢复焦点。
- HTML `lang` 跟随运行时语言；minimap 对辅助技术隐藏，CodeMirror 保持唯一主 scroller 和键盘可访问。
- 语言 factory 独立于 metadata；HTML、Go、C#、Elixir 使用对应 parser-backed 包，9 种 legacy mode 使用显式 StreamLanguage，plaintext 有意 fallback。
- 原生快速捕获仅在显式全局快捷键或托盘动作读取剪贴板，写入普通 revision/outbox 链路并发送脱敏完成事件；全局快捷键不可注册时托盘入口仍可用。
- 全局 focus-visible、reduced-motion、双主题缺失变量和系统字体栈已落实。


- 主列表改为有界 `SnippetSummary` cursor pages，完整正文选择后懒加载；`updated` 与本机 `recent` 使用独立 cursor，generation/query/cursor guards 抑制 stale response；使用记录不参与同步或导入导出。
- schema v2 建立同步 FTS5 external-content 索引；schema v3 保持 `snippets`/FTS 不变，并增加稳定 device identity、revision head/tombstone、bounded immutable outbox、remote state/conflict index 和扩展 sync history；schema v4 增加 durable `revision_objects`，用于 outbox 确认后的本地 ancestry 保留。
- create/update/favorite/delete/import winner 以及最多 200 项的批量收藏/删除现在原子维护 live row、FTS、head、durable revision object 与按需 outbox；批量操作先验证整组，收藏只为实际状态改变生成 revision，delete 从 FTS 移除 live row并保留 tombstone。
- v0/v1/v2/v3/v4/v5/v6 既有磁盘库升级到当前 v7 前创建/验证唯一 `pre-v7` backup，任一步失败恢复来源版本；v2 live rows 获得确定性 legacy head 且不批量进入 outbox，v4 会从 pending outbox、当前 live heads 和 tombstones 回填 durable revision objects，v5 增加不参与同步/导入导出的本地 `snippet_usage`，v6 增加去标识化 `sync_notifications` inbox，v7 增加 `local_snapshots` catalog。
- v4 的 snapshot、validated no-echo remote plan、durable revision-object archive、deterministic conflict index/copy 与 exact revision acknowledgement seams 已由 production WebDAV v2 engine 消费；即时结果契约包含 deletion、conflict、pending、protocol 与 manifest generation，历史包含除 pending 外的对应计数与 protocol/generation。
- WebDAV v2 通过 `protocol-v2.json`、immutable `objects/<revision_uuid>.json`、tombstone revisions、strong ETag 和 conditional manifest PUT 实现单向 v1→v2 activation 与跨设备 CAS；v1 legacy payload 保留但激活后忽略。
- Snippet、summary、head/outbox/remote state 和 sync history 行解码严格验证必需类型、tags JSON、boolean、revision/hash 约束和 RFC 3339，不再静默默认。
- JSON export 使用稳定 format ID/version/app/exported metadata envelope，import 保持旧顶层数组兼容并在写入前拒绝未知或 malformed 版本。
- 文件导出通过 `create_new(true)` 和确定性数字 suffix 避免同秒覆盖，继续不向 WebView 返回绝对路径。

以下安全问题也已在凭据/设置安全与 CSP 检查点解决，详见 [第二轮修复记录](remediation-round-2.md#9-凭据设置安全与-csp-检查点)：

- 新写 `settings.json` 不再含 WebDAV secret；旧明文只通过兼容结构迁移，安全写入成功后才净化 JSON。
- `get_settings` 返回脱敏 `SettingsView`，保存使用显式 Keep/Replace/Clear；password 字段不回填 persisted secret。
- 平台 `CredentialStore` 边界、测试 fake、迁移失败阻断和 credential/autostart/settings 补偿已建立。
- 损坏设置唯一隔离、`.bak`/defaults 恢复状态和受控打开数据目录已建立。
- 通用 Shell scope/plugin/dependency 与不必要窗口/autostart capability 已移除；仓库和可信目录使用固定后端命令。
- `withGlobalTauri` 已关闭，启动 inline executable/style 已迁至 Vite 资产，限制性 CSP 已启用。
- 远端 WebDAV 强制 HTTPS，只有严格 loopback 测试地址允许 HTTP。

以下其余问题不再作为未解决限制，详见 [问题修复记录](remediation-2026-07-31.md)：

- 第一阶段“没有 WebDAV mock suite”的缺口已通过可注入 transport/store/clock、loopback `tiny_http` 和专用远端路径解决；retained v1 cases 覆盖认证、HEAD fallback、边界、部分失败与 bounded convergence，v2 cases 覆盖 transport wire 和 fresh engine bootstrap/exact acknowledgement。完整 cutover/hard-stop/CAS/crash/concurrency engine-loopback 矩阵仍是第 7.1 节的独立缺口。全部自动化边界都不得访问真实凭据、用户数据库或真实 WebDAV。
- 后端生成普通保存时间戳并返回权威片段。
- 保存失败后阻止导航；新建后保持选择。
- WebDAV remote-only 误删、旧快照 manifest、同进程并发、路径编码、响应/manifest 边界与日志脱敏。
- `minimize_to_tray` 和自动同步开关/间隔运行时生效。
- 数据库 v1 迁移基线、空库示例重现、后端标签过滤。
- 事务化受限导入及准确计数。
- candidate-before-commit 与临时文件/备份设置替换。
- 自启动 OS 状态与设置保存失败补偿。
- 独立 frontend typecheck/lint/format/test、Markdown links、图标完整性、package/Cargo/Tauri/Cargo.lock/Vite UI 版本一致性命令和普通 push/PR Linux CI 基线。
- 结构化 Tauri command error、本地化安全 fallback、初始加载错误/重试和 mutation/reload 准确反馈。
- 权威 snippet reload/reconcile：干净编辑器刷新，dirty editor 保留并显示非模态状态。
- Tauri 2 窗口事件改用 `Window.listen(...)` 并执行 unlisten cleanup。
- 根级共享 `SettingsProvider`、后端 status 字段外的 draft/baseline、外部更新提示和设置 Save/Discard/Cancel 关闭保护。
- 未保存 WebDAV draft 禁止立即同步，并明确首次自动同步与后续间隔语义。
- toolbar/settings/tray/background typed 同步完成协调；所有成功来源刷新片段、设置和历史，后台只使用非模态状态。
- 自动同步 busy 快速重试、有界指数失败退避和成功后配置间隔恢复。
- 托盘职责移入 `tray.rs`，设置入口使用 `open-settings` event，设置保存/托盘切换后都会刷新自启动复选状态。
