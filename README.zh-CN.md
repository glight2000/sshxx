# sshxx

[English](README.md) | 简体中文

sshxx 是一个可自建、可持久化的协作终端，支持浏览器访问，并提供基于 Tauri 的跨平台客户端工程。终端进程由 daemon 持有，而不是由浏览器页面维持，因此关闭或刷新浏览端不会结束 Shell。

本项目派生自 Eric Zhang 创建的
[sshx](https://github.com/ekzhang/sshx)。感谢 Eric
Zhang 和所有上游贡献者完成了原始架构与实现。sshxx 保留了这一基础，并针对个人工作流增加功能、调整交互习惯。

![包含终端、同步便利贴和画布页面的 sshxx 工作区](docs/images/sshxx-workspace.png)

## 与上游的关系

sshxx 以独立仓库发布，而不是 GitHub Fork，因此 GitHub 页面可能不会显示“forked
from” 标识。仓库仍保留完整的上游 Git 历史和 MIT 许可证。如需检查或合并上游更新，可以添加
`upstream` remote：

```shell
git remote add upstream https://github.com/ekzhang/sshx.git
```

## 主要变化

- 浏览器和 Tauri 桌面/移动客户端共用同一套 Svelte 前端。
- daemon 将页面、终端布局、外观和便利贴保存在其当前工作目录。
- 支持多个独立画布页面、带页面定位的全局搜索、可编辑便利贴、终端复制、八方向缩放和可选的网格吸附。
- 每个终端可独立设置配色、背景覆盖、透明度和标题；便利贴使用独立的中性配色及外观设置。
- 应用界面支持白天、夜晚和跟随系统模式，不会改变终端或便利贴自身配色。
- 终端分体按钮支持保存 OpenSSH 连接，可选择默认配置、SSH
  Agent、私钥文件或交互式密码认证。
- 支持便利贴字符级同步，以及携带页面标识的协作事件。
- 浏览器本地的页面和视图恢复与共享工作区数据严格分离；工具栏连接图标显示当前会话状态。
- 升级了终端渲染组件和前后端依赖。
- 对外程序分别命名为 `sshxx-daemon`、`sshxx-server` 和
  `sshxx-client`，以便与上游 sshx 区分。

## 架构

| 组件           | 源码位置             | 职责                                  |
| -------------- | -------------------- | ------------------------------------- |
| `sshxx-daemon` | `crates/sshx-daemon` | 持有 Shell 进程并在本地持久化工作区。 |
| `sshxx-server` | `crates/sshx-server` | 协调会话并提供 Web 客户端和 API。     |
| `sshxx-client` | `src/`、`src-tauri/` | 通过浏览器或打包应用显示会话。        |

server 负责协调加密后的终端数据，但不持有 Shell 进程。即使所有浏览端都断开，daemon 仍会继续运行终端。

## 开发

项目遵循仓库锁文件，并优先使用 `mise`
管理的运行时。安装 JavaScript 依赖并启动开发用 Redis：

```shell
mise install
npm ci
docker compose up -d
```

同时启动 server、daemon 和 Web 前端：

```shell
mprocs
```

默认开发会话地址：

```text
http://localhost:5173/s/dev#localdevkey
```

daemon 会在当前目录写入
`.sshx-workspace`。为了兼容已有工作区，这个继承自 sshx 的文件名被有意保留。每次从同一目录启动 daemon，可以恢复页面、便利贴、布局和终端配置；Shell 进程本身会重新创建。

可复用的 SSH 连接配置保存在同目录的
`.sshx-connections`，文件使用带完整性校验的加密格式；本地密钥保存在仅文件所有者可读写的
`.sshx-connections.key`。密码认证始终在 OpenSSH 终端内交互输入，不会保存密码。无法读取或属于未来版本的配置文件会以
`.invalid-*` 后缀隔离保留，不会阻止 daemon 启动。

## 画布和终端操作

- 在画布空白处拖拽可以平移。鼠标中键拖拽始终移动画布，即使鼠标位于终端或便利贴上。
- `Ctrl` + 滚轮始终缩放画布并覆盖浏览器缩放。没有激活终端或便利贴时，普通滚轮也用于缩放；已聚焦终端和可滚动菜单仍保留自身滚动行为。
- 开启网格吸附后，窗口移动和八方向缩放会按照统一的小幅内缩规则对齐可见网格点；新建窗口也使用同一网格。
- 单击便利贴进入编辑，按 `Escape` 或点击外部退出编辑。
- 终端存在文本选区时，`Ctrl+C`
  会复制并清除选区；没有选区时仍发送给 Shell。`Shift+Enter`
  会向支持多行输入的程序发送 LF。
- 终端和便利贴状态属于各自页面。页面切换和视图位置仅保存在当前浏览器；页面内容及编辑操作在多端同步时始终携带页面标识。

## 构建

构建 daemon 和 server：

```shell
cargo build --release -p sshxx-daemon -p sshxx-server
```

构建静态 Web 客户端：

```shell
npm run build
```

安装对应平台的 Tauri 系统依赖后，构建打包客户端：

```shell
npm run app:build
```

Ubuntu 原生依赖：

```shell
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

打包客户端允许连接局域网 HTTP/WebSocket 服务。在不可信网络中应使用 HTTPS 和 WSS。

## 分别运行

启动本地 server：

```shell
./target/release/sshxx-server \
  --listen :: \
  --secret replace-this-secret
```

在需要保存工作区的目录中启动 daemon：

```shell
./target/release/sshxx-daemon --server http://localhost:8051
```

生产环境应在 server 前配置反向代理和 TLS，使用足够强的 secret；运行多个 server 实例时还需要配置 Redis。

## 尚未完成的工作

- Tauri 客户端外壳和平台图标已经加入工程并通过编译检查，但尚未在 Windows、macOS、Linux、Android 和 iOS 上完整验证签名安装包及发布流程。
- 缺少长期维护的生产部署参考配置，包括 TLS、反向代理、secret、Redis、升级和备份。
- 尚未实现 AI Agent 进程识别，以及 Codex、Claude
  Code 等工具的专属图标。当前提醒效果依赖终端响铃或 OSC 通知。
- 跨浏览器、跨平台 UI 自动化测试尚未覆盖拖拽/缩放吸附、页面持久化、终端键盘行为和多人同时编辑便利贴。
- TypeAhead 本地回显仍需读取 xterm 私有的当前 SGR 状态，因为公开 API 没有等价能力。访问点已经隔离，但在后续升级 xterm 前仍需增加兼容保护。

## TODO

- [ ] 在 CI 中构建并测试带签名的桌面端安装包。
- [ ] 初始化、构建并测试 Android 和 iOS 目标。
- [ ] 增加包含 TLS 和升级说明的生产自建部署示例。
- [ ] 增加带版本迁移的工作区格式、备份及恢复工具。
- [ ] 为页面、便利贴、吸附、搜索和终端快捷键增加端到端浏览器测试。
- [ ] 在增加 AI
      Agent 图标或语义化完成提醒之前，设计明确的 daemon-to-client 进程状态协议。
- [ ] 增加带能力检测的 xterm 薄兼容层；所需私有 SGR
      API 变化时，应安全关闭 TypeAhead，而不是影响终端工作。
- [ ] 完成目前暂缓的光标样式设置。

## 已知问题与限制

- `.sshx-workspace`
  只持久化元数据。daemon 重启后会恢复页面、便利贴、布局和终端配置，但 Shell 进程会重新创建，不能接续原来的进程状态。
- Windows 下，当前 ConPTY 实现无法取得子进程的工作目录。复制终端时可能回退到 daemon 的工作目录，而不是源终端所在目录。
- `Shift+Enter`
  会发送 LF 以支持多行输入，最终行为由前台应用决定；普通 Shell 或不支持多行输入的程序仍可能将其视为提交。
- 彩虹提醒依赖前台程序发出响铃或受支持的 OSC 通知，不能自行判断 AI
  Agent 是否正在运行、等待用户输入或已经结束。
- WebGL 不可用或被禁用时，终端会回退到 DOM 渲染器；大型终端的性能可能因此下降。
- TypeAhead 为了在本地回显回滚时准确恢复样式，依赖一个 xterm 私有 API。升级到不兼容的 xterm 版本前必须审计这一边界。
- 明文 HTTP/WebSocket 只适合可信局域网。面向互联网部署时，必须通过代理或网络层提供 HTTPS/WSS 和适当的访问控制。

## 验证

```shell
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run lint
npm run check
npm run test:runtime
npm run build
```

## 许可证

sshxx 继承上游的
[MIT License](LICENSE)，并原样保留原始版权和许可证声明。原项目和历史请参阅
[sshx 仓库](https://github.com/ekzhang/sshx)。
