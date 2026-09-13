# 下一步去哪

够你高效干活了。想再深入：

## 内置帮助

- **`/help`** 或 **`Ctrl+P`**——所有命令、快捷键、技能，可搜索。
- **`/docs`**——完整操作指南就在 TUI 里（`/docs web` 看在线版）。会话、无头模式、子智能体、沙箱、记忆等等全在里面。
- **直接问 Grok 自己**——它读得懂自己的用户指南，也配得好自己。试试：“怎么在 CI 里跑你？”或“给 GitHub 加个 MCP 服务器”。

## 好习惯

- 会话自动保存。用 `grok -c` 回到最新会话，或 `/resume`（`Ctrl+R`）挑一个。
- 会话太长变慢？`/compact` 腾上下文；`/context` 看用量去哪了。
- 万物皆可自动化：`grok -p "summarize new TODOs" --output-format json` 无头跑——脚本和 CI 里很好用。
- `grok update` 保持最新；`/release-notes` 看改了什么。
- 觉得不对劲？`/feedback` 直达团队。

## 重开这个教程

随时输入 **`/tutorial`**。

去造点东西吧。
