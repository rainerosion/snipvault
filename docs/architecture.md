# SnipVault 架构设计

> 本文描述 v2.1.3 源码截至 2026-08-11 的当前架构。已发现但尚未修复的问题集中记录在 [已知限制](known-limitations.md)。

## 1. 系统定位与边界

SnipVault 是一个本地优先的桌面代码片段管理器。应用的主要业务数据保存在本地 SQLite 中，网络仅用于用户主动配置的 WebDAV 同步。

| 层级 | 当前技术与职责 |
|---|---|
| 桌面运行时 | Tauri 2；创建 WebView 窗口、注册 IPC、托盘、单实例和系统插件 |
| 前端 | React 19 + TypeScript + Vite；负责界面、交互和运行时状态 |
| 编辑器 | CodeMirror 6 + `@uiw/react-codemirror`；语法解析、编辑、选区和滚动 |
| 后端 | Rust；负责 IPC、数据库、设置文件、路径、WebDAV 和系统生命周期 |
| 本地数据 | `rusqlite` bundled SQLite；片段和同步历史 |
| 设置 | Rust 内存缓存 + 校验、临时文件/备份替换的无 secret `settings.json` |
| 凭据 | `keyring` 抽象；生产环境使用平台凭据库，测试使用内存/失败 fake |
| 远端同步 | `reqwest::blocking` + WebDAV；进程级互斥、v2 marker/manifest、不可变 revision objects 与条件发布 |
| 发布 | GitHub Actions 构建 Windows、macOS、Linux 产物并创建 GitHub Release |

主要配置来源：

- [package.json](../package.json)
- [Cargo.toml](../src-tauri/Cargo.toml)
- [tauri.conf.json](../src-tauri/tauri.conf.json)
- [default capability](../src-tauri/capabilities/default.json)

## 2. 总体组件关系

```mermaid
flowchart LR
    subgraph WEBVIEW[WebView / React]
        UI[React Components]
        APP[App.tsx<br/>业务与同步协调]
        SETTINGS_PROVIDER[SettingsProvider<br/>权威设置状态]
        SNIPPETS[useSnippets]
        MODAL[ModalSurface<br/>模态栈与焦点所有权]
        EDITOR[SnippetEditor<br/>CodeMirror + Canvas MiniMap]
        SYNTAX_HIGHLIGHT[syntaxHighlight.ts<br/>共享语法范围适配器]
        LANGUAGE_META[languages.ts<br/>轻量语言元数据]
        LANGUAGE_EXT[languageExtensions.ts<br/>编辑器 parser / stream factory]
        UI --> APP
        SETTINGS_PROVIDER --> APP
        SETTINGS_PROVIDER --> UI
        APP --> SNIPPETS
        APP --> EDITOR
        UI --> MODAL
        EDITOR --> LANGUAGE_META
        EDITOR --> LANGUAGE_EXT
        EDITOR --> SYNTAX_HIGHLIGHT
        SYNTAX_HIGHLIGHT --> LANGUAGE_EXT
        LANGUAGE_EXT --> LANGUAGE_META
    end

    subgraph TAURI[Tauri / Rust]
        MAIN[main.rs<br/>启动与窗口生命周期]
        TRAY[tray.rs<br/>托盘所有权与事件]
        SYNC[sync.rs<br/>同步来源事件与调度]
        CMD[commands.rs<br/>IPC 适配]
        ERROR[error.rs<br/>稳定命令错误]
        DB[db.rs<br/>SQLite]
        SETTINGS[settings.rs<br/>设置 / DTO / 恢复]
        CREDS[credentials.rs<br/>平台凭据抽象]
        PATHS[paths.rs<br/>路径决策]
        DAV[webdav.rs<br/>稳定 facade / 进程锁]
        DAV_ENGINE_V1[webdav/engine.rs<br/>legacy v1 engine / tests]
        DAV_ENGINE_V2[webdav/engine_v2.rs<br/>production v2 bootstrap / CAS]
        DAV_TRANSPORT[webdav/transport.rs<br/>认证 / HTTP / retry / conditional PUT]
        DAV_STORE[webdav/store.rs<br/>短时 SQLite adapter]
        DAV_PROTOCOL[webdav/protocol.rs<br/>v1/v2 DTO / URL / 边界]
        OPEN[opener<br/>受控打开]
        MAIN --> TRAY
        MAIN --> SYNC
        MAIN --> CMD
        TRAY --> SYNC
        TRAY --> CMD
        CMD --> ERROR
        CMD --> DB
        CMD --> SETTINGS
        CMD --> CREDS
        CMD --> DAV
        CMD --> OPEN
        SYNC --> SETTINGS
        SYNC --> DAV
        DAV --> DAV_ENGINE_V2
        DAV --> DAV_TRANSPORT
        DAV --> DAV_STORE
        DAV_ENGINE_V1 --> DAV_PROTOCOL
        DAV_ENGINE_V1 --> DAV_TRANSPORT
        DAV_ENGINE_V1 --> DAV_STORE
        DAV_ENGINE_V2 --> DAV_PROTOCOL
        DAV_ENGINE_V2 --> DAV_TRANSPORT
        DAV_ENGINE_V2 --> DAV_STORE
        DAV_TRANSPORT --> DAV_PROTOCOL
        DAV_STORE --> DB
        DAV --> SETTINGS
        SETTINGS --> CREDS
        DB --> PATHS
        SETTINGS --> PATHS
        OPEN --> PATHS
    end

    SNIPPETS -->|invoke| CMD
    SETTINGS_PROVIDER -->|invoke| CMD
    APP -->|Window.listen| TRAY
    APP -->|Window.listen| SYNC
    TRAY -->|open-settings / autostart-toggled| APP
    SYNC -->|typed sync-complete| APP
    DB --> SQLITE[(snippets.db)]
    SETTINGS --> JSON[(settings.json<br/>不含 secret)]
    CREDS --> KEYRING[(Windows Credential Manager<br/>macOS Keychain / Linux Secret Service)]
    DAV_TRANSPORT --> REMOTE[(WebDAV)]
```

前端和后端没有共享的代码生成协议层。TypeScript 类型、IPC 参数和 Rust 结构需要开发者手动保持一致。

## 3. 前端架构

### 3.1 入口和 Context

[main.tsx](../src/main.tsx) 负责：

- 创建 React Root。
- 以单次 `getBootSettings()` promise 同时初始化根级 `SettingsProvider`、主题和语言，避免为三个入口重复读取设置。
- 在 `SettingsProvider` 之内提供运行时 `ThemeContext` 和 [LanguageProvider](../src/context/LanguageProvider.tsx)；设置 Context 本身不依赖主题或语言 Context，因此没有 provider cycle。
- 使用 `localStorage` 缓存主题偏好和有效主题，降低启动时主题闪烁。
- 同步 `data-theme` 到 `documentElement` 和 `#root`，并由 `LanguageProvider` 把有效语言同步为 HTML `lang="zh-CN"` 或 `lang="en"`。
- 上报启动性能阶段，移除 HTML splash，并调用 `frontend_ready`。

主题分为两层：

- **持久偏好**：`dark`、`light`、`system`，来自后端设置。
- **当前有效主题**：`dark` 或 `light`，由偏好和系统主题共同解析。

[LanguageContext.tsx](../src/context/LanguageContext.tsx) 定义运行时 `zh` / `en` 契约；[LanguageProvider.tsx](../src/context/LanguageProvider.tsx) 负责异步启动语言、i18next 和文档 `lang` 同步，使 provider 可脱离 `main.tsx` 的启动副作用测试。

### 3.2 根状态和流程编排

[App.tsx](../src/App.tsx) 是当前前端的应用控制器，集中管理：

- 当前选中片段、是否新建、表单和原始表单快照。
- 保存状态与脏数据检测。
- 搜索词、语言筛选、收藏筛选和分页请求状态。
- 设置模态层、同步状态和全局 Dialog。
- `Ctrl/Meta+N/S/E` 快捷键。
- 自定义文本右键菜单和剪贴板操作。
- `sync-complete`、`open-settings`、`autostart-toggled` 窗口事件。
- 编辑器模块的 lazy load 和空闲预加载。

项目没有 Redux、Zustand、React Query 或前端路由。状态由组件局部 state、Context 和 Tauri 后端持久化共同组成。

### 3.3 Hook 与 IPC 边界

[useSnippets.ts](../src/hooks/useSnippets.ts) 封装：

- 分页摘要查询、按 ID 懒加载完整片段、创建、更新、删除、收藏。
- JSON 字符串导出、文件导出和导入。
- 本地 `SnippetSummary[]`、总数/cursor、独立首屏与 Load More loading/error、标签元数据状态。
- 以 generation + 查询 key/cursor 抑制迟到的旧查询或旧追加响应；筛选变化总是从第一页重新请求。

[useSettings.ts](../src/hooks/useSettings.ts) 定义根级 `SettingsProvider`，封装：

- 一份由所有消费者共享的权威设置状态；`App`、`SettingsPanel` 和编辑器换行入口不再各自维护独立 Hook state。
- 设置读取、非敏感设置保存、显式 secret action、WebDAV 同步、同步历史、系统主题与系统语言 API。
- 读取协议 `SettingsView` 只含非敏感设置、`webdav_secret_configured` 和安全恢复状态；保存协议由 `SettingsDraft` 与 `SecretAction = Keep | Replace(value) | Clear` 组成，持久 secret 不进入 WebView。
- `SettingsDraft` 显式选择用户可编辑的非敏感字段，把后端维护的 `last_sync_at`、凭据状态和恢复状态排除在外。
- reload/save request ID，防止较旧 reload 覆盖较新的成功保存；并以 active counter 统一多个同步请求的 `syncing` 状态。
- `SyncCompletionEvent` 协议：来源为 `toolbar | settings | tray | background`，状态为 `result | error | busy`，携带 `SyncResult` 或结构化 `CommandError`。
- 可注入 `SettingsApi` 和初始设置，供组件测试复用同一生产状态路径。

[useSnippets.ts](../src/hooks/useSnippets.ts) 仍维护片段列表的局部 Hook 状态。前端没有 Redux、Zustand 或 React Query；持久事实源仍是 Rust 设置缓存、`settings.json` 和 SQLite，但设置在 React 运行时只有一个权威 provider 实例。

### 3.4 组件职责

| 组件 | 当前职责 |
|---|---|
| [Titlebar.tsx](../src/components/Titlebar.tsx) | 自定义无边框标题栏、拖动、最小化、最大化/还原、关闭 |
| [Toolbar.tsx](../src/components/Toolbar.tsx) | 搜索、语言/收藏过滤、新建、导入导出、同步、主题临时切换、设置入口 |
| [Sidebar.tsx](../src/components/Sidebar.tsx) | 左侧列表容器 |
| [SnippetList.tsx](../src/components/SnippetList.tsx) | 语义列表项、独立选择/收藏/删除按钮、相对时间和代码预览 |
| [SnippetEditor.tsx](../src/components/SnippetEditor.tsx) | 表单、可访问标签 combobox、CodeMirror、主题高亮、复制、Canvas minimap |
| [languageExtensions.ts](../src/components/languageExtensions.ts) | 编辑器专用的 parser-backed、StreamLanguage 和 plaintext 扩展分类/factory |
| [Settings.tsx](../src/components/Settings.tsx) | 通用设置、WebDAV、同步历史和关于信息 |
| [ModalSurface.tsx](../src/components/ModalSurface.tsx) | 共享模态栈、topmost Tab/Escape、背景 inert/ARIA、初始与恢复焦点 |
| [Dialog.tsx](../src/components/Dialog.tsx) | 基于共享模态 surface 的 Promise 化 `alert`、`confirm`、`ask` 对话框 |

[ModalSurface.tsx](../src/components/ModalSurface.tsx) 维护模块级模态栈和唯一 capture-phase `keydown` listener。Settings panel 与 Promise Dialog 都通过它注册：只有栈顶处理 Tab/Escape；打开时按显式初始目标、标记目标、首个可聚焦元素、容器的顺序决定焦点；祖先分支的背景 sibling 临时设为 `inert` 与 `aria-hidden`；关闭后恢复先前状态和触发器焦点。嵌套 Settings Dialog 因此不会与外层面板建立竞争的 document trap，且保留原有 Save/Discard/Cancel 关闭 guard。

### 3.5 CodeMirror 和 MiniMap

编辑器实际 DOM 外层为：

```text
.cm-editor-split
├── .cm-main-pane
│   └── .snippet-codemirror
└── .minimap-wrap
```

CodeMirror 的语言扩展由 [languageExtensions.ts](../src/components/languageExtensions.ts) 隔离：它从轻量 [languages.ts](../src/utils/languages.ts) 的 `LanguageId` 建立 exhaustively typed 分类和 factory；元数据文件不导入编辑器包。`buildMainExtensions()` 继续负责换行、GitHub 主题、`EditorView.theme()` 和 `HighlightStyle`，并先注册项目复合 highlighter、后注册 UIW GitHub 主题，以确保项目复合样式是编辑器中实际生效的 token 色彩来源。[syntaxHighlight.ts](../src/components/syntaxHighlight.ts) 使用同一语言 factory、受限 `ensureSyntaxTree()` 和共享 `HighlightStyle` 导出有序 token 范围；[SnippetEditor.tsx](../src/components/SnippetEditor.tsx) 的 `MiniMap` 再读取编辑区实际计算的 class 前景色绘制 Canvas。因此编辑区和 codeglance 共用语法语义与最终颜色，解析未完成、无 token 与 plaintext 区域统一使用编辑器默认前景色；MiniMap 仍只负责 Canvas 几何、主滚动同步和 viewport 拖拽。

语言 factory 分三类：官方/维护包提供的 parser-backed 扩展；基于 `@codemirror/legacy-modes` 的 `StreamLanguage` 语法着色；以及 plaintext 的显式空扩展。Stream mode 只提供 token stream 高亮，不是完整 Lezer parser，不能假定具备 parser-backed 折叠、结构选择或语言服务语义。

CodeMirror 的编辑 DOM 由库管理，但当前实现并不依靠 Shadow DOM 隔离。选择、光标、滚动和 token 颜色通过 CodeMirror 扩展配置更稳定；全局 CSS 仍负责布局和部分 `.cm-*` 规则。`EditorView.scrollDOM` 仍是唯一主滚动源；自研 minimap/canvas/viewport 对辅助技术隐藏，不取代可键盘滚动的 CodeMirror 编辑器。

## 4. 后端架构

### 4.1 模块边界

| 模块 | 当前职责 |
|---|---|
| [main.rs](../src-tauri/src/main.rs) | Tauri Builder、插件、单实例、窗口显示/关闭、worker 启动和命令注册 |
| [tray.rs](../src-tauri/src/tray.rs) | 托盘句柄、菜单构造/刷新、菜单与图标事件、窗口唤醒和 `open-settings` / `autostart-toggled` 通知 |
| [sync.rs](../src-tauri/src/sync.rs) | 来源标记的同步完成事件、托盘同步适配、自动同步 scheduler、busy 快速重试和失败退避 |
| [commands.rs](../src-tauri/src/commands.rs) | `#[tauri::command]` IPC 边界、后端时间戳、设置/凭据/自启动事务、受控 URL/目录打开、托盘状态刷新、导出文件、内部错误映射和启动打点 |
| [error.rs](../src-tauri/src/error.rs) | 可序列化 `CommandError`、稳定错误码、安全 fallback、retryable 与可选安全 details |
| [db.rs](../src-tauri/src/db.rs) | SQLite v4 逐版本迁移、通用升级前备份/失败恢复、严格解码、FTS5、revision head/tombstone/durable revision objects/outbox/remote state、分页摘要/详情、原子 mutation/import、v2 同步 snapshot/validated-plan/exact-ack seams、同步历史 |
| [settings.rs](../src-tauri/src/settings.rs) | 无 secret `Settings`、`SettingsView` / `SettingsInput`、显式 secret action、旧 JSON 迁移、损坏文件恢复和原子持久化 |
| [credentials.rs](../src-tauri/src/credentials.rs) | 可注入 `CredentialStore`、稳定 service/account、平台 keyring 实现和测试内存 fake |
| [paths.rs](../src-tauri/src/paths.rs) | 安装模式判断、数据库/设置/导出路径 |
| [webdav.rs](../src-tauri/src/webdav.rs) | 稳定同步 facade、`SyncResult` / structured native failure、进程级互斥、设置/凭据装配，以及 v2 engine 调用与 `last_sync_at` 提交 |
| [webdav/protocol.rs](../src-tauri/src/webdav/protocol.rs) | 显式 v1/v2 manifest、protocol marker、immutable revision DTO，ID/时间/hash/大小校验，以及安全 URL/path 构造 |
| [webdav/transport.rs](../src-tauri/src/webdav/transport.rs) | 可注入 `RemoteTransport` / `Clock`、reqwest 认证、有界响应读取、strong ETag 校验、conditional PUT 与 retry/deadline |
| [webdav/engine_v2.rs](../src-tauri/src/webdav/engine_v2.rs) | 当前生产同步状态机：fresh/v1/v2 bootstrap、revision ancestry reconcile、immutable object publication、manifest CAS、marker activation、publication verification 与 exact acknowledgement |
| [webdav/engine.rs](../src-tauri/src/webdav/engine.rs) | 保留的 v1 reconcile 实现与相关测试辅助；production facade 不再调用它 |
| [webdav/store.rs](../src-tauri/src/webdav/store.rs) | 可注入 store traits 与生产 SQLite adapter；snapshot、validated remote apply 与 published-revision commit 每次独立获取并释放数据库 mutex |
| [webdav/error.rs](../src-tauri/src/webdav/error.rs) | 内部同步错误类别、安全静态诊断与 retryable 元数据 |
| [lib.rs](../src-tauri/src/lib.rs) | 声明当前有效后端模块 |

当前真实的片段协议模型是 [db.rs](../src-tauri/src/db.rs) 中的 `Snippet`；未导出、未引用的旧 `models.rs` 已删除。

### 4.2 全局状态模型

后端使用 `once_cell` 单例：

- 数据库：`OnceCell<Mutex<Connection>>`
- 设置：`OnceCell<Mutex<Settings>>`
- 数据目录和安装模式：`OnceCell`
- 托盘句柄：由 [tray.rs](../src-tauri/src/tray.rs) 持有的 `OnceCell<Arc<Mutex<Option<TrayIcon>>>>`
- 启动计时：`OnceCell<Instant>` + `AtomicBool`
- 同步协调：`Lazy<Mutex<()>>`，覆盖工具栏、设置、托盘和自动同步完整流程

SQLite 只有一个连接，所有数据库操作通过 `with_db()` / `with_db_mut()` 和 Mutex 串行执行。v2 同步引擎只通过 `V2SyncStore` 调用短时 snapshot、validated remote-plan apply 和 published-revision commit；每次调用返回前已释放连接 guard，随后才可能执行 HTTP，因此网络请求不会持有 DB mutex。同一进程的完整 WebDAV 同步由独立 mutex 排他执行；不同设备依靠远端 manifest 的 strong ETag 与 conditional PUT 进行乐观并发控制。

## 5. 启动、窗口和托盘生命周期

```mermaid
sequenceDiagram
    participant OS as OS / 第二实例
    participant Main as Rust main.rs
    participant DB as DB & Settings
    participant Web as React main.tsx
    participant Win as 主窗口

    Main->>Main: 注册 single-instance / clipboard / autostart
    Main->>DB: 后台预热数据库和设置；迁移旧凭据或恢复损坏配置
    Main->>DB: 同步读取设置以创建托盘
    Main->>Win: 主窗口保持 visible=false
    alt 带 --minimized
        Main->>Win: 保持隐藏
    else 正常启动
        Main->>Main: 启动 2.5 秒显示兜底
        Web->>Main: boot_mark(main_eval_start)
        Main->>Win: show + unminimize + focus
        Web->>Web: React render + 两帧后移除 splash
        Web->>Main: frontend_ready
    end
    OS->>Main: 再次启动应用
    Main->>Win: 显示并聚焦既有窗口
```

### 5.1 窗口

[tauri.conf.json](../src-tauri/tauri.conf.json) 定义主窗口：

- label：`main`
- 默认 1200×800，最小 700×500
- `decorations: false`
- `visible: false`

前端尽早发出 `boot_mark("main_eval_start")`，Rust 收到后显示窗口；2.5 秒线程是前端未发信号时的兜底。

### 5.2 单实例和自启动

- `tauri-plugin-single-instance` 阻止创建第二个业务实例，第二次启动只唤醒既有窗口。
- `tauri-plugin-autostart` 注册开机自启，附加 `--minimized` 参数。
- `--minimized` 启动时主窗口保持隐藏，用户通过托盘或第二次启动唤醒。

### 5.3 托盘

Rust 托盘菜单当前固定为中文：

1. 打开灵藏 SnipVault
2. 立即同步
3. 设置
4. 开机自启（复选项）
5. 退出

左键点击托盘图标会显示主窗口。托盘同步在线程中运行，完成后由 [sync.rs](../src-tauri/src/sync.rs) emit 来源为 `tray` 的 typed `sync-complete`；托盘设置通过受支持的 `open-settings` 窗口事件打开 overlay，不再使用 `window.eval()` 或 WebView 全局函数。前端通过 Tauri 2 `Window.listen(...)` 注册 `sync-complete`、`open-settings` 和 `autostart-toggled`，并保存/清理每个 unlisten 回调。设置保存或托盘切换自启动后都会按 OS plugin 状态（失败时回退持久设置）重建复选菜单。

### 5.4 关闭和自动同步

窗口 `CloseRequested` 每次触发时读取最新 `minimize_to_tray`，因此保存后无需重启即可改变关闭行为。

应用始终启动一个持有克隆 `AppHandle` 的轻量自动同步 worker，每 15 秒读取当前设置。有效配置首次被观察时即在该轮尝试，因而启用后仍保持“下一次 poll window 内首次尝试”的语义；成功后恢复配置间隔，`sync_busy` 在 15 秒后快速重试，其他失败按 15、30、60 秒指数退避并封顶 15 分钟。关闭、URL 为空或相关 WebDAV/间隔/`credential_revision` 配置变化会重置 scheduler；scheduler 不保存 secret，实际同步开始时才从凭据库读取。所有尝试与手动入口共用同步 mutex；凭据迁移或补偿恢复未完成时读取被阻止，因此持久自动同步不会使用遗留明文。

```mermaid
sequenceDiagram
    participant Source as Toolbar / Settings / Tray / Worker
    participant Sync as sync.rs / IPC
    participant DAV as webdav facade / lock
    participant Engine as V2SyncEngine
    participant Store as V2SyncStore / SQLite
    participant Remote as RemoteTransport / WebDAV
    participant App as App.tsx
    participant State as Snippets + SettingsProvider

    alt toolbar 或 settings
        Source->>Sync: direct sync_upload
        Sync->>DAV: sync_merge()
        DAV->>Engine: run(deadline)
        Engine->>Store: load owned v4 snapshot / validated apply / exact commit
        Engine->>Remote: MKCOL / GET + strong ETag / immutable PUT / conditional PUT
        DAV-->>Source: SyncResult / CommandError
        Source->>App: source-tagged local completion
    else tray 或 background
        Source->>Sync: run_and_emit / scheduler attempt
        Sync->>DAV: sync_merge()
        DAV->>Engine: run(deadline)
        Engine->>Store: load owned v4 snapshot / validated apply / exact commit
        Engine->>Remote: MKCOL / GET + strong ETag / immutable PUT / conditional PUT
        DAV-->>Sync: result / structured failure / busy
        Sync-->>App: typed sync-complete event
    end
    App->>State: one refreshAfterSync()
    State->>State: reload snippets + settings + history
    State->>State: preserve dirty editor; refresh clean selection
    Note over App: Tray may show Dialog; background only updates aria-live status
```

只有成功结果触发权威刷新；busy/失败仍更新统一非模态状态。后台同步不会打开 modal，托盘同步可以显式提示用户。

## 6. IPC 契约

所有命令在 [main.rs](../src-tauri/src/main.rs) 的 `generate_handler!` 中注册，在 [commands.rs](../src-tauri/src/commands.rs) 实现。

| 域 | 命令 | 当前用途 |
|---|---|---|
| 片段 | `query_snippets` | 返回按 `(updated_at DESC, id DESC)` 排序的有界 `SnippetSummary` 页、下一 cursor 和过滤总数；组合搜索/语言/收藏/精确标签 |
| 片段 | `get_snippet` | 按 ID 返回包含完整正文的 `Snippet`，供选择后懒加载 |
| 片段 | `get_snippet_tags` | 返回去重标签元数据，不读取正文 |
| 片段 | `get_snippets` | 兼容/内部全量接口；主列表不使用 |
| 片段 | `create_snippet` | 后端生成创建/更新时间并返回最终 `Snippet` |
| 片段 | `update_snippet` | 在同一 transaction 中检查 `base_revision_id`、保留创建时间、写片段/FTS/head/outbox，并返回带新 `revision_id` 的最终 `Snippet`；过期基础返回 `stale_revision` |
| 片段 | `delete_snippet` | 原子删除 live row/FTS、写 tombstone head 和 durable outbox，并返回删除 revision head |
| 片段 | `search_snippets` | 兼容的全详情搜索入口；主界面使用 `query_snippets` |
| 片段 | `toggle_favorite` | 原子切换收藏并写新 revision/head/outbox，返回权威 `Snippet` |
| 导入导出 | `export_snippets` | 返回 pretty JSON 字符串 |
| 导入导出 | `export_snippets_to_file` | 写入导出目录，只返回是否写入 Downloads，不返回本地路径 |
| 导入导出 | `import_snippets` | 接受版本化 envelope 或旧顶层数组，先校验再在 transaction 中按 ID/时间合并并返回计数 |
| 设置 | `get_settings` | 返回脱敏 `SettingsView`：非敏感设置、是否已配置凭据和安全恢复状态 |
| 设置 | `save_settings` | 接受非敏感 `SettingsInput` 与 tagged `SecretAction`，协调凭据/autostart/设置提交与补偿，返回脱敏视图 |
| 自启动 | `set_auto_start` | 兼容的原生命令入口；当前 Settings UI 通过 `save_settings` 协调自启动 |
| 自启动 | `is_auto_start_enabled` | 查询 OS 自启动状态 |
| 受控打开 | `open_project_repository` | 只打开 Rust 固定的项目仓库 HTTPS URL |
| 受控打开 | `open_trusted_directory` | 只打开后端从 `paths.rs` 派生的数据或导出目录，不返回绝对路径 |
| 同步 | `sync_upload` | 调用 `sync_to_webdav()`，最终进入 `sync_merge()` |
| 同步 | `sync_download` | 调用 `sync_from_webdav()`，最终进入 `sync_merge()` |
| 同步 | `get_sync_versions` | 返回最近 20 条同步历史 |
| 系统 | `get_system_theme` | 返回主窗口的系统明暗主题 |
| 系统 | `get_system_locale` | 映射为 `zh` 或 `en` |
| 启动 | `frontend_ready` | 前端阶段通知并尝试显示窗口 |
| 启动 | `boot_mark` | 记录前端启动阶段；特定阶段显示窗口 |

`sync_upload` 和 `sync_download` 的命名保留了旧的方向概念，但当前两者都是同一个双向合并过程。

### 6.1 结构化命令错误

所有活跃的 fallible Tauri command 在 [commands.rs](../src-tauri/src/commands.rs) 边界返回 [error.rs](../src-tauri/src/error.rs) 的可序列化 `CommandError`：

```text
{
  code: validation | not_found | stale_revision | outbox_full | database |
        settings | network | sync_busy | sync_cas_conflict |
        sync_legacy_changed | import | export | autostart | credential |
        recovery | open | unknown,
  message: safe fallback string,
  retryable: boolean,
  details?: safe string map
}
```

字段沿用 Rust/TypeScript 的 snake_case 约定，`details` 缺失时不序列化。数据库与设置模块仍可保留各自内部错误；WebDAV 则以 `SyncError` / `SyncFailure` 将 busy 和 retryable 元数据一直保留到 command/event adapter。adapter 负责映射稳定 code、安全英文 fallback、retryable 和显式安全 details，只在原生侧记录内部原因。认证、授权、配置、远端格式/普通 4xx validation 是非 retryable；busy、transient network、deadline、retry-limit、收敛耗尽和本地成功标记持久化失败按其内部分类映射。任何公开映射都不会回显 WebDAV credential、完整敏感 URL、远端响应体、SQL 诊断或本地文件路径。

前端 [commandErrors.ts](../src/utils/commandErrors.ts) 校验 object 或 JSON string 形态，并按稳定 code 映射同步的中英文资源。未知 code、普通字符串、`Error` 和 malformed rejection 都降级为 `unknown`，不会把原始 rejection 文本直接显示给用户。

### 6.2 权威重载和编辑器协调

[useSnippets.ts](../src/hooks/useSnippets.ts) 的 `load()` 成功时返回同一份权威 `SnippetQueryResult`，失败时保留已有摘要列表、记录规范化错误并重新抛出。摘要不含完整 `content`，只含最多 768 UTF-8 bytes 的 `content_preview`；用户选择卡片后才通过 `get_snippet` 获取正文。首屏/刷新错误与空结果分离，追加错误保留已加载卡片并可重试；独立 append request/in-flight guard 防止同一 cursor 被重复并发加载。[App.tsx](../src/App.tsx) 的共享 reload/reconcile 流程在每次权威第一页加载后执行：干净的已选表单按 ID 重新读取并刷新到后端状态；dirty editor 永不被覆盖，并显示非模态“有新数据”状态；干净选择若已不存在则清空。保存请求原有的 request/target/form race guard 保留。

### 6.3 分页查询与详情 IPC 流

```mermaid
sequenceDiagram
    participant UI as Toolbar / Sidebar
    participant Hook as useSnippets
    participant Cmd as commands.rs
    participant DB as SQLite + FTS5
    participant Editor as App / Editor

    UI->>Hook: query / language / favorite 改变
    Hook->>Cmd: query_snippets(cursor=null, limit=100)
    Cmd->>DB: literal search + filters + stable cursor page
    DB-->>Hook: SnippetSummary[] + next_cursor + total
    Hook-->>UI: replace page if generation still current
    opt Load More
        UI->>Hook: loadMore()
        Hook->>Cmd: query_snippets(current cursor)
        Cmd-->>Hook: next summary page
        Hook-->>UI: append if generation/query/cursor still current
    end
    UI->>Cmd: get_snippet(id) after selection
    Cmd->>DB: strict full row decode
    DB-->>Editor: full Snippet body
    Note over Hook,Editor: List IPC never returns full content; dirty detail survives authoritative refresh
```

### 6.4 同步完成协议与前端协调

Rust [sync.rs](../src-tauri/src/sync.rs) 与 TypeScript [useSettings.ts](../src/hooks/useSettings.ts) 手动维护同一 event shape：

```text
{
  source: toolbar | settings | tray | background,
  status: result | error | busy,
  result?: SyncResult,
  error?: CommandError
}
```

工具栏和设置面板直接调用 IPC，由 `App` 在本地构造来源标记，避免 Rust event 与 command result 导致重复刷新或重复 Dialog。托盘与 worker 由 Rust emit `sync-complete`。所有成功来源都进入同一个 `refreshAfterSync()`，并行 reload 片段、共享设置和同步历史，再复用片段编辑器的 dirty/clean reconciliation；只有托盘允许 modal，后台只写入 `aria-live` 状态。

## 7. 数据模型和持久化

### 7.1 SQLite

```mermaid
erDiagram
    SNIPPETS {
        TEXT id PK
        TEXT title
        TEXT content
        TEXT language
        TEXT description
        TEXT tags_JSON
        INTEGER is_favorite
        TEXT created_at
        TEXT updated_at
    }

    SNIPPETS ||--|| SNIPPETS_FTS : "external-content rowid"
    SNIPPETS ||--o| SNIPPET_HEADS : "live head"

    SNIPPETS_FTS {
        TEXT title
        TEXT content
        TEXT description
        TEXT tags
    }

    SYNC_IDENTITY {
        INTEGER singleton PK
        TEXT device_id UK
        TEXT created_at
    }

    SNIPPET_HEADS {
        TEXT snippet_id PK
        TEXT revision_id UK
        TEXT parent_revision_id
        TEXT device_id
        TEXT content_hash
        TEXT revision_time
        INTEGER deleted
    }

    SNIPPET_HEADS ||--o| REVISION_OBJECTS : "current durable object"
    REVISION_OBJECTS ||--o| REVISION_OUTBOX : "pending publish copy"

    REVISION_OBJECTS {
        TEXT revision_id PK
        TEXT snippet_id
        TEXT parent_revision_id
        TEXT device_id
        TEXT content_hash
        TEXT revision_time
        INTEGER deleted
        TEXT origin
        TEXT payload_json
        INTEGER payload_bytes
        TEXT conflict_of
    }

    REVISION_OUTBOX {
        INTEGER sequence PK
        TEXT revision_id UK
        TEXT snippet_id
        TEXT parent_revision_id
        TEXT device_id
        TEXT content_hash
        TEXT revision_time
        INTEGER deleted
        TEXT operation_kind
        TEXT origin
        TEXT payload_json
        INTEGER payload_bytes
    }

    SYNC_REMOTE_STATE {
        TEXT remote_id PK
        INTEGER protocol_version
        TEXT manifest_etag
        TEXT manifest_hash
        INTEGER generation
        TEXT bootstrap_state
        TEXT last_success_at
    }

    SYNC_CONFLICTS {
        TEXT conflict_id PK
        TEXT source_snippet_id
        TEXT local_revision_id
        TEXT incoming_revision_id
        TEXT conflict_snippet_id UK
        TEXT detected_at
    }

    SYNC_VERSIONS {
        TEXT id PK
        TEXT synced_at
        TEXT direction
        INTEGER snippet_count
        INTEGER uploaded_count
        INTEGER downloaded_count
        INTEGER deleted_count
        INTEGER conflict_count
        INTEGER protocol_version
        INTEGER generation
        TEXT message
    }
```

`snippets.tags` 是 JSON 字符串数组，不是独立标签表。时间使用 RFC 3339 字符串；所有活跃行解码会严格检查必需列类型、0/1 boolean、标签 JSON 数组、字段边界、revision token 和时间，损坏数据显式失败，原生诊断不包含完整正文。`Snippet` 与 `SnippetSummary` 都向 WebView 暴露当前 `revision_id`；v2 wire 通过显式 `RevisionObjectV2` DTO 固定 revision 元数据和可选 live payload，避免本地 domain struct 演进静默改变远端格式。

每个本地 create/update/favorite/delete 和每个获胜 import item 都在单个 transaction 中同时更新 live `snippets`、FTS trigger、`snippet_heads`、`revision_objects`，并按需写入 `revision_outbox`。删除移除 live row 并把 head 改为 tombstone；head 因此不依赖 live row 外键。`revision_objects` 保存 canonical JSON、SHA-256、parent/device/time/deleted/origin 元数据，UPDATE trigger 禁止原地修改；outbox 只保存尚未发布确认的 pending copy，待处理上限为 10,000 条或 64 MiB，单 payload 受正文上限加固定余量约束。v2→v3 backfill 按 live row 的稳定字段生成 `legacy-<sha256>` head，不把历史库全部塞入 outbox；v3→v4 从 pending outbox、当前 live heads 和 tombstones 回填 durable objects。

v2 同步 seam 包括一致 snapshot、完整预验证的 remote plan no-echo apply、确定性 conflict copy/index，以及 exact revision-ID publish commit。snapshot 会携带当前 head/outbox 所需的 durable revision-object ancestry；remote apply 不写 outbox但会写入 durable object；publish commit 只删除明确列出的 outbox revision ID，不删除 `revision_objects`，所以较晚本地编辑不会被旧上传确认误清，已确认 ancestry 仍可继续参与后续发布/验证。`sync_remote_state` 与 apply/ack transaction 记录 protocol、strong ETag/hash、generation、bootstrap 和 last-success；这些边界现在由 production `V2SyncEngine` 使用。

`snippets_fts` 是 FTS5 external-content 索引，由 `snippets` 的 insert/update/delete trigger 与 CRUD、导入和 WebDAV merge 事务同步。v2 首选 trigram tokenizer；运行时不支持时记录 `unicode61` fallback，并对所有非空查询使用有界、转义的 LIKE 以保持子串语义。trigram 可用且规范化查询至少 3 字符时使用字面量化 MATCH；短查询使用 LIKE。搜索不做相关度排序，始终按 `updated_at DESC, id DESC` 稳定分页；`%`、`_`、引号和反斜线均按字面值处理，CJK 受测试覆盖。

`sync_versions` 每次成功同步写一条记录，随后只保留按 `synced_at DESC, id DESC` 排序的最近 20 条；v3 起记录上传、下载、删除、冲突、protocol version 和 manifest generation。当前 production v2 成功记录使用 protocol 2 和实际发布 generation。

数据库通过 `PRAGMA user_version` 维护 schema 版本，当前为 v4。`initialize_connection()` 严格按 v0→v1→v2→v3→v4 顺序迁移：v0→v1 创建业务表并保留“真正新库才 seed”的规则；v1→v2 严格扫描并建立 FTS；v2→v3 创建稳定 device identity、heads/outbox/remote/conflict side tables，扩展 history，并确定性回填 live heads；v3→v4 创建 `revision_objects` 并从 pending outbox、当前 live heads 和 tombstones 回填 durable immutable revision objects。任何既有磁盘 v0/v1/v2/v3 文件在第一步写入前都通过 SQLite online backup 创建和验证唯一 `pre-v4` 同级备份；链中任一步失败会 rollback、保留失败数据库副本并恢复/校验原始版本。新建库和重复打开 v4 不创建升级备份，未来 `user_version` 会在不写入的情况下拒绝。

### 7.2 设置、凭据与恢复模型

[settings.rs](../src-tauri/src/settings.rs) 的持久 `Settings` 不含 WebDAV secret：

| 类别 | 字段 |
|---|---|
| 通用 | `auto_start`, `minimize_to_tray`, `theme`, `language` |
| WebDAV 非敏感配置 | `webdav_url`, `webdav_username`, `webdav_auth_mode`, `webdav_timeout_secs` |
| 自动同步 | `auto_sync`, `sync_interval_minutes`, `last_sync_at` |
| 编辑器 | `editor_line_wrap` |
| 后端元数据 | `credential_revision`, `credential_recovery_status`, `settings_recovery_status` |

持久凭据由 [credentials.rs](../src-tauri/src/credentials.rs) 的 `CredentialStore` 管理。稳定身份为 service `cn.rainss.snipvault.webdav`、account `default`；`keyring` 的平台实现对应 Windows Credential Manager、macOS Keychain 和 Linux Secret Service。单元测试只注入内存/失败 fake，不访问开发机凭据库。

Rust 使用三个不同边界：

- `Settings`：后端内存和 JSON 的无 secret 模型。
- `SettingsView`：返回 WebView 的只读脱敏 DTO，包含非敏感字段、`webdav_secret_configured`、`credential_status` 和 `settings_recovery_status`。
- `SettingsInput` + `SecretAction`：WebView 保存时提交的非敏感候选值以及 `Keep`、`Replace(value)` 或 `Clear`；`last_sync_at` 继续由后端所有。

```mermaid
sequenceDiagram
    participant UI as Settings.tsx
    participant Cmd as commands.rs
    participant Store as CredentialStore
    participant Auto as OS Autostart
    participant File as settings.json

    UI->>Cmd: save_settings(SettingsInput, Keep/Replace/Clear)
    Cmd->>Cmd: 基于当前状态构造并校验候选值
    opt Replace 或 Clear
        Cmd->>Store: 快照旧 secret
        Cmd->>Store: 写入替换值或删除
    end
    opt auto_start 变化
        Cmd->>Auto: 应用候选状态
    end
    Cmd->>File: 原子持久化无 secret Settings
    alt 后续步骤失败
        Cmd->>Auto: 尝试恢复旧状态
        Cmd->>Store: 尝试恢复旧凭据
        alt 任一补偿失败
            Cmd-->>UI: recovery error + action_required 状态
        else 补偿成功
            Cmd-->>UI: 原步骤对应的安全错误
        end
    else 全部成功
        Cmd-->>UI: 脱敏 SettingsView
    end
```

设置 JSON 继续采用 candidate-before-commit：JSON 写入唯一临时文件并 `sync_all`，旧文件改名为 `.bak`，新文件安装成功后才更新内存。启动时：

- 发现旧 `webdav_password` 字段时，只通过兼容结构读取；先把非空 secret 写入凭据库，成功后才重写无 secret JSON。
- 迁移失败时保留遗留文件，不加载/暴露遗留 secret，标记 `migration_required` 并阻止凭据读取；用户必须在设置中 Replace 或 Clear 才能恢复。
- 主设置 JSON 损坏时先改名为唯一、非覆盖的隐藏 `.corrupt` 同级文件，再尝试有效 `.bak`；否则写入安全默认值。`SettingsView` 以 `backup_restored` 或 `defaults_loaded` 提示，并提供受控打开数据目录的操作。
- credential/autostart/设置提交的补偿失败会在当前进程标记 `compensation_required` 并阻止 Keep/同步，要求 Replace 或 Clear；该状态跨崩溃持久性仍有边界，见 [已知限制](known-limitations.md#42-凭据库和补偿恢复仍依赖平台与进程状态)。

### 7.3 实际数据路径

路径以 [paths.rs](../src-tauri/src/paths.rs) 为准：

| 场景 | 数据目录 |
|---|---|
| Windows portable/default | `dirs::data_dir()/SnipVault`，通常是 Roaming `%APPDATA%\SnipVault` |
| Windows 检测为安装版且 EXE 目录可写 | `<exe_dir>\data` |
| 安装目录不可写 | 回退到 `dirs::data_dir()/SnipVault` |
| macOS/Linux | 当前安装检测返回 portable，使用平台 `dirs::data_dir()/SnipVault` |

文件：

- `<data_dir>/snippets.db`
- `<data_dir>/settings.json`
- 导出优先写入 `<Downloads>/SnipVault`
- Downloads 不可写时回退 `<data_dir>/exports`

“portable”描述的是安装检测分支，不表示数据保存在 EXE 相邻目录。

## 8. 导入与导出架构

### 8.1 导出

数据库把完整片段包装为稳定版本化 envelope：

```json
{
  "format_id": "snipvault.snippets",
  "schema_version": 1,
  "app_version": "2.1.2",
  "exported_at": "RFC3339 timestamp",
  "snippets": []
}
```

文件导出以本地秒级时间生成 stem `snipvault-backup-YYYY-MM-DD_HH-MM-SS`，再通过 `OpenOptions::create_new(true)` 原子占用 `.json` 文件；冲突时依次尝试 `-1`、`-2`，因此不会覆盖同秒导出。后端只返回 `saved_in_downloads`，不把导出文件或目录的绝对路径发送到 WebView。用户选择打开目录时，前端调用 `open_trusted_directory("export")`，Rust 再从 [paths.rs](../src-tauri/src/paths.rs) 派生并打开受信目录。

### 8.2 导入

前端通过 HTML file input 读取 `.json`，先用 `JSON.parse()` 检查语法，再将原始文本交给 `import_snippets`。后端兼容当前版本化 envelope 和遗留顶层 `Snippet[]`。Envelope 的 format ID、schema version、应用版本非空性和 RFC 3339 导出时间会在任何数据库写入前验证；未知或 malformed 版本整体拒绝。随后校验总体积、条目数、重复 ID、字段边界和片段 RFC 3339 时间，再在单个 transaction 中合并：

- ID 不存在：插入。
- ID 已存在且导入记录时间较新：`ON CONFLICT DO UPDATE`。
- 其他：跳过。

返回 `input_count / inserted / updated / skipped`。FTS trigger 与 merge 在同一 SQLite transaction 中生效。

## 9. WebDAV 架构

### 9.1 远端布局与单向激活

```text
<base>/snipvault/
├── manifest.json                 # 激活后为 ManifestV2
├── protocol-v2.json              # v2 marker：version + vault_id
├── objects/
│   ├── <revision_uuid>.json      # immutable live revision
│   └── <revision_uuid>.json      # immutable tombstone revision
├── <legacy-snippet-id>.json      # v1 遗留 payload；保留但激活后忽略
└── ...
```

`manifest.json` 在相同路径从 v1 被条件替换为 v2；v2 manifest 保存 `version`、`vault_id`、单调递增 `generation` 和每个 snippet 的 head revision。`protocol-v2.json` 是升级判别 marker，不复制 generation/hash；`objects/<revision_uuid>.json` 保存带 parent、device、changed-at、deleted、content-hash、conflict metadata 和可选 live snippet 的不可变对象。

激活是**单向 v1→v2 cutover**：首次看到完整合法 v1 manifest 时，engine 读取并校验它引用的所有 v1 payload，把它们确定性映射为 legacy root revisions，再发布 v2 objects/manifest/marker。旧的逐片段 v1 JSON 不改名、不删除、不覆盖，激活后也不再作为事实源。应用没有 v2→v1 downgrade 或双写；旧版客户端不得继续对同一已激活目录同步。切换前应备份本地数据库与远端 `snipvault/` 目录。

Bootstrap 判定严格如下；除表中唯一的 interrupted-activation 恢复外，歧义状态 hard-stop，不猜测也不自动修复：

| Marker | Manifest | 行为 |
|---|---|---|
| 缺失 | 缺失 | fresh vault：以 `If-None-Match: *` 创建第一代 v2 manifest，随后创建 marker |
| 缺失 | 合法 v1 | legacy activation：要求 v1 manifest strong ETag，以 `If-Match` 条件替换后创建 marker |
| 存在 | 合法 v2，且 `vault_id` 一致 | ready：按 v2 generation/ancestry 合并并以 strong ETag CAS 发布下一代 |
| 缺失 | 合法 v2 | interrupted activation recovery：要求 strong ETag；如已有本地 vault identity 则必须匹配。把 manifest 作为 ready state 条件发布下一代，再以 `If-None-Match: *` 补建 marker |
| 存在 | 缺失或 v1 | hard-stop：marker/manifest 冲突 |
| 存在 | v2 但 `vault_id` 不同 | hard-stop：vault identity 冲突 |

此外，某个本地 remote identity 已提交过 v2 后，如果远端 manifest/marker 都消失或被替换成 v1，engine 也会 hard-stop；它不会把已激活 vault 当成 fresh/legacy 重建。

### 9.2 认证、HTTP 与服务器要求

支持：

- `Auto`：先发无认证请求；遇 Digest challenge 时尝试 Digest，失败或持续 401 时回退 Basic。
- `Basic`
- `Digest`
- `Bearer`：优先使用安全凭据库中的 secret，未配置时仍兼容使用 username 字段。
- `None`

生产传输位于 [transport.rs](../src-tauri/src/webdav/transport.rs)，使用 `MKCOL`、`GET` 与 `PUT`。Base URL 要求 HTTPS；仅本机测试允许 `http://localhost`、`http://127.0.0.1` 或 `http://[::1]`。URL 拒绝 userinfo、query 和 fragment；marker、manifest、objects 与旧 payload 路径都通过 URL API 构造。

v2 对 WebDAV 服务端有不可选的并发语义要求：

- GET v1/v2 `manifest.json` 必须返回可用的 **strong ETag**；缺失、格式错误或 `W/` weak ETag 都会 hard-stop。
- 服务必须对既有 manifest 支持 `If-Match: <strong-etag>` 条件 PUT，对 fresh manifest 和 marker 支持 `If-None-Match: *`；412 表示 CAS 竞争并触发有界重试，428/拒绝条件写表示服务器不兼容并失败。
- PUT 响应没有 strong ETag 时不会直接被信任；engine 重读精确 manifest bytes/hash、marker 和 GET 响应 strong ETag 后才提交成功。
- Revision object 以 UUID 路径不可变发布。丢失 PUT 响应时只 GET 并比对精确对象，不盲目重写；同 revision ID 内容不同视为 validation failure。

GET/PUT 的 transport retry、request timeout 与完整 5 分钟同步 deadline 都有界；认证、授权、validation 和普通 4xx 不盲重试。`Clock`、`RemoteTransport` 与 `V2SyncStore` 可在测试中注入；production store 不在 HTTP 期间持有 SQLite mutex。

### 9.3 当前同步顺序

```mermaid
flowchart TD
    A[获取进程级同步锁] --> B[读取并校验 Settings / 凭据 / URL]
    B --> C[建立 5 分钟总 deadline]
    C --> D[MKCOL snipvault/ 与 objects/]
    D --> E[短时读取 owned SQLite snapshot]
    E --> F[GET protocol-v2.json 与 manifest.json]
    F --> G{Bootstrap 状态?}
    G -- Fresh --> H[空 remote revision graph]
    G -- Legacy v1 --> I[校验 strong ETag 与所有 legacy payload<br/>生成确定性 legacy root revisions]
    G -- Ready v2 --> J[校验 marker/vault/strong ETag<br/>遍历并验证 head ancestry]
    G -- Interrupted v2 --> J
    G -- 其他歧义/不完整状态 --> X[hard-stop validation failure]
    H --> K[按 revision ancestry reconcile]
    I --> K
    J --> K
    K --> L[validated remote plan 单事务应用<br/>remote revisions 不 echo 到 outbox]
    L --> M{本地 expected head 是否竞争?}
    M -- 是且少于 4 个 CAS round --> F
    M -- 是且耗尽 --> Y[retryable CAS failure]
    M -- 否 --> N[重读 snapshot 并发布 immutable revision objects]
    N --> O[以 If-Match 或 If-None-Match:*<br/>条件 PUT 下一代 v2 manifest]
    O -- 412 且有余量 --> F
    O -- 条件写不受支持 --> Z[server-incompatible failure]
    O -- 成功 --> P[以 If-None-Match:* 创建 protocol-v2 marker]
    P --> Q[重读精确 manifest bytes/hash/strong ETag 与 marker]
    Q --> R{验证一致?}
    R -- 否 --> Y
    R -- 是 --> S[原子提交 remote state/history<br/>仅 exact revision IDs ack]
    S --> T[持久化 last_sync_at 并返回 SyncResult]
```

最多执行 4 个 CAS rounds，所有步骤共用 5 分钟 deadline。不可变对象先于 manifest，marker 后于 v2 manifest；因此 marker 一旦可见即表示 v2 manifest 已发布。任何对象、manifest、marker、远端验证、本地 apply/commit/history 或 `last_sync_at` 失败均不得返回成功。HTTP 与 SQLite 不能形成跨系统原子事务；已发布但尚未本地确认的对象由 durable outbox 与后续幂等调用继续收敛。

### 9.4 Reconcile、删除与冲突语义

- 同一 revision：直接收敛并只确认仍在 pending outbox 中的确切 revision ID。
- 一端 head 是另一端祖先：后代获胜；远端后代通过带 expected-local-revision guard 的 validated plan 应用，本地后代发布为新 remote head。
- 无祖先关系的并发分支：已发布的远端 original 确定性保留为 canonical head；本地落败 revision 仍作为 immutable object 发布，并在 manifest durable 后按 exact revision ID 确认。
- 远端覆盖本地且落败分支仍是 live 内容时，本地事务用 source snippet + 两端 revision 生成幂等 conflict copy/index；本地 tombstone 没有正文可复制。当前没有属性级 semantic merge，也没有完整的冲突比较、解决或归档 UI。
- Delete 是 `deleted: true`、无 live snippet、带 canonical tombstone hash 的普通 immutable revision。同步后删除会传播并保留 ancestry；tombstone 和其他 revision objects 当前无限期保留，没有远端 GC/compaction。
- Manifest 只指向每个 snippet 的当前 head，但 engine 会沿 parent links 验证完整 ancestry；缺失对象、cycle、snippet-ID 不一致或 hash/payload 不一致均 hard-stop。

立即 `SyncResult` 契约使用 `protocol_version` 与 `manifest_generation`，并携带 uploaded/downloaded/deleted/conflict/pending/total 计数；Settings 成功状态结构化显示 protocol/generation、所有计数和 pending。`sync_versions` history 使用 `protocol_version` 与 `generation`，保存除 pending 外的业务计数，并本地化 `publish`、兼容历史 `merge` 与未知方向。完整限制见 [已知限制](known-limitations.md#1-webdav-v2-协议与多设备并发)。

## 10. Tauri 权限与安全边界

[default capability](../src-tauri/capabilities/default.json) 只绑定 `main` 窗口，开放现有窗口的关闭、最小化、最大化/还原、显示/隐藏/查询最大化状态，以及剪贴板文本读写。WebView 不再拥有 Shell open、窗口创建/改标题或直接 autostart enable/disable/query 权限。

通用前端 Shell API 与 Rust/前端 Shell 依赖已移除。外部打开只有两个 IPC 边界：

- `open_project_repository` 使用 Rust 中固定的 `https://github.com/rainerosion/snipvault`。
- `open_trusted_directory` 只接受 `data | export` enum，并在后端通过 [paths.rs](../src-tauri/src/paths.rs) 派生真实目录；绝对路径不返回 WebView。

[tauri.conf.json](../src-tauri/tauri.conf.json) 设置 `withGlobalTauri: false`，CSP 为：本地脚本、精确 Tauri IPC scheme、现有 asset/data/blob 图片与 data font；`object-src`、`frame-src`、`base-uri` 和 `form-action` 禁用，且没有 `unsafe-eval` 或远端脚本。`style-src 'unsafe-inline'` 暂时保留，因为 CodeMirror/React 运行时会生成 inline style。

[index.html](../index.html) 不再包含 inline `<style>` 或可执行 inline `<script>`。启动样式和主题 anti-flash 分别位于 Vite 管理的 [boot.css](../src/boot.css) 与 [boot.ts](../src/boot.ts)，splash 节点和既有 frontend-ready 时序保持不变。

持久 WebDAV secret 只在 Rust 凭据边界内读取；`SettingsView`、公开错误、导出结果和受控打开命令均不返回 secret 或绝对路径。平台凭据库可用性和补偿恢复的剩余边界见 [已知限制](known-limitations.md#4-安全与隐私)。

## 11. 构建与发布架构

普通 [ci.yml](../.github/workflows/ci.yml) 在 push/pull request 上执行 Ubuntu full gate：前端 format/lint/typecheck/test/build、Markdown links、版本一致性、图标完整性，以及 Rust fmt/check/clippy/test。它不打包或发布产物。

[release.yml](../.github/workflows/release.yml) 在 `v*` tag 上发布，在手动触发时只执行 dry-run artifact build/校验并上传临时 artifact，不创建 GitHub Release。发布前的 validate job 要求 tag 与内部版本一致，并复用版本和图标门禁。

| 平台 | 当前产物 |
|---|---|
| Windows | MSI、NSIS EXE、复制的 portable EXE |
| macOS | Universal `x86_64` + `arm64` DMG；workflow 用 `lipo -info` 校验 app executable |
| Linux | DEB、AppImage |

Release job 只有在所有平台构建成功后才继续；下载完整 artifact set 后生成 `SHA256SUMS`，拒绝缺失平台产物和 `.app` 目录 asset。Tag 发布会用 GitHub artifact attestations 生成 provenance，再通过 `softprops/action-gh-release` 发布 MSI/EXE/DMG/DEB/AppImage 和 checksum。手动 workflow dispatch 不发布，只保留 dry-run artifact set。

图标链路以 [assets/app-icon.png](../assets/app-icon.png) 为唯一 canonical source；`npm run icons` 调用 Tauri icon generator 输出 [src-tauri/icons/](../src-tauri/icons/)，`npm run icons:check` 校验 PNG/ICO/ICNS magic、关键尺寸、`tauri.conf.json` 引用和旧重复生成器/输出是否已清理。前端标题栏也从同一 canonical source 通过 Vite asset import 获取图标。

仓库当前仍没有应用内 updater，也没有完整 Windows Authenticode、macOS Developer ID signing/notarization 或真实签名产物验证；这些能力需要维护者提供真实密钥/证书和外部发布环境。普通 CI full gate 仍只运行于 Linux；release workflow 的 Universal/checksum/attestation 路径需要在 GitHub Actions tag 或 dry-run 中实际验证。
