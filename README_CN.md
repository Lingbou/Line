# Line

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[English](README.md) | [中文文档](README_CN.md)

Line 是一个用 Rust 编写的高性能、轻量化 Linux 终端 TUI SSH 连接管理工具。它专为解决频繁手动输入用户名、IP、端口与密码的痛点而设计，无需臃肿繁琐的 GUI 工具。

设计灵感源于 `htop` 的极速启动与纯粹交互：毫秒级加载、内存占用极低。启动即展现高密度单屏启动器，回车直接接管终端连接服务器；远程会话结束后自动恢复 TUI 界面并锁定当前连接。

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ line  /  connections                                                          Ctrl+T  New connection │
│                                                                                                      │
│  / Find by name, user or host…                                                             13 saved  │
│                                                                                                      │
│   CONNECTION                             AUTH  DESTINATION                                           │
│ › prod-api-cluster                       KEY   deploy@198.51.100.24:22                               │
│   staging-k8s-node-01                    KEY   root@203.0.113.88:22                                  │
│   bastion-gateway-singapore              KEY   admin@192.0.2.15:2222                                 │
│   backup-database-us                     PWD   root@198.51.100.200:22                                │
│ ──────────────────────────────────────────────────────────────────────────────────────────────────── │
│ SSH key  keys/prod-api-cluster/key                                                                   │
│                                                                                                      │
│  Enter  Connect   Ctrl+E  Edit   Ctrl+D  Delete                                                      │
│                                                                                                      │
│ ↑↓ select   ·   type to filter                                                           Ctrl+C Quit │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

## 核心特性

- **极致轻量与速度**：基于 Rust 编写，无运行时依赖，冷启动几乎无感，内存占用极小。
- **高颜值紧凑布局**：采用 `CONNECTION` → `AUTH` → `DESTINATION` 三列科学布局，长服务器名称与中文字符均能完整显示，杜绝界面中间大面积空白。
- **自适应响应式界面**：自适应 40 列窄屏终端（紧凑双行显示）至大屏终端（居中 104 列卡片），各种屏幕比例下皆自然舒适。
- **零摩擦极速录入**：
  - 登录用户名默认 `root`，端口默认 `22`。添加服务器时填入主机地址后直接回车即可保存。
  - **智能命令识别**：在主机框中可直接粘贴云厂商控制台复制的完整命令（如 `ssh -p 2222 root@1.2.3.4` 或 `user@host:port`），Line 会自动拆解提取主机、用户与端口。
- **完备的密钥与凭据管理**：
  - 密钥存放在以连接名命名的专属目录中：`~/.line/keys/<连接名称>/key`。
  - **自动去重**：共用密钥自动在 `~/.line/keys/.shared/` 中内容去重并安全映射，无需为重复密钥重复创建文件。
  - **公钥自动推导**：仅导入私钥时，Line 会自动调用 `ssh-keygen` 推导补全对应 `.pub` 公钥文件。
  - 本地独立凭据模型：所有配置统一存放于 `~/.line/profiles.json`（权限 `0600`，目录权限 `0700`），不污染个人 `~/.ssh/config`。
- **纯键盘流与鼠标兼顾**：
  - 完整支持终端标准跳词、删词、行首行尾等快捷键，输入如飞。
  - 支持鼠标点击选择、双击/回车直连、滚轮翻页。

## 编译与安装

Line 需要 Rust 环境（1.75+）、系统自带的 OpenSSH 以及 `ssh-keygen`。保存密码登录功能需要 OpenSSH 8.4+（支持 SSH_ASKPASS force）；私钥连接兼容所有标准 OpenSSH 版本。

```bash
# 编译 Release 版本
cargo build --release

# 运行
./target/release/line
```

全局安装到系统路径：

```bash
cargo install --path .
```

## 配置目录结构

Line 将所有数据严格隔离在 `~/.line/` 目录下：

```text
~/.line/
├── profiles.json       # 连接配置文件（权限 0600）
├── profiles.json.bak   # 上一次保存的自动备份
├── known_hosts         # Line 专用的主机密钥库（权限 0600）
└── keys/
    ├── <连接名称>/
    │   ├── key         # 私钥文件（权限 0600）
    │   └── key.pub     # 公钥文件
    └── .shared/        # 内容哈希去重的共享私钥存储区
```

可以通过设置环境变量 `LINE_CONFIG_DIR=/path/to/dir` 自定义配置目录。

## 快捷键一览

### 主界面（浏览列表）

| 快捷键 | 动作 |
| --- | --- |
| `↑` / `↓` 或 `←` / `→` | 切换选中的连接 |
| `Enter` | 直接建立 SSH 连接 |
| `直接输入字符` | 实时拼音/英文模糊过滤搜索（匹配名称、用户名、主机） |
| `Esc` | 清空搜索过滤条件 |
| `Ctrl+T` | 新建连接 |
| `Ctrl+E` | 编辑当前连接 |
| `Ctrl+D` | 删除当前连接（含二次确认） |
| `Ctrl+C` | 退出程序 |
| `Home` / `End` | 直达第一项 / 最后一项 |
| `PageUp` / `PageDown` | 按 5 项跨度上下翻页 |
| `Ctrl+W` / `Ctrl+Backspace` | 搜索框向前快速删词 |
| `Ctrl+U` | 快速清空搜索关键词 |

### 表单编辑（新建 / 修改）

| 快捷键 | 动作 |
| --- | --- |
| `Tab` / `Shift+Tab` | 跳到下一个 / 上一个输入框 |
| `Enter` | 快速保存连接 |
| `Esc` | 取消编辑并返回主列表 |
| `Ctrl+Left` / `Ctrl+Right` | 按单词向左 / 向右光标跳跃 |
| `Alt+Left` / `Alt+Right` | 按单词向左 / 向右光标跳跃 |
| `Ctrl+W` / `Ctrl+Backspace` | 向前删除整个单词 |
| `Ctrl+Delete` / `Alt+D` | 向后删除整个单词 |
| `Ctrl+A` / `Home` | 光标移至行首 |
| `Ctrl+E` / `End` | 光标移至行尾 |
| `Ctrl+U` | 删除光标至行首内容 |
| `Ctrl+K` | 删除光标至行尾内容 |
| `Space` | 切换密码/密钥认证方式，或切换显示/隐藏密码 |

## 源码模块分层

- `src/app/`：无依赖纯状态逻辑层、表单状态机、按键事件处理与编辑分词辅助函数
- `src/ui/`：基于 Ratatui 的终端界面渲染、启动器布局、表单卡片与模态弹窗
- `src/config/`：配置数据结构、JSON 序列化、备份回滚与密钥存储管理
- `src/ssh/`：OpenSSH 调用封装、PTY 终端接管、AskPass 凭据注入与主机密钥告警识别
- `src/runtime/`：终端原始模式（Raw Mode）管理、持久化事件编排与主循环入口
