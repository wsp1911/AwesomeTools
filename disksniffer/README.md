# DiskSniffer

一个 SpaceSniffer 风格的磁盘空间分析与可视化工具，基于 **Tauri 2（Rust）+ React 19 + TypeScript**。

## 功能

- **并行扫描**：Rust 端用 rayon 递归并发遍历目录，Windows 走 `GetFileInformationByHandleEx` 批量枚举（一次拿到占用空间 + 文件 ID），自动跳过符号链接/联接点以避免重复计数与死循环。
- **树图可视化**：自研 squarified treemap 布局，支持多层嵌套（聚焦目录 → 一级子项 → 二级子项），按文件类型着色。
- **下钻与面包屑**：单击文件夹进入，返回上级按钮 / Backspace，路径任意层级跳转。
- **列表视图**：可排序的表格视图，含占比条、类型与子项数。
- **搜索**：全树按名称检索，按体积排序，点击直接定位到所在目录并选中。
- **统计面板**：总大小、文件数与目录数、扫描耗时、无法访问项、按类型分布、最大文件排行。
- **文件操作**：在文件管理器中定位/打开、复制完整路径、移到系统回收站（删除后父级大小实时回收）。Windows 上走 shell API `SHOpenFolderAndSelectItems` / `ShellExecuteW`，注册为默认的第三方文件管理器（Directory Opus、OneCommander、Files 等）可以直接接管，不再固定打开 Explorer。
- **实时进度**：扫描过程中持续推送文件数、目录数、累计字节与当前路径，可随时停止。
- **拖拽扫描**：把文件夹拖进窗口即可开始扫描。

## 架构

后端不把整棵树序列化给前端。扫描结果被打包成定长记录（每个节点 40 字节：名字偏移、父子/兄弟指针、大小、类型标记），名字全部塞进一条共享的字符串区，然后写入临时文件并用 mmap 映射回来，进程堆里不再保留整棵树 —— 只有查询到某个节点时才把对应页读进内存：

    src-tauri/src/
      scan.rs   # 并行目录遍历 + 文件类型分类 + 路径规范化
      tree.rs   # 节点池（落盘 + mmap）、分页/搜索/统计/回收站删除
      fsops.rs  # 在资源管理器中定位与打开
      lib.rs    # Tauri 命令与 scan:progress / scan:done 事件

前端按需分页拉取数据（当前目录的子项 + 若干层嵌套），因此百万级文件的目录也能流畅渲染：

    src/
      lib/treemap.ts    # squarified treemap + 多层布局
      lib/api.ts        # invoke 封装
      components/       # Treemap / ListView / Sidebar / Toolbar ...
      App.tsx           # 状态机与事件编排

## 开发

    npm install
    npm run tauri dev      # 开发模式（会启动 vite dev server）
    npm run tauri build    # 打包安装程序

## 校验

    npm run check:rust      # Rust 单元测试（扫描器 + 节点池）
    npm run check:treemap   # 树图布局算法自检（面积/边界/嵌套/重叠）
    npm run build           # tsc 类型检查 + 生产构建

## 内存

整盘扫描的结果量级很大（百万级节点），所以索引不常驻堆内存：

- 每个节点 40 字节定长记录 + 名字字节流，二者写入临时文件后 mmap 按需读页，索引本身只读。
- 删除不改索引，而是记在内存 overlay 里（被删子树的 id 区间 + 祖先需扣减的字节数），大小与节点数无关。
- 索引文件随 `ScanTree` 一起 drop，退出或重新扫描时自动清理；临时目录不可写时自动回退到内存模式。
- 扫描结束调用 `SetProcessWorkingSetSize`，让 Windows 回收扫描期用过的页。

1,001,001 个节点的实测（debug 构建）：索引落盘 + 映射 171 ms，落盘后进程私有内存约 1 MB、工作集约 6 MB（索引页按需从文件读回，可被系统随时回收）。扫描期间的峰值来自临时的 `RawNode` 树，会边扁平化边释放。

## 说明

- 扫描统计的是**磁盘实际占用**（Windows 取 `AllocationSize`，Unix 取 `st_blocks * 512`），与资源管理器的“占用空间”口径一致：
  - 硬链接只计一次（按文件 ID 去重），避免同一份数据被重复累加。
  - OneDrive 等云端占位文件按真实占用计（未下载的为 0）。
  - 不包含 $MFT、$LogFile 等 NTFS 元数据，因此结果可能略小于资源管理器显示的“已用空间”。
- 没有权限的目录会被跳过并计入“无法访问”。
- “移到回收站”调用系统回收站，删除前会二次确认，可从回收站还原。
