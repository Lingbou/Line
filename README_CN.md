# Line

[English](README.md) | [中文说明](README_CN.md)

Line 是一个轻量化的 Linux 终端 SSH 管理工具，用 Rust 编写。在终端输入 `line` 即可唤出 TUI 启动器，上下键或鼠标选择，回车直接连接，免去每次重复输入用户名、IP 或密码。

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

## 安装与运行

### 预编译安装包（.deb 与 .rpm）

从 [Releases](https://github.com/Lingbou/Line/releases) 页面下载对应发行版的最新安装包：

```bash
# Ubuntu / Debian
sudo apt install ./line_<version>_amd64.deb

# Fedora / RHEL / CentOS
sudo dnf install ./line-<version>-1.x86_64.rpm
```

### 免安装静态二进制（全 Linux 通用）

从 [Releases](https://github.com/Lingbou/Line/releases) 下载 `line-v<version>-x86_64-unknown-linux-musl.tar.gz`，解压后将 `line` 放到 `~/.local/bin/` 或 `/usr/local/bin/` 即可直接运行。

### 源码编译安装

依赖要求：Rust (1.75+)、系统自带的 OpenSSH 与 `ssh-keygen`。保存密码连接使用 `SSH_ASKPASS`，需要 OpenSSH 8.4 或更高版本。

```bash
cargo install --path .
```

## 命令行用法

```bash
# 启动交互式 TUI 启动器（默认）
line

# 直接连接指定服务器（跳过 TUI 秒级直连）
line <服务器名称>

# 纯文本打印已保存的服务器列表
line -l
line --list

# 查看版本号
line -v
line --version

# 查看帮助说明
line -h
line --help
```

## 快捷键操作

### 主列表

| 按键 | 说明 |
| --- | --- |
| `↑` / `↓` 或 `←` / `→` | 移动选择光标 |
| `Enter` | 通过 SSH 连接选中的服务器 |
| `直接输入字符` | 按名称、用户名或主机名实时过滤 |
| `Esc` | 清空搜索关键词 |
| `Ctrl+T` | 新建连接 |
| `Ctrl+E` | 编辑当前选中连接 |
| `Ctrl+D` | 删除当前选中连接（二次确认） |
| `Ctrl+C` | 退出程序 |
| `Home` / `End` | 跳到第一项 / 最后一项 |
| `PageUp` / `PageDown` | 每次滚动 5 项 |
| `Ctrl+W` / `Ctrl+Backspace` | 搜索框向前快速删词 |
| `Ctrl+U` | 清空搜索框 |

同时也支持鼠标点击选择、直接点击按钮及滚轮滑动。

### 表单编辑

| 按键 | 说明 |
| --- | --- |
| `Tab` / `Shift+Tab` | 切换到下 / 上一个输入框 |
| `Enter` | 保存连接 |
| `Esc` | 取消并返回主列表 |
| `Ctrl+Left` / `Ctrl+Right` | 按单词向前 / 向后移动光标 |
| `Alt+Left` / `Alt+Right` | 按单词向前 / 向后移动光标 |
| `Ctrl+W` / `Ctrl+Backspace` | 向前删除整个单词 |
| `Ctrl+Delete` / `Alt+D` | 向后删除整个单词 |
| `Ctrl+A` / `Home` | 移动到行首 |
| `Ctrl+E` / `End` | 移动到行尾 |
| `Ctrl+U` | 删除到行首 |
| `Ctrl+K` | 删除到行尾 |
| `Space` | 切换认证方式或切换显示密码 |

填写提示：
- 用户名默认预填 `root`，端口默认 `22`，直接填入主机回车即可保存。
- 支持在 Host 输入框直接粘贴 `ssh -p 2222 root@192.0.2.1` 或 `user@host:port`，程序会自动拆解填充。

## 配置与存储结构

Line 将配置独立保存在 `~/.line/` 目录下（可通过环境变量 `LINE_CONFIG_DIR` 自定义路径）：

```text
~/.line/
├── profiles.json       # 连接配置（权限 0600）
├── profiles.json.bak   # 上一次保存的自动备份
├── known_hosts         # Line 专用的主机密钥库（权限 0600）
└── keys/
    ├── <连接名称>/
    │   ├── key         # 私钥（权限 0600）
    │   └── key.pub     # 公钥
    └── .shared/        # 自动去重的共享密钥存储区
```

导入私钥时如果未指定公钥，Line 会自动调用 `ssh-keygen` 推导补全对应的 `.pub` 文件。

## 代码结构

- `src/app/`：独立于 UI 的状态机、表单状态、输入事件与文本编辑辅助函数
- `src/ui/`：基于 Ratatui 的界面渲染（启动器列表、表单、弹窗与自适应布局）
- `src/config/`：配置数据模型、原子写入、备份回滚与密钥存储去重
- `src/ssh/`：OpenSSH 进程调用、PTY 终端接管、AskPass 密码注入与主机密钥变更检测
- `src/runtime/`：终端原始模式控制与主事件循环
