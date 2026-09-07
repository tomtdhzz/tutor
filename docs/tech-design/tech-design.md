# 技术设计 — tutor

关联：`docs/prd/PRD.md`（需求 R*/验收 AC*）、`.ai-work/ledger.md`（grounded facts）。

## 1. 架构总览

一个轻量六边形（hexagon），与姊妹项目 `headroom` 同构：**领域纯净、无 IO**，基础设施在边缘实现端口，交付层可替换而不动核心。

```
delivery/{cli,tui,i18n} ─┐
main ─────────┤→ app (Tutor 用例 + ports)
              │        │
              │        └→ domain (Board/WorkItem/WorkState, StudyDeck/Unknown/LoopStage,
              │                    DailyBriefing) —— 纯规则、可脱网单测
              └ adapters: omp_sessions · omp_summarize · cache_file · clock ─┘ (实现 ports)
```

- `domain/`：泛在语言与规则；零 IO；分类、间隔重复、快报组装都在此，单测覆盖。
- `app/`：`Tutor` 用例（scan→board / seed_heuristic / mine / briefing / narrate）与
  `SessionSource` / `Summarizer` / `Clock` / `DeckStore` 四个端口。
- `adapters/`：omp 磁盘会话的防腐层（ACL）、`omp -p` 大脑、XDG 学习库缓存、系统时钟。
- `delivery/`：CLI 一次性渲染 + 交互式 TUI（ratatui），经 `i18n` 本地化（zh 默认）。

**关键分层原则**：规则派数据（看板/进度/排期/结构化快报）**不依赖 LLM**，渲染路径零网络、瞬时。`omp -p` 只做"增强"（蒸馏更好的学习卡、叙述快报），且仅由 `m`/`b` 手动触发（详见 §5）。

## 2. 数据来源与防腐层（adapters/omp_sessions）

只读 `~/.omp/agent`（可 `TUTOR_OMP_DIR` 覆盖）：

- `terminal-sessions/<terminal-id>`：面包屑，line1=cwd，line2=会话文件绝对路径。构成
  `路径→terminal_id` 映射 = "在用/绑定标签"。面包屑在终端关闭后仍留存 → 语义是"这个
  会话绑过某个标签"（对应用户的"每个切出来的标签"），新鲜度由 recency 承载（决策 D5）。
- `sessions/<encoded-cwd>/<ts>_<id>.jsonl`：逐行解析为 `serde_json::Value`，坏行跳过：
  - `title` 槽 / `session` 头 → 命题（标题）、cwd、创建时刻。
  - `message` → 计数、抓首条 user 文本作首 prompt、抓命中线索词的 user 文本作学习候选、
    以 `message.timestamp`(epoch-ms) 更新最近活动。
  - `custom` → `tool_execution_start` 计数；`session_exit.kind` 判 lifecycle；
    `user_todo_edit.phases[].items[].status` 汇总为 `Progress{done,in_progress,total}`。

历史数据中 todo 几乎不存在（grounded fact）→ 进度必须优雅降级到活跃度/recency。

**ISO-8601 解析**：`YYYY-MM-DDTHH:MM:SS(.fff)?Z` 手写解析 + Howard Hinnant
`days_from_civil` → `SystemTime`，零依赖（避免引入 chrono）。单测锚定真实时刻。

## 3. 领域模型（domain）

### 3.1 看板（pane.rs）
- `Progress{done,in_progress,total}`：`total==0` 表示"未知"，非"零"。
- `WorkItem`：命题、进度、活跃度、lifecycle、创建/最近活动、terminal_id。
  - `state(now, active_window=6h, stale=3d) → WorkState`（纯函数，7 条单测）：
    - todo 全完成 → Done；
    - 有声明计划：done>0 或近期活跃 → Doing，否则 Todo；
    - 无计划：近期活跃/在用 → Doing；正常退出且久置 → Done；≤2 消息 → Todo；久置 → Done；余 Doing。
  - `activity_level()`：消息量→0..100 代理（~40 条视为满），`gauge_pct()` = 真实完成度 or 活跃度代理（决策 D6）。
- `Board::build`：按最近活动降序，分三列。

### 3.2 学习闭环（study.rs）
- `LoopStage` 预习/听课/作业/复习/改错，对应五句"…不会的"。
- `Unknown`：id（topic 归一）、topic/detail/project、stage、times_seen、reviews、
  first/last_seen、next_review。
  - `is_due(now)`：Correct 永不到期；Preview 恒到期；余 `next_review<=now`。
  - `reschedule(now)`：reviews+1，间隔取 `[1,3,7,14,30]` 天（间隔重复）。
- `StudyDeck`：`upsert` 按归一 id 合并（累加 times_seen、取新 stage/detail）；
  `due` / `by_stage` / `counts`。5 条单测。

### 3.3 快报（briefing.rs）
`DailyBriefing::compose(board, deck, now, window=24h)`：24h 内触达的 Doing→今日进行、
Done→今日完成、全部 Todo→等待、deck 到期→待复习。`to_prompt` 生成给 `omp -p` 的叙述提示。

## 4. 用例（app/Tutor）
- `scan()`：一次扫盘，board/study/briefing 复用，避免三读磁盘。
- `board(scans)`：映射 `ScannedSession→WorkItem`（命题回退、字段搬运）。
- `seed_heuristic(scans, base)`：候选按时间倒序取前 40，`topic_of` 取首句裁剪，作
  Preview 卡 upsert（离线恒可用，AC4）。
- `mine(summarizer, scans, base)`：取近 60 候选 → 提示 `omp -p` → `parse_mined`
  抽取 `[ … ]`（容错围栏/散文）→ 合并（AC5）。
- `briefing` / `narrate`：组装结构化事实 + 可选叙述。

## 5. LLM 调用策略（关键横切）
`omp -p` ~3.5s 起步且耗额度（grounded fact）→
- **绝不**在渲染/刷新路径调用；看板/学习/快报的结构化部分全规则派。
- 仅 `m`（挖掘）/`b`（快报叙述）手动触发；TUI 先画一帧"处理中…"状态再阻塞执行（`Pending` 队列），
  给用户明确反馈。
- `--no-llm` 让 CLI 快报只出结构化事实。
- 失败进状态栏，事实照常。

## 6. 交付层（delivery）
- `i18n`：`Locale{En,Zh}`，locale 自动探测，`l` 切换；所有面向用户的字串在此，领域不含语言。
- `cli`：`tutor board` / `tutor briefing`（`briefing_markdown` 可导出）。
- `tui`：header(tabs) + body + status + footer 四行布局；看板=三列 `List`（焦点列高亮、
  边框着色、方向键选择、←→ 切列）；学习=五阶段概览 + "今日要看"列表；快报=结构化+叙述段。
  以 ratatui `TestBackend` 渲染三页做冒烟单测（AC 之外的实测证据）。

## 7. 只读与隐私
- 运行期不写任何 `~/.omp`（AC8）。唯一落盘：`~/.cache/tutor/deck.json`（原子 temp+rename）。
- 缓存只存学习卡（topic/detail/project/stage/时间戳），不存凭证/邮箱/原始会话内容。

## 8. 备选方案（alternatives considered）
- **Web 看板**：拖拽体验好，但脱离终端、异栈、需起服务。放弃（用户选 TUI）。
- **自带模型 API key 作大脑**：不依赖 omp CLI，但要单独存 key/单独计费。放弃（用户选 `omp -p`）。
- **纯规则、不接 LLM**：最快最稳，但学习卡的 topic/stage 质量差。折中：规则派恒可用 + LLM 增强。
- **引入 chrono/dirs**：省去手写 ISO/XDG，但增依赖。放弃，与 headroom 一致地零额外依赖。

## 9. 风险
- 面包屑陈旧 → "在用"高估：以 recency 补偿，文档说明语义。
- 启发式学习候选噪声：LLM 挖掘 + topic 归一去重收敛；卡片可累计出现次数排序。
- 大会话文件全量读入：v0 可接受（headroom 同量级）；如遇超大文件可后续改流式（omp 内部 8MiB 阈值同理）。
