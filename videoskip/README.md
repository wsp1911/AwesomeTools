# VideoSeek

看番时左右方向键的步长一般只有 5 s / 10 s，跳过一分半的 OP 得按十几下。这个 Chrome 扩展把它变成一次按键：按步长跳，或者直接输入时间点。

## 功能

- **Shift + → / ←**：按设定的步长快进 / 快退，默认 30 s，可调 1–600 s。
- **Shift + G**：弹出时间输入框，输入绝对时间（`1:30`、`90`、`0:01:30`）或相对跳转（`+30`、`-15`），回车即跳。
- 每次跳转后在画面下方闪过一行提示（`+30s → 1:53`），不挡内容、不打断播放。
- 扩展弹窗里改步长、开关功能，也可以直接为当前标签页打开时间框。

设置存在 `chrome.storage.sync`，跟着浏览器账号走；改动立刻生效，不需要刷新页面。

## 安装

1. 打开 `chrome://extensions`，打开右上角「开发者模式」。
2. 点「加载已解压的扩展程序」，选择这个目录（`videoskip/`）。
3. 在任何带视频的网页里试 `Shift + →` 与 `Shift + G`。

改完代码后，在扩展页点一下刷新按钮，再刷新目标页面。

## 键位

| 键 | 作用 |
| --- | --- |
| `Shift` + `→` / `←` | 前 / 后跳一个步长 |
| `Shift` + `G` | 打开时间输入框（`Enter` 跳转，`Esc` 取消） |

在输入框、文本域、`contenteditable` 里打字时不会触发，播放器自身的快捷键也会被拦下，不会叠加一次站内跳转。

## 时间格式

| 输入 | 含义 |
| --- | --- |
| `83` | 第 83 秒 |
| `1:23` | 1 分 23 秒 |
| `1:02:03` | 1 小时 2 分 3 秒 |
| `1:23.5` | 支持小数秒 |
| `+30` / `-15` | 相对当前位置前进 / 后退 |
| `1：23` | 全角冒号也认（中文输入法常打出来） |

## 架构

    manifest.json        # MV3：content script 注入所有页面（含 iframe，all_frames）
    content.js           # 键位、播放器选择、Shadow DOM 的提示条与输入框
    time.js              # 时间解析与格式化（不依赖 chrome/DOM，可直接跑单测）
    popup.html / popup.js # 步长与开关、为当前标签页打开时间框
    icons/make-icons.ps1 # 用 System.Drawing 画 16/32/48/128 图标，图标由脚本产出
    tests/time-check.mjs # time.js 的断言

几个取舍：

- **没有 background service worker**：逻辑全在 content script 里，设置通过 `chrome.storage.sync` 的 `onChanged` 实时同步，省掉一层消息转发。
- **键位监听挂 `window` 的捕获阶段**：比播放器自己的 `keydown` 更早拿到事件，然后 `preventDefault` + `stopPropagation`，避免站内再跳一次。
- **播放器选择**：优先用户最近碰过的 `<video>`（播放 / 点击 / 聚焦），否则取可视面积最大的一个，兼容一个页面上挂多个播放器。
- **UI 全在 Shadow DOM 里**（`position: fixed` + 最高 z-index）：站点样式和事件处理器影响不到它，它也不污染站点。

## 校验

    node --check time.js content.js popup.js
    node tests/time-check.mjs
    node -e "JSON.parse(require('fs').readFileSync('manifest.json', 'utf8'))"

时间解析、格式化和脚本语法都有自动检查；键位、跳转、弹窗这些真实交互只能手动验证 —— 加载解压后的扩展，在任意带 `<video>` 的页面里试一遍。

## 已知限制

- 只对页面里的 HTML5 `<video>` 生效；纯 canvas 输出的播放器拿不到 `currentTime`。
- 元数据还没加载时（`duration` 为 `NaN`）仍会设置 `currentTime`，但要等元数据到位才真正跳过去。
- 全站注入（`<all_urls>` + `all_frames`），安装时 Chrome 会提示「读取和更改你在所有网站上的数据」。只想在少数站点用时，把 `manifest.json` 里的 `matches` 收窄即可。
