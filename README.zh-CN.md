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

## 课程 —— 给私人教师指定一个学科

给它一个空目录和一个学科,它会把**学习路线**(grounding 在公认路线上——算法用 NeetCode /
roadmap.sh / LeetCode 分类)起草成可编辑的 `roadmap.md`,再打开一个**课程看板**,由你手动推进。
每个路线 Topic 是一张卡,走 预习 → 听课 → 作业 → 复习 → 改错;`roadmap.md` 里 `- [x]` 表示已掌握,
`- [ ]` 绝不会重置你的进度。整门课自包含在目录里(`roadmap.md` + `.tutor/deck.json`),可 git、可迁移。

```bash
# 把算法路线起草进 ./algo 并建课
tutor course new ./algo --subject "algorithms"

# (改 ./algo/roadmap.md —— 重排、增删、把已会的 `- [x]` 勾掉)

# 打开交互式课程仪表盘(复习 / 课程 / 进度 三页)
tutor course ./algo

# 或一次性打印课程看板
tutor course board ./algo
```

课程仪表盘里:`←→`/`↑↓` 选卡,`.` 进阶,`,` 退阶,`空格` 标记已复习(排下一次间隔复习),
`p` 打开该 Topic 的**讲义**(题目 + 题解),`r` 重载路线改动。离线?`tutor course new … --no-llm`
会写一份 starter 路线让你自己填。路线与讲义都跟随 `--lang` 用中文起草(`tutor course new ./algo --subject "算法" --lang zh`)。

**可脚本化、可观察(不必开 TUI)。** 每个阶段动作都有一次性命令,`course state`
把整门课——进度、各阶段计数、带日期的**复习计划**——以文本或 `--json` 打印出来,
你(或脚本)无需全屏仪表盘即可推进并验证学习闭环:

```bash
tutor course state ./algo                 # 人读:状态 + 复习计划
tutor course state ./algo --json          # 机读:结构稳定
tutor course advance ./algo --topic "binary search"   # 向掌握推进一阶
tutor course demote  ./algo --topic "binary search"   # 退回一阶
tutor course review  ./algo --topic "binary search"   # 记一次间隔复习
tutor course lesson  ./algo --topic "二分查找"          # 起草/打印该 Topic 的讲义(题目+题解)
```

Topic 只有进入**复习**阶段才会进入复习计划;每次 `review` 按 1·3·7·14·30 天阶梯把
下次日期往后推,掌握(改错)后自动移出计划。

**每个 Topic 是一节「一道题一道题做」的课。** 路线只列 Topic *名字*;`course lesson`
(或 TUI 里按 `p`/`Enter`)让"大脑"把它展开成一节**预习讲义**——一段概述 + 3~5 道递进难度
的**题目**,每题配完整**题解**(思路、步骤、代码、复杂度)。在 TUI 里你像上课一样学:**一次只
显示一道题**,你**先自己做**,按 `空格` 展开**题解**,再按 `Enter` 标记**已掌握**,`←→` 切上/下
一题。进度**按做完的题目数**算(不是整话题),进度条和每张卡上的 `题 s/N` 徽章会随之走;做完一个
话题的全部题目即掌握。讲义缓存进 `<dir>/.tutor/lessons/<id>.md`(可编辑 Markdown、随目录走、
git 友好),完成记录存进卡组,二次打开秒出;`g` 重新生成。生成在**后台**跑——`omp -p` 工作时界面不卡——
讲义里按 `c` 弹出**语言选择器**切换代码语言(或 `course new`/`lesson` 时 `--code rust`,按课程持久化)。

**它用你的会话把闭环合上了。** 凡是 cwd 在课程目录内的 omp 会话都会被并进来:它暴露的"不会的"
(你的提问、你踩的报错)作为额外卡片进入卡组;你已开始接触的路线 Topic——在会话里被提到,或目录里出现
对应文件(如 `recursion_solver.py`)——会自动 预习 → 听课。于是路线说"你该会什么",会话揭示"你还不会什么";
掌握仍是一个有意的动作(`- [x]` 或 `.`)。

### 怎么选课题、看题解 —— 一步步来

两种方式:交互式仪表盘,或一条命令。

**在仪表盘里** —— `tutor course ./algo`:

1. 打开就停在**课程**看板(待办 / 进行中 / 已完成)。每张卡是一个路线 Topic —— 就是你的**课题**。
2. 用 `←→`(切换列)和 `↑↓` / `jk`(在列内上下移)把选中框移到想学的那个课题上。
3. 按 **`p`**(或 `Enter`,在**复习**页也行)打开这个课题的**讲义**——它**一次只显示一道题**,顶部有 `题目 i/N` 计数。
4. 每道题:读题、**先自己做**,按 **`空格`** 展开 `题解`,再按 **`Enter`** 标记 **已掌握**。**`←→`** 切上/下一题 · `↑↓`/`jk` 滚动 · **`g`** 重新生成 · **`esc`** 返回看板。完成记录会保存,进度按做完的题目数算。

对一个课题**第一次**按 `p`(或 `g`)会调 `omp -p`——几秒,状态栏显示 `正在生成讲义…`;之后就缓存了,再打开秒出。

**一条命令** —— 不开仪表盘,直接打印到终端:

```bash
# 用课题名字的一个(大小写不敏感)子串来选;打印概述 + 题目/题解
tutor course lesson ./algo --topic "二分"          # 短片段就够,不用写全名
tutor course lesson ./algo --topic "双指针" --lang zh
```

- `--topic` 匹配第一个包含该文字的路线 Topic,写个片段即可。
- `--lang zh|en` 强制讲义用哪种语言起草。
- 想重来:删掉缓存文件(`rm ./algo/.tutor/lessons/<id>.md`),或在仪表盘里按 `g`。

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

# 打开学科课程,然后在某个课题上按 p 学它的 题目 + 题解
tutor course ./algo
tutor course lesson ./algo --topic "二分查找"   # 或一次性打印到终端
```

TUI 按键(窗口模式):`Tab` / `1` `2` `3` 切页 · `←→` 切列、`↑↓`/`jk` 选择 ·
`m` 挖掘学习(omp -p)· `b` 生成快报(omp -p)· `r` 刷新 · `l` 中/EN · `q` 退出。

课程模式按键(`tutor course <dir>`):`←→`/`↑↓` 选课题 · `.` 进阶 · `,` 退阶 ·
`空格` 标记已复习 · **`p` 打开该课题的讲义** · `r` 重载路线 · `l` 中/EN · `q` 退出。
讲义里(一次一题):`←→` 上/下一题 · **`空格` 显示题解** · **`Enter` 标记已掌握** · `↑↓` 滚动 · **`c` 代码语言** · `g` 重新生成 · `esc` 返回。

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
