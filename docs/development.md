# SnipVault 开发指南

> 本文描述当前仓库已有的开发能力和变更路径。不要把“建议以后增加”的检查写成已经存在的命令或门禁。

## 1. 开发环境

### 1.1 技术栈

- Node.js + npm
- React 19 + TypeScript 5.8 + Vite 6
- Rust stable + Tauri 2
- Windows 开发需要 Visual Studio Build Tools C++ workload 和 WebView2
- Linux/macOS 还需要 Tauri 官方文档对应的系统依赖

CI 当前使用 Node 20 和 stable Rust。根仓库没有 `.nvmrc`、`.node-version`、`rust-toolchain.toml` 或 `rust-version`，因此尚未机械固定最低版本；本地开发推荐与 CI 对齐使用 Node 20 和 stable Rust。

### 1.2 安装依赖

已有 [package-lock.json](../package-lock.json)，全新检出建议使用：

```bash
npm ci
```

日常需要调整依赖时再使用 `npm install`，并检查 lockfile 变化是否符合预期。

## 2. 现有命令

命令来自 [package.json](../package.json)：

| 命令 | 实际用途 |
|---|---|
| `npm run dev` | 启动 Vite 前端开发服务器，固定端口 1420 |
| `npm run build` | 执行 Vite 前端生产构建，输出 `dist/` |
| `npm run preview` | 预览 `dist/`；普通浏览器环境没有可用的 Tauri IPC |
| `npm run tauri -- <args>` | 调用 Tauri CLI |
| `npm run tauri:dev` | 执行 `tauri dev`，由 Tauri 配置自动运行 `npm run dev` |
| `npm run tauri:build` | 执行生产应用构建；Tauri 会先运行 `npm run build` |
| `npm run tauri:build:debug` | 构建 debug 安装产物 |
| `npm run tauri:info` | 输出 Tauri 环境信息 |

等价的 npm 参数转发形式也可用：

```bash
npm run tauri dev
npm run tauri build
```

不要把“先运行 `npm run build` 再运行 `tauri dev`”当作避免 Rust 重建的优化。`tauri dev` 的 [tauri.conf.json](../src-tauri/tauri.conf.json) 明确配置了 `beforeDevCommand: npm run dev`，并加载 `http://localhost:1420`，不会复用之前生成的 `dist/` 代替 Vite dev server。

### 2.1 Vite 开发服务器与懒加载编辑器

`npm run tauri:dev` 应是当前检出目录的唯一 Vite owner；不要在同一目录另起 `npm run dev`，因为 1420 使用严格端口，第二个进程不会自动改用其他端口。Vite dev 模式中，首次选择片段或新建草稿才会请求 lazy `SnippetEditor` 模块，因此已停止的 1420 server 会在此时表现为动态 import 失败；它不证明 CodeMirror/editor module 终止了 Vite。

`SnippetEditorLoadBoundary` 会把这类 rejected import/render failure 保持在右侧 pane，允许用户在恢复 server 后点击 Retry。Retry 不会重启 Vite 或自动轮询，开发环境只会请求一个新的带尝试 query 的绝对 `/src` module URL，避免复用已 rejected 的浏览器 import；生产构建仍使用静态 lazy import，不能依赖 localhost Vite server。

要安全诊断 Vite 本身的退出，请使用没有真实数据库、凭据或 WebDAV 配置的隔离环境：先仅以前台终端启动 `npm run dev` 并保留完整 stdout/stderr 与退出码，再在浏览器开发者控制台执行 `await import("/src/components/SnippetEditor.tsx")`。若仍在运行的 Vite 返回 HTTP transform 报错或进程退出，以终端的最后诊断定位问题；对 native 路径只运行 `npm run tauri:dev`，不要同时启动第二个 Vite。出现故障后可用 `netstat -ano | findstr :1420` 确认监听状态。不得为掩盖此问题恢复 idle editor prefetch、弱化 strict port 或在没有终端证据时调整生产 chunk 配置。

## 3. 当前质量保障状态

仓库已提供以下独立前端与仓库完整性命令：

| 命令 | 检查内容 |
|---|---|
| `npm run typecheck` | 对前端和 Node/Vite/Vitest 配置执行 `tsc --noEmit` |
| `npm test` | 以 watch 模式运行 Vitest |
| `npm run test:run` | 单次运行 Vitest，供本地完整检查和 CI 使用 |
| `npm run lint` | ESLint 检查 TypeScript、React、Hooks 和本任务新增的 Node 工具脚本 |
| `npm run format:check` | Prettier 检查维护中的配置、测试、完整性脚本和普通 CI workflow；当前有意不要求一次性格式化既有业务源码/文档 |
| `npm run docs:check` | 检查 `README.md`、`CLAUDE.md` 和 `docs/**/*.md` 的相对文件链接与 Markdown anchor |
| `npm run versions:check` | 比较 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和 Vite 注入的 Settings/UI 版本；设置 `SNIPVAULT_RELEASE_TAG` 时还校验 tag |
| `npm run icons` | 用 Tauri icon generator 从 [assets/app-icon.png](../assets/app-icon.png) 生成 [src-tauri/icons/](../src-tauri/icons/)，并从同一 canonical source 生成 1080×1080 [logo-1080.png](../assets/logo-1080.png) |
| `npm run icons:check` | 校验 canonical icon source、1080×1080 logo、生成 PNG/ICO/ICNS magic 与尺寸、Tauri 配置引用，并拒绝旧重复图标生成器/输出 |

前端测试使用 Vitest、jsdom、React Testing Library、`user-event` 和 `jest-axe`。共享 [setup.ts](../src/test/setup.ts) 提供 Testing Library 清理、jest-dom/axe 断言，以及 Tauri core invoke、event、clipboard 和 window API mock。测试现在覆盖窗口控制、列表/工具栏语义与名称、嵌套模态焦点、Settings 关闭 guard、标签 combobox、文本菜单范围/导航、HTML `lang` 同步、语言扩展穷尽分类，以及既有片段/设置/同步/安全工作流；它们仍不是应用级 E2E。命令面板、原生全局快捷键、托盘捕获、真实剪贴板可用性、recent cursor 和批量 mutation 的新增生产路径没有新测试代码（遵循本任务不新增测试代码的约束），必须通过对应的现有 gate 和隔离的真实 Tauri smoke 补充验证。

Rust 测试覆盖数据库数据格式、v3/v4/v5/v6/v7 migration/FTS/query、ignored benchmark、凭据/设置恢复、command transaction、scheduler/event/tray pure logic，以及 WebDAV v1/v2 protocol、transport、engine 和 store seams。当前 `tiny_http` suite 只绑定 `127.0.0.1:0`，使用专用 `/dedicated-test-root/snipvault/` 路径和 fake/in-memory store；除保留的 v1 transport/engine 行为外，也覆盖 v2 精确 marker/object/manifest 路径、条件请求头、parsed metadata、不可变对象碰撞恢复，以及 fresh v2 engine bootstrap/exact acknowledgement。其余 legacy cutover、ambiguous hard-stop、CAS exhaustion、crash/retry 和并发矩阵仍主要由 synthetic/pure unit tests 覆盖。它不访问真实用户数据库、平台凭据库或真实 WebDAV 服务。按照本阶段“不要新增或修改测试相关代码”的约束，revision history/descendant restore、snapshot worker/full-vault restore、restore/write gate 与 notification inbox 没有新增 focused 自动化覆盖；必须通过既有 gate 与隔离的真实 Tauri smoke 补充验证。

普通 [ci.yml](../.github/workflows/ci.yml) 在 push 和 pull request 上运行 Linux full gate：`npm ci`、format/lint/typecheck/frontend test/build、文档链接、版本一致性、图标完整性，以及 Rust fmt/check/clippy/test。workflow 不打包、不签名、不上传、不发布。发布产物仍由独立 [release.yml](../.github/workflows/release.yml) 处理。

当前保障仍有边界：

- ESLint 现有代码仍报告少量 warning；CI 只在 error 时失败，没有启用 `--max-warnings=0`。
- Prettier gate 有意限制到本轮新增/维护的工具、测试和配置，避免本轮广泛改写既有源码与文档。
- 现有 loopback-only WebDAV HTTP mock integration 同时覆盖 retained-v1 transport/engine、v2 transport wire 与 fresh v2 engine bootstrap/exact acknowledgement；legacy cutover、全部 hard-stop、CAS exhaustion、crash/recovery、concurrent local edit 等更宽矩阵仍缺少 loopback engine coverage。没有任何真实第三方 WebDAV compatibility test，也尚无 Tauri IPC/窗口真实运行 automation 或本次 v2 activation 的 production Tauri smoke。前端同步结果/历史已有 focused integration 覆盖，但仍不是完整 App 流程覆盖。
- 普通 CI 的 full gate 只在 Linux 执行；Windows/macOS 行为仍由本地人工验证和 tag release 构建覆盖。

`npm run build` 使用 Vite 转译并打包 TypeScript，不等同于完整类型检查；修改前端时应同时运行 `npm run typecheck`。Rust 检查可单独运行：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## 4. 目录和主要变更入口

```text
src/
├── main.tsx                 # 依窗口 label 选择主 App 或独立历史根；共享启动设置、SettingsProvider、语言和主窗口 ready
├── boot.ts / boot.css       # Vite 管理的主题 anti-flash 和 splash 样式
├── App.tsx                  # 根业务、片段 reconciliation、同步/快速捕获完成协调、命令注册、批量选择和快捷键
├── index.css                # 全局主题变量和布局样式
├── types/index.ts           # 前端 Snippet / SnippetForm / query、usage、bulk 协议
├── components/
│   ├── RevisionHistoryWindow.tsx # 独立原生窗口 target pull、事件刷新与 restore-request 控制器
│   ├── RevisionHistory.tsx    # immutable history、review desk 比较与仅请求式 descendant restore UI
│   ├── RestoreWizard.tsx      # 本地 SQLite checkpoint 管理与完整恢复确认
│   ├── SyncNotificationCenter.tsx # 持久脱敏同步收件箱
│   ├── CommandPalette.tsx    # 共享 ModalSurface 的命令面板
│   ├── ModalSurface.tsx      # 共享嵌套模态栈、focus trap 和焦点恢复
│   ├── languageExtensions.ts # 编辑器专用 parser/stream/plaintext 分类与 factory
│   └── ...                   # UI、编辑器、设置和 Dialog
├── hooks/                    # 片段 Hook 与权威 SettingsProvider/IPC
├── context/                  # LanguageContext 与可测试 LanguageProvider
├── i18n/                    # i18next 和中英文资源
├── test/                    # Vitest/RTL/user-event/axe focused integration tests
└── utils/languages.ts       # 可选择语言/颜色和 LanguageId；不导入编辑器包

src-tauri/
├── src/main.rs              # Tauri 启动、插件、窗口生命周期和 worker wiring
├── src/capture.rs           # 原生快速捕获、全局快捷键和脱敏完成事件
├── src/tray.rs              # 托盘所有权、菜单、事件与状态刷新
├── src/sync.rs              # typed sync event、脱敏 inbox 持久化和自动同步 scheduler
├── src/snapshots.rs         # SQLite online snapshot、验证、catalog、恢复与保留 worker
├── src/commands.rs          # IPC command 与内部错误边界映射
├── src/error.rs             # 稳定可序列化 CommandError
├── src/credentials.rs       # 平台 CredentialStore 与可注入测试边界
├── src/db.rs                # SQLite、真实 Snippet 模型、导入和历史
├── src/settings.rs          # 无 secret Settings、脱敏 DTO、迁移/恢复和 JSON 持久化
├── src/paths.rs             # 数据与导出路径
├── src/webdav.rs            # 稳定 facade、process lock、设置/凭据装配和 v2 调用
├── src/webdav/
│   ├── protocol.rs           # v1/v2 DTO、marker/object URL 与大小/字段边界
│   ├── transport.rs          # 可注入 HTTP/auth/clock/retry、strong ETag/conditional PUT
│   ├── engine_v2.rs          # production bootstrap/ancestry/CAS/publish/exact-ack engine
│   ├── engine.rs             # 保留的 legacy v1 engine/test helper；production 不调用
│   ├── store.rs              # v1/v2 可注入持久化 seam；生产 SQLite adapter
│   ├── error.rs              # 内部 structured failure 分类
│   └── integration_tests.rs  # loopback tiny_http mock suite
├── capabilities/default.json # main 窗口最小权限
├── capabilities/revision-history.json # history 窗口最小标题栏权限
├── tauri.conf.json
└── Cargo.toml
```

完整架构见 [架构设计](architecture.md)。

## 5. 前端功能开发

### 5.1 在现有分层中放置代码

- **可复用展示和局部交互**：优先放到 `src/components/`。
- **根业务流程、片段选择、跨组件状态**：当前集中在 [App.tsx](../src/App.tsx)；命令 registry、command enabled 状态、当前已加载项的 `Set<string>` 批量选择、dirty-editor guard 和捕获完成后的权威 reload 都必须继续通过该层协调，避免在命令面板/列表中复制业务逻辑。
- **Tauri IPC 与片段状态**：放在 [useSnippets.ts](../src/hooks/useSnippets.ts)；`updated` 与 `recent` 排序字段都必须进入 request key，`recent` cursor 不得与 `updated` cursor 混用。usage 写入可 best-effort，但批量 mutation 必须直接等待结果。版本历史窗口可用窄的 direct revision IPC wrapper，但不得装载主列表 Hook 状态；Conflict Center 的 direct IPC 只可传 conflict ID、expected source head 和枚举 action，关联 snippet/revision 必须由 Rust 从 `sync_conflicts` 导出；新 window command 必须由 Rust 校验调用 window label。
- **设置、同步状态与设置 IPC**：扩展 [useSettings.ts](../src/hooks/useSettings.ts) 中的 root `SettingsProvider` 和 injectable `SettingsApi`；消费者必须使用 provider，不得重新引入独立 `useSettings()` state 实例或绕过 provider 的 `get_settings` fallback。成功技术历史、持久脱敏 notification inbox 与 snapshot policy/status 都经此 provider 协调；notification reload 必须保留 request generation 保护，read/dismiss mutation 成功后要使旧 reload 失效。
- **共享协议类型**：更新 [types/index.ts](../src/types/index.ts)，并同步核对 Rust 参数和序列化名称。
- **外观与界面配色**：深浅偏好与 `sky | violet | emerald | amber | rose | white` 精选界面配色是独立的 persisted Settings 字段；类型、normalizer、启动镜像键和 `data-theme` / `data-accent` 写入集中在 [theme.ts](../src/theme.ts)。`ThemeProvider` 必须只消费根级 `SettingsProvider` 的权威 SettingsView；Settings 面板不得直接写 DOM attribute/localStorage 或成为第二个外观状态来源。六个精选值均须在 `index.css` 提供显式 dark × light 的 12 组完整 semantic token matrix（其中 `white` 的界面名称为“简约白”，并复现初始版本的中性深浅界面）。组件不得加入 palette-dependent raw 色值。不得把 preset 扩展为 raw CSS/hex 输入，也不得改变 syntax token、语言标签或状态语义。
- **语言信息**：可选择项、颜色和 `LanguageId` 在 [utils/languages.ts](../src/utils/languages.ts)；parser/stream/plaintext 分类与 factory 在 [languageExtensions.ts](../src/components/languageExtensions.ts)。元数据层必须保持无 CodeMirror import；新增 ID 时两处都要更新，穷尽分类测试会在遗漏时失败。
- **文案**：同步更新 [zh.json](../src/i18n/locales/zh.json) 与 [en.json](../src/i18n/locales/en.json)。

### 5.2 状态注意事项

设置运行时使用根级 `SettingsProvider`：

- `main.tsx` 的单次 boot settings promise 同时供 provider、主题和语言初始化，避免 provider cycle 和重复 IPC。
- `App`、`SettingsPanel` 与编辑器设置入口共享同一权威 settings state。
- 用户可编辑对象必须显式使用不含 secret 和后端所有权/status 字段的 `SettingsDraft`；不得用 spread 把 `SettingsView` 直接变成保存 payload。
- 持久凭据只通过独立 `SecretAction = Keep | Replace(value) | Clear` 修改。password 输入每次打开/保存后保持空白；任何读取 DTO、fixture、snapshot 或错误都不得包含 persisted secret。
- 外部 reload 与 save 必须保留 provider 的 request ID race protection；旧 reload 不能覆盖成功 save。
- Settings 面板需要局部 draft/baseline，但权威状态变化时只有 clean draft 可自动采用，dirty draft 必须保留并反馈冲突。

同步扩展必须保留 `SyncCompletionEvent` 的来源和状态 union；toolbar/settings direct command 由前端协调，tray/background 由 Rust event 协调，每个终态恰好写入一条去标识化 inbox 记录。所有成功来源只调用一次 `refreshAfterSync()`，并刷新 snippets/settings/success history/notifications；失败或 busy 只刷新 notifications。后台来源不得显示 modal。完整 vault restore 置位 `sync_confirmation_required` 后，background scheduler 取得 WebDAV lock 后也必须重新检查该锁；只有 toolbar/settings/tray 的成功手动同步可清除它。WebDAV `SyncFailure` 的 busy/retryable 元数据必须一直保留到 `CommandError` 映射；不要退回字符串匹配来判断认证/validation 是否可重试。

对片段表单做修改时，必须同步考虑：

- `SnippetForm`
- `EMPTY_FORM`
- `isFormDirty()`
- `loadSnippet()`
- 创建和更新 IPC 参数
- Rust `Snippet` 与 SQL 字段
- 导入导出 JSON 兼容性
- WebDAV v2 revision object DTO 与 canonical hash/payload compatibility

### 5.3 原生辅助窗口与跨窗口请求

版本历史是唯一按需创建的辅助 WebView：使用 Rust `WebviewWindowBuilder` 而不是在 `tauri.conf.json` 静态定义窗口。Windows 上不得在同步 command 或 event handler 里直接创建 WebView；`open_revision_history` 必须保持 async，并在建窗前以 Rust-managed Mutex state 写入 opaque target/generation。新窗口先 pull state，再把 event 当作刷新提示，不能让首次 target 仅依赖事件传送。任何新注册的 `#[tauri::command]` 都必须同步列入 [build.rs](../src-tauri/build.rs) 的 build-time `AppManifest` command 表，并在对应 capability 显式授予 `allow-<command>`；注册并不等于自动允许所有 WebView 调用。

该 state 只能保存必要的 opaque snippet/revision IDs、generation、有限 restore request 和 `succeeded | cancelled | failed` outcome；禁止保存正文、绝对路径、远端 URL、凭据、token 或原始错误。对 history 相关 command 除 capability 外还要验证 `WebviewWindow.label()`：main 只能 open/consume/complete，且 mutation `restore_snippet_revision` 也必须拒绝非 main 调用；history 只能 read target/request restore/read outcome。每个新窗口都需独立 capability 文件，最小化权限；history 仅取得标题栏实际所需的 core/window 控制（含 `start_dragging`），不要为方便让 child 获得 clipboard、shell、filesystem、show/hide 或主窗口权限。

跨窗口恢复必须由 `App` 主窗口执行。child window 只发 request；main 要同时监听通知并在注册后 pull pending request，防止 listener 尚未建立时丢失。主窗口独占 dirty guard、Dialog、权威 current-head 读取、restore IPC 和 reconciliation；不要在 child window 重建它们或让无关 dirty editor 被覆盖。主窗口隐藏到托盘时隐藏 child，实际退出前 destroy child；child CloseRequested 仅 hide 以供复用。

### 5.4 异步工作流

保存/导航工作流当前以 `handleSave(): Promise<boolean>` 表示可安全继续与否；切换、新建和取消只能在返回 `true` 后继续。`true` 既表示一次成功持久化，也表示已有干净表单的成功 no-op；该 no-op 不得发起 IPC、更新时间戳或创建 revision/outbox。标题校验、保存失败或保存中仍返回 `false`。后续修改必须保留这个约束，不能在内部吞掉错误后让调用者误判成功。

对 delete、favorite、sync、import/export 等 IPC 操作，应提供用户可见错误反馈，不只 `console.error` 或 Hook 内部保存错误状态。

## 6. CodeMirror 和 MiniMap 开发

核心文件：[SnippetEditor.tsx](../src/components/SnippetEditor.tsx)、[completion.ts](../src/components/completion.ts)、[languageExtensions.ts](../src/components/languageExtensions.ts)。

### 6.1 复用现有扩展入口

- 新增语言：先在 [utils/languages.ts](../src/utils/languages.ts) 增加 metadata/`LanguageId`，再在 [languageExtensions.ts](../src/components/languageExtensions.ts) 的 `LANGUAGE_SUPPORT` 和 `getLanguageExtensions()` 增加穷尽分支，并更新 [LanguageExtensions.test.ts](../src/test/LanguageExtensions.test.ts)。还必须在 [completion.ts](../src/components/completion.ts) 为该 ID 保留关键词 fallback；仅在有成熟、普适的离线模板时才添加 snippet catalog 项。
- 本地补全的 controller 只能由 `completion.ts` 的一个 `autocompletion()` extension 安装，`SnippetEditor` 的 UIW `basicSetup.autocompletion` 必须保持关闭，避免竞争 controller。不要设置 `override`：应用 source 通过 `EditorState.languageData` 注册，CodeMirror 会并列运行 parser package 的上下文 completion 和本地 source，并保留各自异步、replacement range、`validFor` 和 dedupe 语义。不得手写 `Promise.all` 合并 language provider，也不得让 registered source 再枚举 `languageDataAt("autocomplete", ...)`，否则会递归调用自身。
- 本地 source 必须无网络、无 LSP、无 IPC：静态关键词/有 placeholder 的 snippet、以光标为中心最多 120,000 字符扫描出的当前文档标识符，以及当前已加载同语言摘要的 title/description/tag 分词（至多 80 个）按低优先级提供。不得在输入时请求完整 snippet、传递 `content_preview`、绝对路径或任何 secret；最多返回 160 个本地候选。隐式触发至少需要一个 identifier 字符，`Ctrl/Cmd+Space` 可在空 token 显式触发。
- 优先使用官方 CodeMirror 6/维护中的 Lezer language package；若只存在 reviewed legacy mode，可使用 `StreamLanguage.define()`，但文档和 UI 不得把它描述为完整 Lezer parser。
- Plaintext 以及有意不支持的兼容 ID 必须显式返回空 `Extension`，不要通过漏掉 switch 分支获得隐式 fallback。
- 修改编辑器主题、光标、选区、滚动或换行：更新 `buildMainExtensions()` 中的 `EditorView.theme()` / `HighlightStyle`。编辑器只能安装 [codeHighlightTheme.ts](../src/components/codeHighlightTheme.ts) 的一个 non-fallback syntax highlighter；UIW wrapper 必须保持 `theme="none"`，不得再加入会注册另一套 highlighter 的完整 UIW theme。GitHub 兼容 tag 规则属于共享 `HighlightStyle`，view chrome 属于 `EditorView.theme()` 和语义 CSS token。编辑器 surface/gutter/active gutter、光标、选区、匹配括号和 MiniMap viewport 必须只使用 `--editor-*` / `--minimap-*` 语义 token，不能重新硬编码 preset 色。Canvas 背景色应从 `.minimap-pane` 的计算 token 读取，并在有效深浅模式或精选配色变化时重绘。
- 修改 Codeglance：复用 `MiniMap` 与 [syntaxHighlight.ts](../src/components/syntaxHighlight.ts) 的共享语言/语法高亮范围适配器；不要再建立独立正则 tokenizer 或硬编码 token 调色板。
- 版本历史 live preview/diff 必须通过 [LazyRevisionCodePreview.tsx](../src/components/LazyRevisionCodePreview.tsx) / [LazyRevisionDiffViewer.tsx](../src/components/LazyRevisionDiffViewer.tsx) 按需加载，不能因 [RevisionHistory.tsx](../src/components/RevisionHistory.tsx) 被 App 静态引用而把 parser/highlighter 提前纳入启动图。它们重用 [codeHighlightTheme.ts](../src/components/codeHighlightTheme.ts) 的 editor token palette 和 [syntaxHighlight.ts](../src/components/syntaxHighlight.ts) 范围；plain DOM `<span>` 使用生成 class 前须调用 `StyleModule.mount(document, HighlightStyle.module)`，绝不能用 `dangerouslySetInnerHTML`。逐行比较由 [lineDiff.ts](../src/components/lineDiff.ts) 本地执行，必须保留字符/行数/matrix/渲染行数/时间上限与完整并排源码 fallback；历史 code review 固定 `white-space: pre` 和行号，弹性双栏不得为布局制造外层横向滚动，只有真实超出单个 source pane 的长行可在所属 pane 内横向滚动，详细对齐仅同步垂直位置。
- 修改滚动：以 `EditorView.scrollDOM` 为主滚动源，不额外截获 wheel 事件。
- 编辑器只在 `SnippetEditor` 首次渲染时通过 [LazySnippetEditor.tsx](../src/components/LazySnippetEditor.tsx) 的 lazy boundary 加载；不得在 `App.tsx` 静态导入 CodeMirror runtime 或无用户编辑意图时 idle-prefetch。`SnippetEditorLoadBoundary` 只能隔离懒模块被拒绝或 editor render 的 pane 级失败：Retry 必须由用户显式触发、由 `LazySnippetEditor` 在组件边界创建新的 lazy identity 并继续使用 App 持有的 selection/form；不得自动轮询、重启 Vite、重取详情或清空草稿。开发 retry 可使用独立 Vite `/src` query URL 避免浏览器缓存 rejected import，生产初始和 retry 路径必须继续有静态可分析的 editor import。文本菜单只有在明确命中 `.cm-editor` 时才可在其异步 action 内按需解析 `EditorView`，解析失败必须安全关闭，不得转而对其他元素执行操作。
- [vite.config.ts](../vite.config.ts) 必须将 CodeMirror runtime、services、UI 与 parser/stream language family 分配到有界语义 chunk；新增语言包及其 Lezer grammar 必须进入对应 family，不能恢复 `@codemirror`/`@lezer`/`codemirror` 的宽泛 catch-all，也不能提高 `chunkSizeWarningLimit` 隐藏超限。chunk 拆分只能改变传输与缓存边界，`getLanguageExtensions()`、Codeglance 解析和同步 minimap 高亮仍必须在 editor 内保持当前同步共享契约；若要做按语言异步加载，需要独立设计 reconfigure、stale-result 和 fallback 流程。

### 6.2 DOM 和样式事实

当前外层结构是：

```text
.cm-editor-split
├── .cm-main-pane
│   └── .snippet-codemirror
└── .minimap-wrap
```

不要继续按旧 `.cm-editor-wrap → shadow root` 描述设计。CodeMirror 管理其内部 DOM，但当前应用并不使用 CodeMirror Shadow DOM 作为样式边界。

[index.css](../src/index.css) 仍有若干 `.cm-*` 和历史样式；修改前应确认 JSX 是否仍引用选择器。对 token、光标、选择和滚动的关键样式，优先通过 CodeMirror API 保持与组件生命周期一致。界面配色更新必须保持语法 token/Codeglance class 颜色不变，同时让 editor/minimap chrome 通过 CSS variable 同步。

### 6.3 MiniMap 约束

- 活动 Codeglance 必须从可编辑 CodeMirror 的 live `EditorState` 同时取得正文与 `syntaxTree(state)`，再通过 [syntaxHighlight.ts](../src/components/syntaxHighlight.ts) 的共享 tree-to-range adapter 和 [codeHighlightTheme.ts](../src/components/codeHighlightTheme.ts) 的唯一 `HighlightStyle` 绘制；不得为每次输入建立独立 parser、增大 parse timeout、轮询、调用 forced parsing 或挂载第二个 editor。
- 只在 document identity 或 syntax-tree identity 改变时发布 Canvas 输入。这样普通输入/粘贴、受控内容或语言重配置以及 CodeMirror background parser 发布新树都会重绘，而 selection/focus-only transaction 不会；正文和 parser snapshot 不得来自不同生命周期。
- 独立 `EditorState` + 50 ms `ensureSyntaxTree()` 只保留给没有 live `EditorView` 的版本历史只读 renderer。活动 Codeglance 对当前树尚未覆盖、无 token 或 plaintext 的文本使用编辑器默认前景色，并等待 CodeMirror 自身的后续 tree publication。
- Canvas 按生成 class 测量颜色前必须先挂载共享 `HighlightStyle.module`；有效深浅模式或精选配色变化时清空 class-color cache。空白字符只占用缩略图水平位置而不绘制色条。
- Canvas 高度随行数增长，大文件可能有性能和浏览器 Canvas 尺寸限制。
- viewport 比例依赖主 scroller 的 `scrollHeight/clientHeight`。
- 宽度 state 当前不持久化。

若引入第三方 minimap，应先明确是否替换现有实现，并同步更新功能文档和候选遗留依赖说明。

### 6.4 可访问交互约束

Settings 和 Promise Dialog 必须复用 [ModalSurface.tsx](../src/components/ModalSurface.tsx)，不得增加第二套 document/window Tab/Escape trap。Conflict Center、Device Identity Recovery、命令面板和恢复向导同样必须复用它：使用稳定的 dialog label/description、确定性初始焦点与 topmost ownership；Device Identity Recovery 的高影响轮换必须把最终确认委托给现有 Promise Dialog，不能由嵌套向导再创建竞争的焦点 trap。命令面板使用初始搜索框焦点、listbox/option、`aria-activedescendant`、Arrow/Home/End/Enter 和关闭后的触发器焦点恢复；命令执行前先关闭面板。模态必须保留背景 inert/ARIA 和焦点恢复。关闭 Settings 的所有入口仍须通过异步 Save/Discard/Cancel guard。

列表项不得通过可点击 `div` 模拟按钮，也不得把收藏/删除嵌套在选择按钮内。批量复选框需要包含片段标题的可访问名称、可见 selected count 和最多 200 个当前已加载项的边界；勾选不应打开详情或干扰脏草稿导航。标签建议沿用 combobox/listbox/option 和 active-descendant 键盘模型；自定义 context menu 只可接管明确支持的可编辑文本目标，其他区域必须保留原生菜单。图标按钮同步中英文名称，二态按钮使用 `aria-pressed`，异步状态按场景使用 `aria-busy`/live region。

布局或动画修改还必须保持共享 `:focus-visible` 和 `prefers-reduced-motion` 行为。不要把 `overflow: hidden` 放到导致 `EditorView.scrollDOM` 无法解析高度的祖先；minimap 保持辅助技术隐藏且不能成为编辑器键盘访问的替代路径。


## 7. Tauri IPC、原生捕获与事件

### 7.1 新增命令

典型流程：

1. 在 Rust 模块实现业务函数。
2. 在 [commands.rs](../src-tauri/src/commands.rs) 增加 `#[tauri::command]` 适配函数。
3. 在 [main.rs](../src-tauri/src/main.rs) 的 `tauri::generate_handler!` 注册。
4. 在前端 Hook 中调用 `invoke()`。
5. 同步 TypeScript 请求/响应类型和 Rust serde 命名。
6. 若使用新 Tauri 插件或系统 API，审查 [default capability](../src-tauri/capabilities/default.json) 和必要的 [tauri.conf.json](../src-tauri/tauri.conf.json) scope；只增加 WebView 实际调用所需的最小 capability。
7. 更新 [架构设计的 IPC 契约](architecture.md#6-ipc-契约) 和相关功能文档。

批量 mutation 的 command 参数必须先受限、排序、去重并验证全部 live ID，再在一个 SQLite transaction 内处理；客户端不可用 `Promise.all` 拼接多个单项调用来伪造“批量”。收藏必须是幂等的显式设值，未改变项不生成 revision/object/outbox；删除为每一有效片段生成独立 tombstone。业务写入成功后，前端只做一次权威 reload；reload 失败应和 mutation 失败分开反馈。

`src-tauri/src/commands.rs` 的新增 conflict/identity command 必须同时出现于 `main.rs` handler、[build.rs](../src-tauri/build.rs) manifest command 表和 main [default capability](../src-tauri/capabilities/default.json) 的最小 allow-list；不应把它们加入 [revision-history capability](../src-tauri/capabilities/revision-history.json)。所有五个 command 都需验证 main window label；`resolve_sync_conflict` 先取得 snapshot mutation guard，`rotate_device_identity` 先取得 WebDAV exclusive guard 再取得 mutation guard。

### 7.2 原生快速捕获和事件生命周期

[capture.rs](../src-tauri/src/capture.rs) 统一原生全局 `Ctrl/Cmd+Shift+V` 与托盘“从剪贴板快速捕获”入口。仅在这两个明确用户动作触发时，后台线程才读取纯文本剪贴板；空白、读取失败或写入失败都以脱敏失败结果结束。它创建普通的 `plaintext` 片段，并在同一 transaction 内写入正常 live/FTS/head/revision-object/outbox 链路及仅本地的 usage；不得为托盘或快捷键实现第二条绕过 revision/outbox 的保存路径。

全局快捷键在 Rust 原生侧注册：注册冲突或平台失败仅记录安全的通用诊断，不能阻止启动，托盘入口仍需可用。插件的 native registration **不**意味着 WebView 获得 global-shortcut capability；除非前端实际要调用该插件，不得在 default capability 加入 `global-shortcut:*`。剪贴板正文、推导标题、底层错误、凭据和本地路径不得写日志、返回 IPC 或进入事件。现有 clipboard-manager capability 也不能扩展为任意 shell/file/network scope。

完成事件 `quick-capture-complete` 和 fallback command 只携带 `source`、`success` 与可选 `snippet_id`。原生层以单槽 latest-completion 缓存弥补前端 listener 尚未注册的窗口；前端收到实时事件时也读取一次 fallback 以消费该槽，并按成功 `snippet_id` 去重。捕获只允许一个 in-flight 操作，重复快速触发安全忽略；不要用 clipboard 内容来判断重复。成功后由 [App.tsx](../src/App.tsx) 进行权威列表刷新并保留 dirty draft，失败只显示短暂非模态反馈。

命令面板是纯前端 `Ctrl/Cmd+K` 工作流，不注册第二个全局快捷键。命令定义由 `App.tsx` 提供并复用已有 handler；Settings 或 Promise Dialog 打开时不响应，执行命令前先关闭面板，使后续 dirty guard/confirm Dialog 保持唯一 topmost owner。“聚焦代码片段搜索”不得在 command handler 中直接调用输入框 `focus()`：此时 palette 仍挂载，工具栏位于 inert 背景内，且 `ModalSurface` 的卸载会恢复触发元素焦点。应由 `App` 记录关闭后的焦点意图，并在 `commandPaletteOpen` 变为 false 的 effect 中、ModalSurface cleanup 之后聚焦工具栏输入框。

### 7.3 参数和错误

现有活跃 fallible command 在 [commands.rs](../src-tauri/src/commands.rs) 边界返回 [error.rs](../src-tauri/src/error.rs) 的 `CommandError`。稳定 code 集合为：

- `validation`
- `not_found`
- `stale_revision`
- `outbox_full`
- `database`
- `settings`
- `network`
- `sync_busy`
- `sync_cas_conflict`
- `sync_legacy_changed`
- `import`
- `export`
- `autostart`
- `credential`
- `recovery`
- `snapshot`
- `open`
- `unknown`

新增或修改命令时必须：

- 在 command adapter 将内部 `rusqlite::Error` / `String` / plugin error 映射到稳定 code、安全 fallback、`retryable` 和可选安全 `details`。
- 只在 Rust 原生侧记录内部原因；不要把 credential、完整敏感 URL、远端响应体、剪贴板正文、推导标题、本地路径、SQL 诊断或任意 source error 复制到公开字段。
- 前端使用 [commandErrors.ts](../src/utils/commandErrors.ts) 的 `normalizeCommandError()` / `localizeCommandError()`，并同步更新 [zh.json](../src/i18n/locales/zh.json) 与 [en.json](../src/i18n/locales/en.json)。不要 `String(error)` 后直接显示。
- 未知 object、普通字符串、`Error` 和 malformed JSON rejection 必须安全回退 `unknown`。
- 检查 SQL 受影响行数，将不存在 ID 映射为 `not_found`，避免静默成功。
- 后续新增稳定 code 或 error mapping 时，需要按正常质量要求补充序列化、redaction、normalization 与 localization 验证；本阶段按用户约束不新增测试代码。

Tauri 2 window event 使用 `getCurrentWindow().listen(...)`；异步注册必须保存并在组件卸载时调用返回的 unlisten。不要通过 `as any` 调用不存在的 `Window.on(...)`，也不要用 `window.eval()` 或 WebView 全局函数代替受支持的 typed event。托盘逻辑归 [tray.rs](../src-tauri/src/tray.rs)，同步事件/scheduler 归 [sync.rs](../src-tauri/src/sync.rs)，`main.rs` 只负责 wiring 和生命周期。

## 8. SQLite 和数据模型开发

核心文件：[db.rs](../src-tauri/src/db.rs)。

### 8.1 当前连接模型

所有操作通过 `with_db()` 获得唯一 `Mutex<Connection>`。长时间网络操作不要在持有 DB lock 时运行；同步当前采用短时读取/写入与网络步骤交错。

### 8.2 Schema 变更

数据库使用 `PRAGMA user_version`，当前 schema v8，`initialize_connection()` 只按 `v0→v1→v2→v3→v4→v5→v6→v7→v8` 顺序执行迁移：v1 建立业务表，v2 严格扫描既有行并建立/回填 FTS5 与 triggers，v3 保持 `snippets`/FTS 设计不变并增加稳定设备身份、revision head/tombstone、不可变 durable outbox、remote state、conflict index 和扩展 sync history，v4 增加 durable `revision_objects` 并从 outbox、当前 live heads 和 tombstones 回填 ancestry，v5 增加本机 `snippet_usage`、recent index 和 live-snippet 删除清理 trigger，v6 增加去标识化 `sync_notifications` inbox，v7 增加 `local_snapshots` catalog，v8 为 `sync_conflicts` 增加仅本机的 lifecycle/resolution 字段，并为 `sync_identity` 增加 `last_rotated_at`。既有磁盘 v0/v1/v2/v3/v4/v5/v6/v7 在任何升级步骤前只创建并验证一个来源版本的 `pre-v8` online backup；任一步失败都会保留失败副本、恢复并重新验证原来源版本。真正新库、v8 重开和未来版本拒绝不创建升级备份。

`snippet_usage` 只用于本机“最近使用”：打开详情、复制已保存正文和快速捕获可 best-effort 写入 `last_used_at`/计数，但不改变 snippet `updated_at`，也绝不能进入 revision payload/object、outbox、WebDAV、JSON 导出或导入。既有升级片段不伪造使用历史。删除 trigger 只清理本地 usage，不改变 tombstone/revision 语义。

### 8.3 本地快照、恢复与通知不变量

[snapshots.rs](../src-tauri/src/snapshots.rs) 是完整 vault checkpoint 的唯一业务边界。创建时只能从活动 SQLite connection 执行 online backup 到受控 snapshots 目录中的 pending 文件；候选必须以独立 `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NOFOLLOW` 连接通过 integrity、允许的 current（v8）或紧邻 previous（v7）schema、唯一 `sync_identity`、live count、file size 与流式 SHA-256 验证，才可原子 rename、写入 catalog 并在成功后执行 retention。文件名只能是后端生成的 canonical `snapshot-<uuid>.sqlite`，IPC 只返回 opaque ID 和安全摘要，不能暴露 filename、checksum 或绝对路径。

完整 restore 必须依次取得 snapshot serialization、WebDAV operation 与 restore/write gate，先重新验证目标、建立 verified emergency checkpoint，再由 SQLite `Backup` 写入既有活动 connection；不得以文件 copy/rename 覆盖打开的 DB。目标和 emergency 文件在每次 reopen/copy 前都要重新计算并比对 catalog 的流式 SHA-256，若验证后的文件被替换则拒绝安装。若源是唯一允许的 v7 snapshot，复制到活动 connection 后必须调用 `db::migrate_connection_to_current()` 执行受控 v7→v8 迁移，并在提交前重新通过 current-schema 验证。所有生产数据库 mutation—including snippet CRUD/import/usage/history restore、quick capture、conflict resolution、identity rotation、remote plan/commit 与 notification read/dismiss/write—必须经 `snapshots::mutation_guard()`，避免 emergency checkpoint 与数据库替换之间丢失提交。设置保存和完整同步（包含手动同步完成时清除 restore confirmation latch）也必须在 restore/write gate 内执行，避免恢复后的 `sync_confirmation_required` 被旧状态覆盖或被恢复前的同步误清除。restore 失败时应通过 emergency checkpoint 复原活动 connection；恢复成功后 catalog reconciliation、retention 失败或记录 `restore_required` 通知失败不能谎报已经提交的 vault restore 为失败。

`sync_notifications` 是与 20 条成功 `sync_versions` 分离的本地隐私边界：只可持久化固定 source/status/category、稳定 error code/retryable、聚合计数、protocol/generation、时间与 read/dismiss metadata；绝不保存自由文本消息、URL、用户名、secret、远端响应、路径、片段正文或 revision/hash。写入最多保留最新 200 条，列表默认 50、上限 100。完整 SQLite snapshot 会包含 inbox 和 catalog；JSON export/import、WebDAV payload 与 OS credential store 均不包含它们。

`sync_identity` 在 v3 migration 首次生成；v8 的 Device Identity Recovery 只可由 Rust 生成替换 UUID，并只更新 singleton 的当前 identity/`last_rotated_at`。轮换必须严格取得 `webdav::try_exclusive_operation_guard() → snapshots::mutation_guard() → short db::with_db_mut()`，不发 HTTP、不启动 sync、也不重写 head/object/outbox/remote attribution；后续真实 local mutation 才读取新值。`sync_conflicts` 的 v8 lifecycle 是 local-only：candidate resolution 必须通过 conflict ID 和 expected source revision，由后端派生关联 ID、在一个短 transaction 再次校验 state/head，并确认 lifecycle `UPDATE` 恰好影响一行。保留规范结果不创建 no-op revision；应用/新建只复用正常 object/outbox 写入；source 已前进仅允许 reviewed，不能覆盖更晚内容。

任何后续 schema 变更都必须：

- 将 schema version 递增且只逐级迁移，不能把 `CREATE TABLE IF NOT EXISTS` 当作旧库升级。
- 在 transaction 中执行，并定义失败恢复；既有磁盘历史版本必须在升级前创建、验证并在失败时恢复唯一 preflight backup。
- 保持旧用户数据与 import/export 兼容，并测试 v1 remote bootstrap 到 v2 object/manifest 的一次性转换；本地模型字段不得绕过显式 wire DTO。
- 测试真正新库、每个历史版本、既有空库、重复执行、高于当前版本的拒绝路径、稳定 identity、确定性 backfill、损坏 backfill rollback 和每个来源版本的磁盘恢复。
- 不得对当前用户数据库做测试性手工修改、删除或迁移演练；migration/recovery fixture 只使用 temp DB。

### 8.4 Revision、tombstone 与 outbox 不变量

本地 create、update、favorite、delete 和 import winner 必须在同一 SQLite transaction 中完成 live snippet、既有 FTS trigger 副作用、`snippet_heads`、durable `revision_objects` 和按需 `revision_outbox` 写入；任一步失败必须整体回滚。delete 删除 live row/FTS 并保留 tombstone head/object/outbox。update 必须在该 transaction 内读取当前 head 并比较前端 `base_revision_id`；不匹配返回结构化 `stale_revision`，前端刷新权威 detail/base revision，但不得覆盖 dirty draft。

Revision payload 使用 [revision.rs](../src-tauri/src/revision.rs) 的固定 canonical DTO 与 SHA-256；不要直接序列化会继续演进的 domain struct。`revision_objects` 是 immutable local archive，只能插入，不能原地修改或在 outbox acknowledgement 时删除。Outbox row 是 pending publish copy，只能插入或按**确切 revision ID**确认后删除，不得按 snippet ID、时间范围或 sequence 上界批量确认。写入前强制限制 pending 最多 10,000 条、payload 总计最多 64 MiB，且单 revision payload 受独立上限保护；达到边界后返回 `outbox_full`，不得先更新 live row。

Production WebDAV v2 必须复用以下数据库边界：

- `load_sync_snapshot()` 在短 DB lock 内读取 identity、live snippets、全部 heads/tombstones、durable revision objects、顺序 outbox 和 remote state；返回后再执行网络 I/O。
- `apply_validated_remote_plan()` 必须先完整校验 ID、revision、timestamp、hash 与 payload/head 一致性，再以单 transaction 应用；remote revision 必须写入 `revision_objects`，但不得 echo 到本地 outbox。
- conflict copy/index 必须由 source snippet 与排序后的两端 revision 生成确定性 ID，并在重试时幂等。
- `commit_published_revisions()` 将 remote state、history 与确切 ack 集合原子提交，只删除列出的 outbox revisions，保留 `revision_objects` 和 snapshot 后产生的 later edits。
- Tombstone 是正常 immutable revision，必须和 live ancestry 一样发布、验证和保留；当前不得增加静默 remote GC/compaction。

修改上述不变量至少覆盖 stale base、FTS tombstone removal、mutation rollback、strict row decoding、count/byte 精确边界、remote no-echo、conflict retry、invalid-plan rollback、exact/repeated ack 和 later-edit preservation。

### 8.5 时间戳所有权

正常创建、编辑和收藏的版本时间由 Rust 在成功写入时生成，创建/更新 IPC 返回最终 `Snippet`。前端不得重新引入客户端时间作为数据库版本，也不能在本地伪造 `updated_at`。导入和 WebDAV 例外地使用外部数据携带的时间，但必须先通过 RFC 3339 与字段边界校验。

### 8.6 查询、FTS 与摘要/详情协议

主列表只能使用 `SnippetSummary` 分页协议，不得重新通过列表 IPC 返回完整 `content`。`updated`（默认）使用 `(updated_at DESC, id DESC)`；`recent` 先按 usage 是否存在、再按 `last_used_at DESC`，最后用 `(updated_at DESC, id DESC)` 稳定打破并列。两种排序的 cursor 都编码排序模式和对应排序键，不能互用。后端 page size 封顶 200；前端当前每页 100，搜索、筛选或排序变化会清空 cursor 和当前批量选择，并以 generation/query/cursor guard 丢弃 stale response。完整正文仅由 `get_snippet(id)` 懒加载，权威刷新必须继续保留 dirty editor。

`recent` 是纯本地访问顺序，不是相关度、跨设备最近修改或同步状态；未使用项稳定置底。usage 写入成功后，若当前采用 `recent`，前端可以刷新列表以反映新顺序，但 usage 失败不得阻断打开或复制。

FTS5 是 external-content table，任何 CRUD、收藏、导入或 WebDAV merge 变更都必须保持 triggers/transaction 同步。用户输入不得直接作为 raw MATCH；至少覆盖 `%`、`_`、引号、反斜线、短查询、CJK、trigram 和 `unicode61` fallback。当前结果不按相关度排序，不要把任意一种排序描述为相关度排序。

确定性的 1k/10k ignored benchmark 记录 row count、返回 count、序列化 payload bytes 和 latency：

```bash
cargo test --manifest-path src-tauri/Cargo.toml db::tests::benchmark_query_1k_and_10k -- --ignored --nocapture
```

它使用内存临时数据库，不访问用户数据；结果是开发机 checkpoint，不是稳定性能 SLA。

### 8.7 导入/导出格式

当前导入先限制 25 MiB / 10,000 条，并接受 `snipvault.snippets` schema v1 envelope 或遗留顶层 `Snippet[]`。Envelope version/metadata 和全部 Snippet 必须在写入前验证，再在单个 transaction 中按 ID/`updated_at` 合并；每个 winner 同时写 live row、FTS、head、revision object 和 import-origin outbox，任一无效条目或 pending limit 失败都会整体拒绝。返回结构为 `input_count / inserted / updated / skipped`。schema v1 envelope 可加法携带 `revision_id`，旧文档因 serde default 继续兼容；import 不信任该值作为本地 head，而会为 winner 生成新的本地 revision。

扩展格式时必须保留明确 format ID、逐版本兼容/拒绝策略和旧数组 fixture；文件导出继续使用 `create_new(true)` 与确定性数字 suffix，不得返回绝对路径。

后续仍可设计：
- 更明确的 UUID/ID 策略（当前允许受限的非 UUID ID）。
- 冲突预览、用户确认和 schema migration。
- 属性级错误定位，而不是只有字符串错误。

## 9. Settings 与凭据开发

核心文件：[settings.rs](../src-tauri/src/settings.rs)、[credentials.rs](../src-tauri/src/credentials.rs)、[commands.rs](../src-tauri/src/commands.rs)、[Settings.tsx](../src/components/Settings.tsx)、[useSettings.ts](../src/hooks/useSettings.ts)。

新增设置字段时至少同步：

1. Rust 持久 `Settings` 字段和 `Default`；secret 不能加入该结构。
2. 旧 JSON 兼容结构、序列化和恢复行为；当前 `LegacySettings` 是唯一允许读取旧 `webdav_password` 的边界。
3. 只读 `SettingsView`、写入 `SettingsInput` 与 TypeScript `SettingsView` / `SettingsDraft`；不得以同一 full object 兼任读写 DTO。
4. 设置表单初始化、输入、保存、baseline/dirty 和运行时应用逻辑。
5. 是否需要 Rust 生命周期动态重配置。
6. 是否敏感；持久 secret 必须走 `CredentialStore`，不能写 JSON、返回 WebView、进入 scheduler、日志、错误、测试 snapshot 或路径响应。
7. 中英文文案。
8. 架构、功能、README 和限制文档。

`save_settings` 是跨三个副作用的 candidate-before-commit 事务：需要变更 secret 时先快照凭据并应用 `Keep/Replace/Clear`，再应用 OS autostart，最后原子持久化无 secret JSON；后续失败必须尝试恢复 autostart 和旧凭据。新增步骤必须定义顺序、回滚和“补偿本身失败”的安全恢复状态，不能提前更新内存或让 `last_sync_at` 由客户端覆盖。

旧明文迁移必须遵守：先写平台凭据库，成功后才能净化 JSON；失败时保留遗留文件，不加载/暴露 secret，并阻断凭据读取和持久自动同步。损坏 JSON 必须隔离到唯一文件名，再尝试有效 `.bak` 或安全 defaults；不要在公开错误中返回 quarantine/数据目录的绝对路径。

生产凭据身份由 [credentials.rs](../src-tauri/src/credentials.rs) 的 service/account 常量固定，改动它们需要显式迁移。测试必须注入 `MemoryCredentialStore` 或失败 fake，禁止调用开发机真实 Windows Credential Manager、macOS Keychain 或 Linux Secret Service；依赖下载/平台 backend 不可用时也不得退回明文。

`minimize_to_tray` 和自动同步 worker 会动态读取设置。scheduler 只持有 `credential_revision` 来检测变更，实际同步才读取 secret。`save_settings` 和托盘自启动入口还会调用 [tray.rs](../src-tauri/src/tray.rs) 的共享菜单刷新逻辑。新增生命周期设置仍必须明确是“每次事件读取”“worker 轮询”“event 驱动重建”还是“仅启动生效”，不能只依赖整对象保存。

Settings UI 的 draft 必须排除后端所有权/status 字段；新增 WebDAV 相关设置或 secret action 时还要更新“未保存/凭据需恢复则禁用 Sync Now”的集合和中英文说明。保存/关闭行为需继续使用异步 Save/Discard/Cancel guard，且保存失败不得关闭面板、清空 draft 或伪装成成功。

## 10. WebDAV 开发

核心 facade：[webdav.rs](../src-tauri/src/webdav.rs)。协议、transport/auth、production v2 engine、legacy v1 engine、持久化 adapter 和测试分别位于 [webdav/protocol.rs](../src-tauri/src/webdav/protocol.rs)、[webdav/transport.rs](../src-tauri/src/webdav/transport.rs)、[webdav/engine_v2.rs](../src-tauri/src/webdav/engine_v2.rs)、[webdav/engine.rs](../src-tauri/src/webdav/engine.rs)、[webdav/store.rs](../src-tauri/src/webdav/store.rs) 与 [webdav/integration_tests.rs](../src-tauri/src/webdav/integration_tests.rs)。

### 10.1 当前 v2 协议与兼容策略

```text
<base>/snipvault/manifest.json
<base>/snipvault/protocol-v2.json
<base>/snipvault/objects/<revision_uuid>.json
```

v2 manifest 保存 vault/generation/head revision；marker 只判别 v2 activation；objects 是不可变 live/tombstone revisions。production facade 只调用 `V2SyncEngine`；`engine.rs` 的 v1 engine 仍可作为 historical/test code 存在，但不得把它描述为 active sync path。

Cutover 是 one-way：fresh 与合法 v1 vault 都会激活为 v2；legacy v1 payload files 不删除、不覆盖，激活后忽略；没有 downgrade 或兼容双写，旧客户端不得共享已激活目录。协议修改必须维持 bootstrap matrix：

- marker/manifest 都缺失：fresh create。
- marker 缺失 + v1 manifest：读取并校验全部 legacy payload 后 activate。
- marker 缺失 + v2 manifest：interrupted activation recovery；要求 strong ETag，如已有本地 vault identity 则要求一致，作为 ready state 条件发布下一代 manifest，再补建 marker。
- marker 缺 manifest、marker + v1、vault ID 不一致：hard-stop。
- 已有本地 v2 commit/vault identity 时，远端整体消失或退回 v1：hard-stop，不按 fresh/legacy 重建。

### 10.2 必须保持的发布与 reconcile 不变量

- manifest GET 必须有 strong ETag；weak/missing/malformed ETag 不得降级为无条件 PUT。
- fresh manifest/marker 用 `If-None-Match: *`；ready 或 v1 cutover manifest 用 `If-Match: <observed-strong-etag>`。412 可在最多 4 CAS rounds 内重新观察；428/不支持 conditional PUT 是 server incompatibility，不得 fallback。
- 发布顺序固定为 immutable objects → conditional v2 manifest → conditional marker create → exact reread/verification → local exact commit。
- 对丢失响应的 object PUT 只 GET 精确 revision 内容并比较，不能 blind retry；相同 revision UUID 的不同内容必须 validation failure。
- 远端 ancestry 要检测缺失对象、cycle、snippet mismatch、hash/payload mismatch；所有请求共享 5 分钟 invocation deadline。
- ancestry 后代胜祖先；无祖先关系时，已发布的 remote original 确定性胜出。remote winner 覆盖 live local branch 时要通过数据库 seam 生成幂等 conflict copy/index；落败 tombstone 没有 live copy，但仍上传 immutable revision 并在 manifest durable 后精确 ack。没有 semantic merge。
- Tombstone 与 live revision 使用同一 ancestry/publish 路径，远端无限期保留；不得在没有版本化 GC 协议与迁移/兼容测试时删除 objects。
- `apply_validated_remote_plan()` 必须保持 expected-local-head guard 与 no-echo；`commit_published_revisions()` 只 ack 当前 snapshot 中候选的精确 revision IDs，不能误清 later edits。
- 任一 object、manifest、marker、verification、history、remote state、exact ack 或 `last_sync_at` 失败都不得返回 success。
- Immediate `SyncResult`/UI 字段名是 `protocol_version`、`manifest_generation`；history 使用 `protocol_version`、`generation`。不要在这两个 DTO 之间误复用 generation 名称。

### 10.3 认证、URL 与验证范围

新增认证能力时复用 [transport.rs](../src-tauri/src/webdav/transport.rs) 的统一 request/auth 路径，secret 只从 `settings::get_webdav_secret()` 获取。错误和日志必须 redact URL userinfo/query token、响应敏感数据、凭据值、snippet content 和本地路径。

Base URL 校验由 `validate_base_url()` 统一承担：生产端要求 HTTPS，HTTP 只允许严格的 `localhost` / `127.0.0.1` / `::1` loopback 测试地址，并继续拒绝 userinfo、query 和 fragment。不能为方便测试放宽非-loopback HTTP。

Focused mock suite 可单独运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml webdav -- --nocapture
```

WebDAV automated tests 必须只绑定 loopback、使用专用远端路径、fake clock/store、合成凭据，并覆盖 fresh、legacy activation、ready、所有 ambiguous hard-stop、local-only、remote-only、双方更新、tombstone/delete、concurrent branches/conflict copy、strong/weak/missing ETag、conditional PUT precondition、CAS race、partial publication、exact ack/later edit 与 deadline。禁止连接用户已配置目录、真实凭据库或真实用户数据库。

当前 production-v2 验证包含 synthetic/pure unit/fake、v2 transport loopback wire，以及 fresh v2 engine bootstrap/exact acknowledgement loopback。Legacy activation、全部 ambiguous hard-stop、CAS exhaustion、crash before/after CAS、concurrent local edit 与 ancestry bound 的完整 engine-loopback 矩阵仍未覆盖。**没有执行真实 Nextcloud、ownCloud、Apache mod_dav 或其他第三方 WebDAV 服务测试，也没有执行 production Tauri desktop smoke**。文档、PR 或 release note 不得把 loopback suite 写成真实服务兼容性证明。

## 11. 国际化开发

- 所有用户可见前端文案应同时加入 `zh.json` 和 `en.json`。
- 避免通过 `language === "zh" ? ... : ...` 新增硬编码分支。
- Rust 托盘和错误当前未完整国际化；若扩展后端文案，优先设计稳定 error code，由前端翻译，而不是继续增加混合语言字符串。
- 日期/单位/方向应使用 locale 或翻译资源。
- 切换 UI 语言时同步考虑 HTML `lang` 属性和无障碍标签。

## 12. 权限、CSP 与受控系统操作

如果使用新 Tauri API：

1. 增加必要依赖并在 `main.rs` 注册；不要恢复通用 Shell plugin 来实现固定业务动作。
2. 在 [default capability](../src-tauri/capabilities/default.json) 只添加当前窗口和当前动作所需权限。
3. 如有 URL/路径，优先使用 Rust enum/allowlist 和 [paths.rs](../src-tauri/src/paths.rs) 派生，不把任意 URL、绝对路径或通用 opener 能力交给 WebView。
4. 更新 [架构安全边界](architecture.md#10-tauri-权限与安全边界)。
5. 执行安全影响评估，尤其关注 secret、剪贴板、文件系统、网络、窗口和 CSP。

当前 `withGlobalTauri` 为 false；CSP 只允许本地 script、精确 IPC scheme 和当前资源 scheme，没有 `unsafe-eval`。`style-src 'unsafe-inline'` 是 CodeMirror/React 运行时样式的已知必要例外，新增代码不能借此增加 inline executable script。启动 anti-flash/splash 代码必须继续通过本地 [boot.ts](../src/boot.ts) / [boot.css](../src/boot.css) 由 Vite 管理；修改后用 [SecurityConfig.test.ts](../src/test/SecurityConfig.test.ts)、生产 build 和真实 Tauri console/splash smoke 校验。

全局快速捕获只由 Rust 通过 global-shortcut plugin 注册和处理，因此当前 [default capability](../src-tauri/capabilities/default.json) 只保留读取/写入文本的 clipboard-manager 项；不要把生成的 global-shortcut permission schema 当成前端权限而添加 `global-shortcut:default`、register 或 unregister capability。任何未来把快捷键控制暴露给 WebView 的需求都必须重新评估窗口、CSP、privacy、最小 scope 和文档。

仓库 URL 与数据/导出目录当前分别由 `open_project_repository` 和 `open_trusted_directory` 控制。扩展受控打开时应新增小型 enum/固定常量并补静态/Rust 测试，而不是重新加入 `shell:allow-open`、广泛 regex 或返回路径给前端。

## 13. 发布开发

[release.yml](../.github/workflows/release.yml) 当前由 `v*` tag 负责正式发布，手动触发只做 dry-run build/校验并上传临时 artifact；普通 push/PR 验证由 [ci.yml](../.github/workflows/ci.yml) 负责：

- Windows x64：`windows-latest` 构建 MSI、NSIS、portable EXE。
- Windows ARM64：原生 `windows-11-arm` runner 使用 `aarch64-pc-windows-msvc` 构建 MSI、NSIS、portable EXE；产物在 staging 阶段加入 `windows-arm64` 文件名标记。
- macOS：`universal-apple-darwin` DMG，安装 x86_64/aarch64 targets 并用 `lipo -info` 校验 app executable。
- Linux amd64：`ubuntu-22.04` 构建 DEB、AppImage。
- Linux ARM64：原生 `ubuntu-22.04-arm` runner 使用 `aarch64-unknown-linux-gnu` 构建 DEB、AppImage；产物在 staging 阶段加入 `linux-arm64` 文件名标记。
- ARM job 使用 target-qualified `src-tauri/target/<triple>/release/bundle` 路径，并在上传前检查 Windows PE `0xAA64`、Linux DEB `Architecture=arm64` 和 AppImage AArch64 ELF。Linux ARM64 不使用未配置 sysroot 的 x64 交叉构建。
- 所有平台 job 必须成功，release job 才会生成 `SHA256SUMS`、检查完整 x64/ARM64 artifact set、拒绝 `.app` 目录 asset，并在 tag 发布时生成 artifact attestations 和 GitHub Release。手动 dry-run 不创建 tag 或 Release。

ARM hosted runner label 受 GitHub 账户/仓库计划和可用性影响；若不可用，应提供维护好的等价 ARM64 self-hosted runner，而不是静默回退到 x64 产物。打包成功不等于实际 ARM 设备上的安装、WebView、托盘、凭据库或快捷键运行 smoke 已完成。

发布能力变更时需同步核对：

- `package.json`、`Cargo.toml`、Cargo.lock、`tauri.conf.json`、Vite 注入 Settings/UI 版本，以及 tag；`npm run versions:check` 在 `SNIPVAULT_RELEASE_TAG` 存在时执行完整门禁。
- 平台 target 和产物名称。
- 图标格式；canonical source 是 [assets/app-icon.png](../assets/app-icon.png)，生成物在 [src-tauri/icons/](../src-tauri/icons/)，`npm run icons:check` 必须通过。
- 代码签名、公证、checksums 和 provenance。
- 是否引入 Tauri updater，以及 updater endpoint/public key/artifacts。
- README 平台承诺和开发文档。

当前 release workflow 已准备 Universal macOS、checksum 和 GitHub artifact attestation 路径，但仍没有应用内更新，也没有完整 Windows Authenticode、macOS Developer ID signing/notarization 或真实签名产物验证。不要在真实凭据和产物验证完成前宣称“已签名”或“updater 已启用”。

## 14. 文档同步门禁

任何功能设计或功能开发都必须在开始时做文档影响评估，在完成前落实文档修改。

### 14.1 开始前

回答以下问题：

1. 用户可见行为是否变化？
2. 模块职责、数据流、生命周期或权限是否变化？
3. IPC、TypeScript/Rust 类型、SQLite、Settings、导入导出或 WebDAV 协议是否变化？
4. 是否新增限制、风险、兼容性要求，或修复了现有限制？
5. README 中的安装、平台、配置、路径或功能表述是否受影响？

### 14.2 对应文档

| 影响 | 必须检查/更新 |
|---|---|
| 架构、IPC、数据、生命周期、权限 | [architecture.md](architecture.md) |
| 用户入口、交互和功能语义 | [feature-design.md](feature-design.md) |
| 开发命令、扩展规范、验证流程 | 本文 |
| 新限制/风险或已修复问题 | [known-limitations.md](known-limitations.md) |
| 用户安装、配置、平台和存储 | [README.md](../README.md) |

### 14.3 完成条件

- 代码和文档在同一任务/同一变更中完成。
- 未实现设计不得写进“当前行为”。
- 修复限制后更新或删除对应条目。
- 数据流或模块关系变化时同步 Mermaid。
- 检查源码链接仍指向存在的文件和符号。
- 若确认没有文档影响，最终交付说明必须写明“文档影响：无”及原因。
- 文档未同步时，不得宣告功能任务完成。

## 15. 变更验证清单

根据任务范围选择，不要声称未执行的检查已经通过。

### 15.1 通用

```bash
npm run format:check
npm run lint
npm run typecheck
npm run test:run
npm run build
npm run docs:check
npm run versions:check
npm run icons:check
git diff --check
git status --short
```

### 15.2 Rust / IPC / 数据

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

### 15.3 端到端人工验证

使用 `npm run tauri:dev` 验证受影响流程，例如：

- 新建、编辑、保存、切换和删除；对已有干净片段确认编辑器 Save 和命令面板 Save 均禁用，`Ctrl/Meta+S` 拦截浏览器默认行为但不产生 IPC、更新时间、revision 或 outbox；修改后通过按钮、快捷键和命令面板各保存一次，确认每次意图更新只新增一个 revision，新建带标题草稿仍可创建。
- 搜索、语言/收藏筛选、标签、独立的“最近更新 / 最近使用”排序及 Load More；确认默认“最近更新”使用稳定更新时间顺序，`recent` 在打开/复制成功后会重新排序，而未使用项不会被误作相关度结果。
- 命令面板的 `Ctrl/Cmd+K`、初始焦点、过滤、Arrow/Home/End/Enter/Escape、disabled 项、Settings/Promise Dialog 模态优先级与关闭后的焦点恢复。
- 批量选择的 200 项可见边界、select-loaded/clear、收藏/取消收藏/删除、筛选或排序切换清空选择，以及当前 dirty 片段包含在批量操作时的 Save/Discard/Cancel guard。
- 在隔离的临时数据目录、无真实 WebDAV/凭据且只使用非敏感剪贴板文本的环境中，验证全局快捷键注册成功或冲突 fallback、托盘快速捕获、隐藏/最小化窗口时的反馈、recent usage 更新，以及事件/日志不含剪贴板文本或标题。
- 对同一示例片段执行多次保存：从主编辑器打开 History，确认只创建或复用一个独立原生窗口，新创建窗口约为 1280×760 且不会压倒主窗口，紧凑时间线选中态保持中性但有清晰 accent rail/边框，右侧低权重 review command/context band 和唯一 code stage 不出现 document 级嵌套纵向滚动。检查 live 预览只有一个只读 editor-chrome header：标题只显示一次，language badge、最多两个 passive tag chip 与 `+N` 摘要、favorite/non-favorite 静态状态都可读且不可交互；分别检查无 tag、多个 tag、长标题和长 tag 的截断/悬停完整值，不得产生外层横向滚动。检查分页元数据、同一分钟内连续保存两个实际修改后的秒级本地化时间与悬停完整时间、live 版本的语法高亮预览、equal/insert/delete/replace/空行/长行/末尾换行的两路逐行比较和行号；replace 必须在两侧均显示 `~` 与中性 source surface、accent rail/gutter，摘要将其独占计为“修改”，而仅未配对 insert/delete 分别计入 `+新增`/`−删除` 并保持绿色/红色状态样式。代码不自动换行，新增/删除 source row 维持中性 editor surface、只以等宽 status rail 和低强度 gutter 区分，正常桌面宽度（至少 1200px）没有布局造成的外层横向滚动，双 live pane 只同步垂直滚动，只有实际超出单个 source pane 的长行可在该 pane 内横向滚动。验证 comparison stage 只显示一次标题且仅保留一个 `h2`，内部工具栏只在有内容时显示比较摘要和窄宽度 pane 切换控件；并在 accessibility tree 或 screen reader 中确认装饰 `~`/`+`/`−` marker/行号不会重复朗读、替换两侧真实 source row 分别提供本地化的之前版本/所选版本修改提示，未配对新增/删除行才提供相应方向提示。将窗口缩到 1000–1199px，确认只显示一个 pane、Baseline / Selected 原生按钮组可用键盘访问且默认 Selected，切换不重复 comparison IPC 或重新计算，并保留已加载内容/滚动状态。快速切换主窗口的目标片段、历史选择项或比较目标，确认旧异步结果不会覆盖新 generation；超出本地 diff 限制的可丢弃大文本快速退回带行号的完整并排源代码。live/tombstone 与 tombstone/tombstone 不得伪造正文，tombstone/current head 的恢复入口禁用。对历史 live revision 请求恢复时，确认主窗口被显示/聚焦；仅同一 target 的 dirty editor 触发 Save/Discard/Cancel guard，无关 dirty 草稿保留；确认后产生新的 current descendant 而非改写旧 revision，成功才隐藏历史窗口且不自动同步。关闭历史窗口应隐藏以便复用；主窗口最小化到托盘时历史窗口也隐藏，主窗口实际退出不遗留 child。该流程不配置或访问真实 WebDAV。
- 启用/切换 daily、weekly 与 7/30/90 本地 snapshot policy，并在受控向导手动创建 checkpoint；修改可丢弃的隔离数据后确认恢复会先产生 emergency checkpoint、设置和 OS credentials 未变、界面正确 reload，scheduled sync latch 生效且只有工具栏/设置/托盘的成功手动同步可解除。不得以真实用户 DB、真实凭据或任意文件路径执行此验证。
- 在没有 WebDAV URL 的隔离窗口和可控 tray/background 路径检查一条脱敏 notification 是否准确落入 inbox；检查未读徽标、已读、关闭、retry、默认 50/最多 100 列表和 failure/busy 文案，确认没有 URL、用户名、secret、路径、正文、revision ID 或原始远端错误。background 反馈必须保持非模态。
- 主题、语言、设置保存和重启恢复。
- 导入/导出和打开目录。
- 托盘、关闭、第二实例和自启动。
- WebDAV 需使用专用测试目录，覆盖本地独有、远端独有、双方更新和删除场景。

### 15.4 文档

- 运行 `npm run docs:check` 检查内部链接、源码相对链接和 Markdown anchor。
- 在支持 Mermaid 的 Markdown 预览中检查图表；当前脚本不渲染或语义验证 Mermaid。
- 确认当前行为与已知限制没有混写。
- 确认 README、CLAUDE.md 和 `docs/` 对同一事实表述一致。
