# DiskSniffer (egui)

DiskSniffer 的纯 Rust 版本：同样的磁盘扫描与树图可视化，界面用 [egui](https://github.com/emilk/egui)/eframe 直接画在 GPU 上，没有 webview、没有 JS 运行时。

## 和 Tauri 版的区别

| | Tauri 版（[`disksniffer`](../disksniffer)） | egui 版（本目录） |
| --- | --- | --- |
| 界面 | React 19 + TypeScript | egui 0.37（立即模式，Painter 直绘） |
| 运行时依赖 | 系统 WebView2 | 无（单个 exe） |
| 常驻内存 | 进程 + WebView2 | 只有进程本身 |
| 扫描内核 | `shared/disksniffer-core` | 同一份 crate |

本目录目前实现的是**扫描 + 树图主链路**。Tauri 版里的列表视图、搜索、右键菜单（在文件管理器中打开 / 移到回收站）还没有搬过来。

## 功能

- **并行扫描**：共享内核用 rayon 递归并发遍历，Windows 走 `GetFileInformationByHandleEx` 批量枚举（一次拿到占用空间 + 文件 ID），自动跳过符号链接/联接点以避免重复计数与死循环。
- **树图可视化**：自研 squarified treemap，支持多层嵌套（聚焦目录 → 一级子项 → 二级子项），按文件类型着色。父块的标题条独立占一条，子块永远画在标题条下方，标签不会互相遮挡。
- **悬停命中**：命中测试取指针下**最深**的块，与绘制顺序无关；两个相邻块之间的缝隙吸附到距离更近的那个子块，不会闪回父块。
- **下钻与面包屑**：单击文件夹进入，面包屑任意层级跳转，`Backspace` 返回上级。
- **统计面板**：总大小、文件数与目录数、扫描耗时、无法访问项、按类型分布、最大文件排行。
- **实时进度**：扫描过程中显示文件数、目录数、累计字节与当前路径，可随时停止（`Esc`）。
- **拖拽扫描**：把文件夹拖进窗口即可开始扫描。

## 架构

    src/
      layout.rs   # squarified treemap + 多层布局 + 命中测试（含单元测试）
      app.rs      # eframe App：扫描线程、进度、面包屑、绘制、交互
      theme.rs    # 按文件类型配色
      format.rs   # 字节/计数/百分比/耗时格式化
      main.rs     # run_native 入口

因为 egui 是立即模式，每帧都重画。布局结果按「聚焦节点 + 像素尺寸」缓存，只有下钻或窗口尺寸变化时才重新计算；一帧内最多画 220 个块（一级层面超出部分聚合成「其他 N 项」）。

## 构建

    cargo build --release

产物：`target/release/disksniffer-egui.exe`（release profile 已开 LTO、`opt-level = "s"`、strip）。

## 校验

    cargo test

覆盖布局与格式化：面积守恒、边界、嵌套不越界、标题条不被覆盖、缝隙吸附、绘制顺序无关性、退化输入等。
