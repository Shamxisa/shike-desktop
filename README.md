# 时刻 · Windows 桌面闹钟

闹钟、倒计时、便签与可调毛玻璃界面。使用 Tauri 2、React、TypeScript、Rust、SQLite、reqwest 和 keyring。

## 启动

在项目目录运行以下命令，首次使用时安装依赖。

```powershell
cd C:\application\study\git\shike-desktop
npm install
npm run desktop
```

需要 Node.js 20.11+、Rust stable MSVC 工具链、Microsoft C++ Build Tools、Windows SDK 和 WebView2。启动脚本自动查找本项目和上一级 `.tools` 中的编译工具，也支持系统安装的工具。

`npm run desktop` 首次启动或源代码变化后会构建前端与桌面程序，将可直接运行的程序缓存在项目的 `.tools/runtime` 中。后续启动直接打开缓存程序，界面由本地文件加载。此过程生成开发用程序，安装包按后续要求生成。

修改界面时使用以下命令，可以实时查看修改。

```powershell
npm run desktop:dev
```

迁移到本目录后，当前机器已安装的编译工具保留在上一级 `.tools`。项目的 `.gitignore` 已排除 `.tools`、依赖、构建产物和本地环境配置，正常 Git 提交会遵循这些规则。通过网页手动上传文件时需自行排除这些目录。

## 检查

```powershell
npm run build
npm test
npm run backend:check
npm run backend:test
```

`npm run dev` 仅启动前端开发服务。桌面功能通过 `npm run desktop` 或 `npm run desktop:dev` 连接 Rust 后台。

## 使用

- 关闭主窗口后进入系统托盘。托盘菜单提供显示、置顶和退出。
- 闹钟可设置日期、自选星期、铃声、音量和稍后提醒间隔。
- 倒计时支持后台运行及暂停、继续、重置。
- 开始与暂停共用一个按钮，运行后显示暂停，暂停后显示继续。
- 小窗模式在倒计时或闹钟提醒时显示关闭响铃按钮，可同时关闭当前提醒。
- 便签自动保存，可分别打开独立窗口和置顶。
- 外观支持背景图片、透明度、模糊、时间格式和开机启动。
- AI 服务页可配置 OpenAI 兼容 Chat Completions 服务。基础地址通常以 `/v1` 结尾。API Key 保存至 Windows 凭据管理器。

## 项目结构

- `src/components` 保存业务界面。
- `src-tauri/src/scheduler.rs` 处理后台计时与提醒。
- `src-tauri/src/storage.rs` 处理 SQLite 数据。
- `src-tauri/src/ai.rs` 处理 reqwest、SSE 与 keyring。
- `src-tauri/src/windows.rs` 处理窗口恢复和便签小窗。

数据库与导入媒体保存在 Windows 当前用户的应用数据目录 `com.shike.desktop` 中，通常位于 `%APPDATA%` 下。便签与闹钟功能可离线使用。

完整设计范围见 [DESIGN.md](DESIGN.md)。

