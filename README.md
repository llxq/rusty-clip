# Rusty Clip

一个基于 Tauri 2、React 19 和 TypeScript 的桌面剪贴板历史工具。

Rusty Clip 通过全局快捷键呼出历史面板，支持检索文本、图片、文件路径和链接，并提供置顶、收藏、删除、排序，以及将历史内容重新粘贴回之前输入位置的能力。

## 当前能力

- 监听系统剪贴板并持久化历史记录
- 支持文本、图片、文件路径列表三类内容
- 自动识别链接内容并单独筛选
- 支持搜索、分类筛选、时间排序
- 支持置顶、收藏、删除、清空非收藏记录
- 支持键盘导航
- 支持通过快捷键重新呼出 launcher
- 支持将选中的历史项重新写回并自动粘贴到原应用

## 技术栈

- Tauri 2
- React 19
- TypeScript
- Vite
- Rust
- SQLite（通过 `tauri-plugin-sql`）

## 快速开始

### 1. 安装依赖

```bash
npm install
```

### 2. 启动桌面开发环境

```bash
npm run tauri
```

这会同时启动前端开发服务和 Tauri 桌面应用。

## 常用命令

```bash
# 前端开发服务器
npm run dev

# 前端生产构建
npm run build

# Tauri 桌面开发
npm run tauri

# 打包桌面应用
npm run release
```

Rust 侧静态检查：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

## 使用方式

默认全局快捷键：

- macOS：`Command + Shift + V`
- Windows：`Ctrl + Shift + V`

基础交互：

- 输入关键字搜索文本、文件路径或图片路径
- `↑ / ↓` 切换当前选中项
- `Enter` 将当前选中项粘贴回之前的应用
- `Esc` 关闭面板
- 双击列表项可将内容写回剪贴板

## 权限要求

### macOS

为了支持“恢复之前应用焦点并自动粘贴”，通常需要以下权限：

- 辅助功能（Accessibility）
- 自动化（Automation，允许应用控制 `System Events`）

如果已经授权但仍然无法自动回填，请优先检查：

- `Terminal`、开发中的 app、或最终打包 app 是否都拿到了辅助功能权限
- 系统是否允许 `osascript` 发送按键事件
- 目标应用是否允许被重新激活并接收粘贴快捷键

## 项目结构

```text
.
├── src/
│   ├── App.tsx                    # Launcher 前端界面与交互逻辑
│   ├── App.css                    # Launcher 样式
│   ├── constants/history.ts       # 前后端共享的事件名与快捷键文案
│   └── types/clipboard.ts         # 剪贴板历史项类型定义
├── src-tauri/
│   ├── src/lib.rs                 # Tauri 应用入口、窗口/托盘/快捷键逻辑
│   └── src/clipboard_history.rs   # 剪贴板监听、SQLite 持久化、粘贴能力
└── README.md
```

## 核心实现说明

### 前端

`src/App.tsx` 负责：

- 读取历史列表
- 搜索、筛选、排序
- 键盘导航与选中状态维护
- 调用 Tauri 命令执行复制、置顶、收藏、删除、清空、自动粘贴

### 后端

`src-tauri/src/clipboard_history.rs` 负责：

- 轮询系统剪贴板
- 标准化并写入 SQLite
- 图片落盘
- 历史项读取与更新
- 将历史内容重新写入系统剪贴板

`src-tauri/src/lib.rs` 负责：

- 创建和显示 launcher 窗口
- 管理托盘与全局快捷键
- 记录 launcher 打开前的前台应用
- 关闭 launcher 后恢复原应用并触发自动粘贴

## 已知限制

- 自动粘贴依赖平台能力与系统权限，不同应用对焦点恢复和模拟按键的兼容性不同
- Linux 平台当前没有完善的“恢复原应用并自动粘贴”实现
- 当前没有自动化测试，变更后建议至少执行：

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

## 后续可补充项

- 更完善的图像预览与文件图标
- 更稳定的跨应用焦点恢复策略
- 自动化测试
- 历史数据导出/导入
- 更细粒度的排序与筛选规则
