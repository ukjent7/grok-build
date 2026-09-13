# 从 Claude、Cursor 或 Codex 迁移而来？

别担心——你的设置、规则和技能都能带过来。Grok Build 会读取其他智能体使用的同一套项目约定，其余部分也能一键导入。

## 自动沿用的内容

- **规则与指令**——`AGENTS.md`（Codex/OpenCode 约定）、`CLAUDE.md`（含嵌套的），以及 `.claude/rules/` 和 `.cursor/rules/` 下的 `*.md` 规则。
- **技能与自定义命令**——`~/.claude/skills/`、`~/.claude/commands/`、`~/.cursor/skills/` 及其项目级对应目录。扁平的命令 `.md` 文件在这里也会变成斜杠命令。
- **MCP 服务器**——来自 `~/.claude.json`、`.cursor/mcp.json` 以及项目级 `.mcp.json`。
- **Hooks**——来自 `.claude/settings.json`，含 `Bash` 这类匹配器别名，多数 hooks 无需改动即可运行。

## 一键导入

**`/import-claude`** 会扫描你的 `~/.claude` 设置——权限、环境变量、MCP 服务器、hooks——并显示勾选预览；确认后把选中的项写入 `.grok` 配置。随时可以重跑。

## 从上次的位置继续

**`/resume-claude`**、**`/resume-codex`** 和 **`/resume-cursor`** 这几个技能可以直接在这里继续那些工具里的最近会话。

## 查看发现了什么

在仓库里运行 **`grok inspect`**，可以看到 Grok 发现的每份规则文件、技能和 MCP 服务器，并标出它们来自哪里。每个兼容来源都可以在 `[compat.claude]` / `[compat.cursor]` 配置节中开关。

还有几个在别处容易错过的功能：`/btw` 可以在不打断当前任务的情况下问个旁支问题，`/rewind` 可以回退到较早的轮次（文件改动保持原样）。

*深入了解：`/docs Project Rules (AGENTS.md)`、`/docs Skills` 或 `/docs MCP Servers`*
