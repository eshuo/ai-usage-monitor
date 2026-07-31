# AI 用量监控

轻量级 Windows 桌面工具，实时监控各 AI 厂商的订阅套餐用量（5 小时窗口 / 每周限额）和账户余额。

基于 **Tauri 2**（Rust + WebView2）构建，体积仅 **5 MB**，内存占用 ~40 MB。

## ✨ 功能特性

- 🔔 **系统托盘常驻** — 鼠标悬停即可查看所有厂商用量详情（百分比 + 倒计时 + 重置时间）
- 📊 **主界面仪表盘** — 进度条可视化，支持用量、余额、重置倒计时展示
- 🔄 **自动定时刷新** — 默认 60 秒轮询，间隔可配置，可随时开关
- 🏢 **多厂商支持** — Kimi · 智谱 GLM · MiniMax · DeepSeek · 自定义
- 📋 **详细重置时间** — `2小时15分30秒后重置 (8月3日 10:00)` 格式
- 🔒 **数据本地存储** — 所有 API Key 和配置仅保存在本地 `%APPDATA%` 中

## 📦 下载使用

从 [Releases](https://github.com/eshuo/ai-usage-monitor/releases) 下载 `AI用量监控.exe`，**双击即可运行**，无需安装。

> 首次运行 Windows 可能提示"已保护你的电脑"，点击「更多信息」→「仍要运行」即可。

## 🖥️ 界面预览

```
┌─────────────────────────────────┐
│ 📊 AI 用量监控        🔄 📥     │
├─────────────────────────────────┤
│ 用量详情 | 厂商配置 | 添加 | 设置 │
├─────────────────────────────────┤
│  智谱 GLM · Level 3              │
│  5小时窗口              45.0%    │
│  ████████████░░░░░░             │
│  ⏱ 2小时15分30秒后重置 (15:00)  │
│                                  │
│  每周限额               60.0%    │
│  █████████████████░░░░          │
│  ⏱ 3天5小时12分后重置 (8月3日)  │
└─────────────────────────────────┘
```

## 支持的厂商

| 厂商 | 类型 | API 端点 | 认证方式 |
|------|------|----------|---------|
| Kimi 编程套餐 | 5h/周限额 | `api.kimi.com/coding/v1/usages` | Bearer |
| 智谱 GLM | 5h/周限额 | `open.bigmodel.cn/api/monitor/usage/quota/limit` | Raw Key |
| MiniMax | 5h/周限额 | `api.minimaxi.com/.../coding_plan/remains` | Bearer |
| Kimi 余额 | 余额查询 | `api.moonshot.cn/v1/users/me/balance` | Bearer |
| DeepSeek | 余额查询 | `api.deepseek.com/user/balance` | Bearer |
| 自定义 | 灵活配置 | 用户自定义 URL + 解析规则 | Bearer/Raw/None |

## 🛠️ 从源码构建

### 环境要求

- [Rust](https://rustup.rs/) (stable)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (MSVC)
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (Windows 10/11 通常已自带)

### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/eshuo/ai-usage-monitor.git
cd ai-usage-monitor

# 编译 Release 版本
cargo build --release

# 生成产物
# target/release/ai-usage-monitor.exe  (约 5 MB)
```

### 项目结构

```
ai-usage-monitor/
├── src/
│   ├── main.rs          # 程序入口
│   ├── lib.rs           # 应用主体 (托盘/窗口/轮询/IPC)
│   ├── config.rs        # 配置持久化
│   └── providers.rs     # 各厂商 API 查询逻辑
├── frontend/
│   ├── index.html       # 主界面
│   ├── app.js           # 前端交互逻辑
│   └── styles.css       # 样式
├── icons/               # 应用图标
├── Cargo.toml           # Rust 依赖配置
└── tauri.conf.json      # Tauri 配置
```

## ⚙️ 配置说明

配置文件位置：`%APPDATA%\ai-usage-monitor\config.json`

```json
{
  "providers": [
    {
      "id": "prov_xxx",
      "name": "我的智谱",
      "providerId": "zhipu",
      "creds": { "apiKey": "xxx.xxx", "baseUrl": "https://open.bigmodel.cn" },
      "enabled": true
    }
  ],
  "autoRefresh": true,
  "refreshInterval": 60
}
```

## 🔧 技术栈

- **Rust** — 后端逻辑、HTTP 请求、托盘管理
- **Tauri 2** — 桌面应用框架，使用系统 WebView2
- **原生 HTML/CSS/JS** — 前端界面，无打包器依赖
- **reqwest + rustls** — HTTP 客户端（纯 Rust TLS）

## 📄 License

MIT
