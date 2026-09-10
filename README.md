# tutor

[![CI](https://github.com/tomtdhzz/tutor/actions/workflows/ci.yml/badge.svg)](https://github.com/tomtdhzz/tutor/actions/workflows/ci.yml)

**Your local omp tutor: it turns the "things you don't know" scattered across your coding sessions into a course with a review schedule — and keeps an eye on every window and files you a daily briefing.**

[English](README.md) · [简体中文](README.zh-CN.md)

`● tutor`  ·  **PREVIEW › CLASS › HOMEWORK › REVIEW › CORRECT**  ·  read-only

**headroom** (quota) × **tutor** (learning) × **omp** (the agent)

You learn by doing: you run [`omp`](https://github.com/can1357/oh-my-pi) across many
terminal tabs, ask it questions, hit errors, and move on. The problem is that the *things
you didn't know* evaporate — no one collects them, and nothing brings them back for review.
`headroom` (its sibling) tells you how much *quota* each model has left. `tutor` is the
teacher for the rest:

> *What don't I know yet, and when should I review it? What is each tab actually working on
> and how far along is it? And — what did I do today?*

`tutor` is **read-only**: it never changes your `omp` config or switches anything. It reads
the session store under `~/.omp/agent`, turns it into a terminal dashboard (drop it into a
zellij/tmux pane next to `headroom`), and — only when you ask — borrows `omp -p` to distill
your unknowns and narrate the day.

The whole UI toggles between English and 中文 at runtime (`l`), so a single build serves both.

## The effect

```
Study · due today (5)
  [Preview]   Rust lifetimes & the borrow checker            ×3
  [Review]    ratatui wide-char cell layout                  ×2   ← due today
  [Class]     the title slot in omp session JSONL            ×1

  Preview   surface the unknowns     12 ▍▍▍▍▍▍▍▍▍▍▍▍
  Class     solve the unknowns        5 ▍▍▍▍▍
  Homework  test the unknowns         2 ▍▍
  Review    grind the unknowns        4 ▍▍▍▍
  Correct   eliminate the unknowns    7 ▍▍▍▍▍▍▍

Board · Doing (3)
  Refine TUI activity meter and tests   [LIVE]
  ██████████████ ~    tutor · 2s ago
```

## What it does

### 1 · Study loop — the heart
The organising idea, in one breath: *preview surfaces what you don't know, class solves it,
homework tests it, review grinds it, correcting errors eliminates it.* `tutor` auto-mines
the "things you don't know" (your questions, the errors you hit) from recent sessions into
cards and cycles each through **Preview → Class → Homework → Review → Correct**. The Review
stage runs a spaced-repetition schedule (1/3/7/14/30 days), so the deck always shows what is
**due today**. Mining works offline (heuristic); press `m` and `omp -p` distills sharper
topics and assigns each a stage.

### 2 · Work board
Your tutor also keeps track of what you're working on. Every terminal tab / session becomes a
card; the **proposition** is auto-inferred from the session title, and cards fall into **To
do / Doing / Done** by a rule-based classifier (recent activity, todo completion, lifecycle).
Cards bound to a live terminal are tagged **LIVE**. Progress is the todo completion percentage
when a session declared todos, or an honest message-volume activity meter (marked `~`) when it
didn't — it never fakes "done".

### 3 · Daily briefing
A rolled-up account of the day — what moved, what finished, what is waiting on you, and what to
review — as structured facts, optionally narrated into prose by `omp -p`, and exportable as
Markdown.

## Courses — point the tutor at a subject

Give the tutor an empty folder and a subject and it drafts a **learning roadmap**
(grounded on well-known public paths — NeetCode / roadmap.sh / LeetCode patterns for
algorithms) into an editable `roadmap.md`, then opens a **course kanban** you drive by
hand. Each roadmap topic is a card moving through Preview → Class → Homework → Review →
Correct; `- [x]` in `roadmap.md` marks a topic mastered, and `- [ ]` never resets your
progress. The course is self-contained in its folder (`roadmap.md` + `.tutor/deck.json`),
so it's git-friendly and portable.

```bash
# Draft an algorithms roadmap into ./algo and seed the course
tutor course new ./algo --subject "algorithms"

# (edit ./algo/roadmap.md — reorder, add, check off `- [x]` what you already know)

# Open the interactive course dashboard (Review / Course / Progress tabs)
tutor course ./algo

# Or print the course kanban once
tutor course board ./algo
```

In the course dashboard: `←→`/`↑↓` pick a card, `.` advance a stage, `,` step back,
`space` mark reviewed (schedules the next spaced review), `p` open the topic's
**lesson** (practice problems + solutions), `r` reloads roadmap edits. The roadmap
and every lesson are drafted in your `--lang` (e.g. `tutor course new ./algo --subject "算法" --lang zh`
writes them in Chinese). Offline? `tutor course new … --no-llm` writes a starter roadmap you fill in yourself.

**Scriptable & observable (no TUI needed).** Every stage action has a one-shot
command, and `course state` prints the whole course — progress, per-stage counts,
and a dated **review plan** (复习计划) — as text or `--json`, so you (or a script)
can drive and verify the loop without the full-screen dashboard:

```bash
tutor course state ./algo                 # human-readable state + review plan
tutor course state ./algo --json          # machine-readable (stable schema)
tutor course advance ./algo --topic "binary search"   # one stage toward mastery
tutor course demote  ./algo --topic "binary search"   # one stage back
tutor course review  ./algo --topic "binary search"   # record a spaced review
tutor course lesson  ./algo --topic "binary search"   # draft/print the topic's lesson (problems + solutions)
```

A topic only joins the review plan once it reaches **Review**; each `review`
pushes its next date out on the 1·3·7·14·30-day ladder, and mastery drops it off.

**Each topic is a lesson you work, one problem at a time.** The roadmap lists topic
*names*; `course lesson` (or `p`/Enter in the TUI) has the brain expand one into a
**preview lesson** — a short overview plus 3–5 practice **problems** of increasing
difficulty, each with a full **solution** (idea, steps, code, complexity). In the TUI
you study it like a course: one problem shows at a time, you **attempt it first**, press
`space` to reveal the **题解**, then `Enter` to mark it **已掌握** — and `←→` moves to the
next problem. Your progress is counted **by problems solved**, not whole topics, so the
bar and the per-card `题 s/N` badge move as you go; finishing all of a topic's problems
masters it. Lessons are cached to `<dir>/.tutor/lessons/<id>.md` (editable Markdown,
portable, git-friendly) and your marks to the deck, so reopening is instant; `g` redrafts.
Generation runs in the **background** — the UI never freezes while `omp -p` works — and you
pick the **code language** by pressing `c` in the lesson to open a language picker (or `--code rust` on `course new`/`lesson`,
persisted per course).

**It closes the loop with your sessions.** Any omp session whose cwd is inside the
course folder is folded in: the "things you don't know" it surfaces (your questions,
the errors you hit) join the deck as extra cards, and a roadmap topic you've started
touching — named in a session or a file in the folder (e.g. `recursion_solver.py`) —
auto-advances Preview → Class. So the roadmap says what you *should* know; your sessions
reveal what you *don't* yet; mastery still stays a deliberate act (`- [x]` or `.`).

### Pick a 课题, read the 题解 — step by step

Two ways: the interactive dashboard, or a one-shot command.

**In the dashboard** — `tutor course ./algo`:

1. It opens on the **Course** kanban (To do / Doing / Done). Each card is a roadmap
   topic — your **课题**.
2. Move the selection with `←→` (switch column) and `↑↓` / `jk` (move within a
   column) to the topic you want to study.
3. Press **`p`** (or `Enter`, and also on the **复习** tab) to open that topic's
   **lesson** — it shows **one problem at a time** with a `题目 i/N` counter.
4. For each problem: read it, **try it yourself**, press **`space`** to reveal the
   `题解`, then **`Enter`** to mark it **已掌握**. **`←→`** move to the prev/next
   problem · `↑↓`/`jk` scroll · **`g`** redraft via `omp -p` · **`esc`** back to the
   kanban. Your marks persist, and progress is counted by solved problems.

The first `p` (or `g`) on a topic calls `omp -p` — a few seconds, with `正在生成讲义…`
in the status line. After that the lesson is cached, so reopening it is instant.

**One-shot** — no dashboard, prints straight to the terminal:

```bash
# Pick the 课题 by a case-insensitive substring of its name; prints overview + 题目/题解
tutor course lesson ./algo --topic "binary search"
tutor course lesson ./algo --topic "二分" --lang zh   # a short fragment is enough
```

- `--topic` matches the first roadmap topic that contains the text — you don't need
  the full name.
- `--lang zh|en` forces the language the lesson is drafted in.
- To redraft from scratch, delete the cached file (`rm ./algo/.tutor/lessons/<id>.md`)
  or press `g` in the dashboard.

## Prerequisites

Platforms: macOS or Linux.

1. **omp** ([Oh My Pi](https://github.com/can1357/oh-my-pi)) — on your `PATH`. `tutor` reads
   its session store under `~/.omp/agent`, and the "brain" shells out to `omp -p` using your
   logged-in accounts (only for the `m`/`b` actions and `tutor briefing`).
2. **Rust toolchain 1.88+** to build from source (via [rustup](https://rustup.rs)).

Dependencies are pure Rust (serde, serde_json, anyhow, ratatui, crossterm) and built by cargo.

## Install

With cargo, straight from the repo (no clone):

```bash
cargo install --git https://github.com/tomtdhzz/tutor
```

Or from a local checkout:

```bash
git clone https://github.com/tomtdhzz/tutor
cd tutor
cargo install --path .        # or: cargo run --release -- tui
```

Once a release is tagged, a Homebrew formula is published to the tap:

```bash
brew install tomtdhzz/tap/tutor
```

## Usage

```bash
# One-shot: print the work board and exit
tutor
tutor board

# Interactive dashboard (Study / Board / Brief tabs)
tutor tui

# Compose today's briefing as Markdown (narrated via omp -p)
tutor briefing > today.md

# Structured facts only, no LLM call
tutor briefing --no-llm

# Force a language (auto-detected from locale otherwise; order-independent)
tutor --lang en
tutor board --lang zh

# Open a subject course, then press p on a topic to study its 题目 + 题解
tutor course ./algo
tutor course lesson ./algo --topic "binary search"   # or one-shot to the terminal
```

TUI keys (window mode): `Tab` / `1` `2` `3` switch tabs · `←→` move column, `↑↓`/`jk` select ·
`m` mine study (omp -p) · `b` generate briefing (omp -p) · `r` refresh · `l` 中/EN · `q` quit.

Course-mode keys (`tutor course <dir>`): `←→`/`↑↓` pick a topic · `.` advance · `,` back ·
`space` mark reviewed · **`p` open the topic's lesson** · `r` reload roadmap · `l` 中/EN · `q` quit.
In the lesson (one problem at a time): `←→` prev/next · **`space` reveal 题解** · **`Enter` mark 已掌握** · `↑↓` scroll · **`c` code language** · `g` regenerate · `esc` back.

Options:

| Flag | Meaning |
|---|---|
| `--lang <zh\|en>` | Display language (default: auto-detect from `LANG`/`LC_*`) |
| `--no-llm` | Do not call `omp -p` (`briefing` prints rule-based facts only) |
| `-h, --help` | Print help |
| `-v, --version` | Print version |

Environment: `TUTOR_OMP_DIR` overrides the omp agent directory (default `~/.omp/agent`).

## How it reads your sessions

- `~/.omp/agent/terminal-sessions/<id>` — a breadcrumb binding a terminal tab to a session
  file (line 1 = cwd, line 2 = path). This is the **LIVE** signal.
- `~/.omp/agent/sessions/<cwd>/<ts>_<id>.jsonl` — the transcript. `tutor` reads the session
  title (→ proposition), message/tool counts and timestamps (→ activity, recency),
  `session_exit` (→ lifecycle), any `user_todo_edit` (→ progress), and cue-bearing user lines
  (→ study candidates). Malformed lines are skipped, never fatal. `tutor`'s own `omp -p` calls
  run with `--no-session`, so the tutor never ingests its own prompts.

## Architecture

A lightweight hexagon; the domain is pure and IO-free (mirrors its sibling `headroom`).

```
delivery/{cli,tui,i18n} ─┐
main ─────────┤→ app (Tutor use case + ports)
              │        │
              │        └→ domain (StudyDeck/Unknown/LoopStage, Board/WorkItem/WorkState,
              │                    DailyBriefing)
              └ adapters: omp_sessions · omp_summarize (omp -p) · cache_file · clock ─┘
```

Rule-based data (study schedule, board, progress, structured briefing) renders instantly with
**no** LLM call; `omp -p` only enriches (study distillation, briefing prose) and is always
behind an explicit key. All user-facing strings live in `delivery/i18n` (zh/EN); the domain
holds no language. See [`docs/prd/PRD.md`](docs/prd/PRD.md) and
[`docs/tech-design/tech-design.md`](docs/tech-design/tech-design.md).

## Privacy

Read-only with respect to omp: `tutor` never writes anything under `~/.omp`. The only thing it
persists is the study deck at `$XDG_CACHE_HOME/tutor/deck.json` (or `~/.cache/tutor/deck.json`)
— topics/details/stages/timestamps, never credentials, emails, or raw session content.

## Test

```bash
cargo test                                  # unit + TUI render tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Limitations

- **Read-only.** v0 observes and teaches; it does not act on omp.
- **Loop stage is inferred.** v0 assigns study stages heuristically or via `omp -p`; manual card
  actions (promote stage, mark resolved, reschedule a review) are planned for v1.
- **Heuristic mining is noisy.** Offline candidates are raw questions/errors; `m` (omp -p) is
  what turns them into sharp, deduplicated knowledge points.
- **Todos are sparse**, so board progress usually falls back to a message-volume activity proxy
  (marked `~`), not true completion. **"LIVE" means tab-bound, not process-alive** — terminal
  breadcrumbs persist after a tab closes; recency carries the freshness signal.
- **The brain is `omp -p`.** Mining/narration cost a few seconds and some quota; they run only
  on `m`/`b`, never on the render path.

## Roadmap

- **v1** — manual card actions in the TUI (promote stage / mark resolved / reschedule); a
  `watch` mode with auto-refresh.
- **v2** — richer progress from tool-call/diff signals; per-project board filter; on-disk
  briefing cache per day.

## License

MIT — see [LICENSE](LICENSE).

## Disclaimer

Independent project, not affiliated with or endorsed by the `omp` / Oh My Pi authors,
Anthropic, or OpenAI. Provider and omp interfaces may change.
