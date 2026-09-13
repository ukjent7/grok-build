# 个性化定制

## 最省事：直接开口

Grok 懂自己的能力，可以自己配自己。试试：

- *“给我们的 staging 数据库加上 Postgres MCP 服务器”*
- *“换个浅色主题”*
- *“给这个仓库写个 AGENTS.md”*

想自己动手，下面每项也都有对应命令。

## 教 Grok 认识你的项目：AGENTS.md

在仓库根目录放一个 `AGENTS.md`，写清构建命令、约定和坑。Grok 每个会话都会自动读——这是杠杆率最高的定制：

```markdown
# My Project
- Run tests with `pnpm test`
- Never edit files under generated/
```

## 教 Grok 记住事实：记忆

提示以 `#` 开头（或用 `/remember`）可以给以后的会话存条备注：`# the staging deploy uses eu-west`。

## 外观、按键和扩展

- **`/theme`**——配色主题（`auto` 跟随系统）；**`/settings`**（或 `F2`）管其余所有设置；喜欢 vim 就看 **`/vim-mode`**。
- **技能**（`/skills`）——可复用的提示包；用户可调用的技能会自动变成斜杠命令。
- **MCP 服务器**（`/mcps`）和**插件与 hooks**（`/plugins`、`/hooks`）。

先搞定 `AGENTS.md` 和主题，其余用到再加。

*深入了解：`/docs Project Rules (AGENTS.md)`、`/docs Skills` 或 `/docs MCP Servers`*
