# AwesomeTools

一组自己写、自己用的小工具集合。每个工具都是独立可运行的程序，独立版本号、独立发布。

## 工具

| 工具 | 说明 | 技术栈 | 版本 |
| --- | --- | --- | --- |
| [DiskSniffer](./disksniffer) | 磁盘空间扫描与树图可视化，SpaceSniffer 的替代品 | Tauri 2（Rust）+ React 19 + TypeScript | 0.1.0 |
| [DiskSniffer (egui)](./disksniffer-egui) | 同一套扫描内核的纯 Rust 版本，无 webview | Rust + egui 0.37 | 0.1.0 |

## 目录约定

```
AwesomeTools/
  README.md          # 本文件：工具索引
  .gitignore
  shared/            # 被多个工具共用的 crate
    disksniffer-core/   # 扫描器 + 节点池（两个 DiskSniffer 都用它）
  disksniffer/       # 每个工具一个顶层目录
    README.md        # 该工具自己的说明：功能、构建、校验
    package.json
    src-tauri/
  disksniffer-egui/
    README.md
  <下一个工具>/
```

新增工具时：

1. 在仓库根目录建同名目录，尽量**自包含**：单独把这个目录拷出去也能构建运行。
2. 目录内放自己的 README，写清功能、构建方式、校验命令。
3. 把新工具登记到上面的表格里。
4. 不要跨工具共享代码，除非确有需要（真需要时放 `shared/` 并在 README 说明）。目前只有 `shared/disksniffer-core` 一处共享：它是 DiskSniffer 的扫描内核，被 Tauri 版和 egui 版同时 path 依赖。因此这两个工具不能只拷自己的目录。

## 版本管理

每个工具**独立**演进版本，仓库整体不打版本号，也不使用同一个版本号串起所有工具。

- 版本号写在工具自己的清单文件里，遵循语义化版本：
  - Node 侧：`package.json` 的 `version`
  - Rust 侧：`Cargo.toml` 的 `package.version`
  - Tauri 还需同步 `tauri.conf.json` 的 `version`（三处必须一致）
- 发布用带工具名前缀的 tag，格式 `<tool>/v<major>.<minor>.<patch>`：

```bash
git tag disksniffer-egui/v0.1.0
git push origin main
git push origin disksniffer-egui/v0.1.0
```

- GitHub Release 基于该 tag 建立，标题写工具名 + 版本（例如 `DiskSniffer egui v0.1.0`），安装包等产物挂在该 Release 下。
- 只改一个工具时，不要顺手动别的工具的版本号。

### 发版检查清单

1. 三处版本号（或 Rust 项目对应清单）改成同一个新版本。
2. 跑一遍该工具 README 里列出的校验命令，全绿再发。
3. `git commit` → `git tag <tool>/vX.Y.Z` → 推送分支与 tag。
4. 构建产物并上传到对应 Release。

## 许可

[MIT](./LICENSE) © 2026 wsp1911
