# tutor

[![CI](https://github.com/tomtdhzz/tutor/actions/workflows/ci.yml/badge.svg)](https://github.com/tomtdhzz/tutor/actions/workflows/ci.yml)

**你的本地 omp 私人教师：把散落在会话里的"不会的"收成一门有复习节奏的课，顺带盯住每个窗口的进度，每天给你一份快报。**

[English](README.md) · [简体中文](README.zh-CN.md)

`● tutor`  ·  **预习 › 听课 › 作业 › 复习 › 改错**  ·  只读

**headroom**（额度）× **tutor**（学习）× **omp**（智能体）

你是在实战里学的：在很多终端标签里并行跑 [`omp`](https://github.com/can1357/oh-my-pi)，
提问、踩坑、然后就过去了。问题是——那些"当时不会的"就这么蒸发了，没人替你收集，也没人到点提醒你回头看。
姊妹项目 `headroom` 告诉你每个模型还剩多少**额度**；`tutor` 是管其余这些事的那位老师：

> *我还有哪些"不会的"、什么时候该复习？每个标签到底在做什么、进展到哪了？以及——我今天到底干了啥？*

`tutor` **只读**：绝不改你的 omp 配置、绝不切换任何东西。它读 `~/.omp/agent` 会话库，变成一个终端仪表盘
（可以塞进 zellij/tmux 面板，跟 `headroom` 并排），并且只在你需要时借 `omp -p` 蒸馏"不会的"、叙述这一天。

整个界面在运行时用 `l` 键中英切换，一个二进制同时服务两种语言。

## 效果

```
学习 · 今日要看 (5)
  [预习] Rust 生命周期与借用规则                       ×3
  [复习] ratatui 宽字符单元格布局                       ×2   ← 今日到期
  [听课] omp 会话 JSONL 的 title 槽结构                 ×1

  预习 找出不会的   12 ▍▍▍▍▍▍▍▍▍▍▍▍
  听课 解决不会的    5 ▍▍▍▍▍
  作业 检验不会的    2 ▍▍
  复习 死磕不会的    4 ▍▍▍▍
  改错 消灭不会的    7 ▍▍▍▍▍▍▍

看板 · 进行中 (3)
  Refine TUI activity meter and tests   [在用]
  ██████████████ ~    tutor · 2s 前
```

## 它做什么

### 1 · 学习闭环（学习）—— 核心
一句话的方法论：*预习是找出不会的，听课是解决不会的，作业是检验不会的，复习是死磕不会的，改错是消灭不会的。*
`tutor` 从最近会话里**全自动**挖出"不会的"（你的提问、你踩的报错），落成卡片，让每一张走
**预习 → 听课 → 作业 → 复习 → 改错**。`复习` 阶段按间隔重复排期（1/3/7/14/30 天），于是卡组永远告诉你
**今日要看**什么。离线也能挖（启发式）；按 `m`，`omp -p` 会蒸馏出更利落的主题并给每张卡定阶段。

### 2 · 工作看板（看板）
这位老师也替你盯着手上的活。每个终端标签/会话变成一张卡；**工作命题**自动取自会话标题，卡片按规则分入
**待办 / 进行中 / 已完成**（依据近期活跃、todo 完成度、生命周期）。绑定了活动标签的卡标记 **在用**。
进度：会话声明了 todo 就显示真实完成度百分比；没有就用消息量活跃度代理（标 `~`）——绝不谎报"完成"。

### 3 · 每日快报（快报）
把一天汇总成账：今天推进了什么、完成了什么、什么在等你、该复习什么——先是结构化事实，可选由 `omp -p`
叙述成一段话，并能导出 Markdown。

## 前置条件

平台：macOS 或 Linux。

1. **omp**（[Oh My Pi](https://github.com/can1357/oh-my-pi)）在 `PATH` 上。`tutor` 读它在
   `~/.omp/agent` 下的会话库；"大脑"通过 `omp -p` 用你已登录的账号（仅 `m`/`b` 与 `tutor briefing` 会调）。
2. **Rust 工具链 1.88+**（从源码构建，用 [rustup](https://rustup.rs)）。

依赖均为纯 Rust（serde、serde_json、anyhow、ratatui、crossterm），cargo 直接构建。

## 安装

用 cargo 直接从仓库装（无需 clone）：

```bash
cargo install --git https://github.com/tomtdhzz/tutor
```

或从本地检出安装：

```bash
git clone https://github.com/tomtdhzz/tutor
cd tutor
cargo install --path .        # 或：cargo run --release -- tui
```

打完 release tag 后，会自动往 tap 发布 Homebrew formula：

```bash
brew install tomtdhzz/tap/tutor
```

## 使用

```bash
# 一次性：打印工作看板后退出
tutor
tutor board

# 交互式仪表盘（学习 / 看板 / 快报 三页）
tutor tui

# 生成今日快报（Markdown，由 omp -p 叙述）
tutor briefing > today.md

# 只出结构化事实，不调 LLM
tutor briefing --no-llm

# 强制语言（否则按 locale 自动探测；与子命令顺序无关）
tutor --lang en
tutor board --lang zh
```

TUI 按键：`Tab` / `1` `2` `3` 切页 · `←→` 切列、`↑↓`/`jk` 选择 ·
`m` 挖掘学习（omp -p）· `b` 生成快报（omp -p）· `r` 刷新 · `l` 中/EN · `q` 退出。

选项：

| 参数 | 含义 |
|---|---|
| `--lang <zh\|en>` | 显示语言（默认按 `LANG`/`LC_*` 自动探测） |
| `--no-llm` | 不调 `omp -p`（`briefing` 只出结构化事实） |
| `-h, --help` | 帮助 |
| `-v, --version` | 版本 |

环境变量：`TUTOR_OMP_DIR` 覆盖 omp agent 目录（默认 `~/.omp/agent`）。

## 它如何读你的会话

- `~/.omp/agent/terminal-sessions/<id>`：把终端标签绑到会话文件的面包屑（第 1 行 cwd，第 2 行路径），
  这是 **在用** 的判据。
- `~/.omp/agent/sessions/<cwd>/<ts>_<id>.jsonl`：会话记录。`tutor` 读标题（→命题）、消息/工具计数与
  时间戳（→活跃度、最近活动）、`session_exit`（→生命周期）、`user_todo_edit`（→进度）、以及带线索词的
  user 行（→学习候选）。坏行跳过，绝不崩。`tutor` 自己的 `omp -p` 调用带 `--no-session`，不会把自己的提问再当会话读进来。

## 架构

一个轻量六边形；领域纯净、无 IO（与姊妹项目 `headroom` 同构）。

```
delivery/{cli,tui,i18n} ─┐
main ─────────┤→ app (Tutor 用例 + ports)
              │        │
              │        └→ domain (StudyDeck/Unknown/LoopStage, Board/WorkItem/WorkState,
              │                    DailyBriefing)
              └ adapters: omp_sessions · omp_summarize (omp -p) · cache_file · clock ─┘
```

规则派数据（复习排期、看板、进度、结构化快报）**不调** LLM、瞬时渲染；`omp -p` 只做增强
（学习蒸馏、快报叙述），且始终在明确按键之后。详见 [`docs/prd/PRD.md`](docs/prd/PRD.md) 与
[`docs/tech-design/tech-design.md`](docs/tech-design/tech-design.md)。

## 隐私

对 omp 只读：`tutor` 绝不写 `~/.omp` 下任何文件。唯一落盘的是学习库
`$XDG_CACHE_HOME/tutor/deck.json`（或 `~/.cache/tutor/deck.json`）——只存主题/细节/阶段/时间戳，
不存凭证、邮箱或原始会话内容。

## 测试

```bash
cargo test                                  # 单元 + TUI 渲染测试
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 限制

- **只读。** v0 只观察、只教学，不对 omp 采取行动。
- **阶段是推断的。** v0 由启发式或 `omp -p` 判定学习阶段；手动操作卡片（提升阶段/标记改错/改排期）留 v1。
- **启发式挖掘有噪声。** 离线候选是原始的提问/报错；`m`（omp -p）才把它们收敛成利落、去重的知识点。
- **todo 很稀疏**，看板进度通常回退到消息量活跃度代理（标 `~`），非真实完成度。**"在用"= 绑过标签，非进程存活**——
  面包屑在标签关闭后仍留存，新鲜度由"最近活动"承载。
- **大脑是 `omp -p`。** 挖掘/叙述要几秒并耗一点额度；只在 `m`/`b` 触发，绝不在渲染路径上。

## 路线图

- **v1** —— TUI 内手动操作卡片（提升阶段 / 标记已解决 / 改排期）；`watch` 常驻自动刷新。
- **v2** —— 用工具调用/diff 信号做更细的进度；看板按项目过滤；每日快报磁盘缓存。

## 许可

MIT —— 见 [LICENSE](LICENSE)。

## 免责声明

独立项目，与 `omp` / Oh My Pi 作者、Anthropic、OpenAI 均无隶属或背书关系。厂商与 omp 接口可能变化。
