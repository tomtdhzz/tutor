//! Presentation-layer localization. Language lives here, never in the domain.

use crate::domain::study::LoopStage;
use crate::domain::WorkState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    /// Detect from `LC_ALL` / `LC_MESSAGES` / `LANG`; Chinese locales → `Zh`.
    pub fn detect() -> Locale {
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(key) {
                if v.to_ascii_lowercase().contains("zh") {
                    return Locale::Zh;
                }
            }
        }
        Locale::En
    }

    pub fn toggle(self) -> Locale {
        match self {
            Locale::En => Locale::Zh,
            Locale::Zh => Locale::En,
        }
    }

    pub fn from_flag(v: &str) -> Option<Locale> {
        match v.to_ascii_lowercase().as_str() {
            "zh" | "cn" | "zh-cn" => Some(Locale::Zh),
            "en" => Some(Locale::En),
            _ => None,
        }
    }

    pub fn tab(self, idx: usize) -> &'static str {
        match (self, idx) {
            (Locale::Zh, 0) => "学习",
            (Locale::Zh, 1) => "看板",
            (Locale::Zh, _) => "快报",
            (Locale::En, 0) => "Study",
            (Locale::En, 1) => "Board",
            (Locale::En, _) => "Brief",
        }
    }

    pub fn column(self, state: WorkState) -> &'static str {
        match (self, state) {
            (Locale::Zh, WorkState::Todo) => "待办",
            (Locale::Zh, WorkState::Doing) => "进行中",
            (Locale::Zh, WorkState::Done) => "已完成",
            (Locale::En, WorkState::Todo) => "To do",
            (Locale::En, WorkState::Doing) => "Doing",
            (Locale::En, WorkState::Done) => "Done",
        }
    }

    /// Short stage name.
    pub fn stage(self, s: LoopStage) -> &'static str {
        match (self, s) {
            (Locale::Zh, LoopStage::Preview) => "预习",
            (Locale::Zh, LoopStage::Class) => "听课",
            (Locale::Zh, LoopStage::Homework) => "作业",
            (Locale::Zh, LoopStage::Review) => "复习",
            (Locale::Zh, LoopStage::Correct) => "改错",
            (Locale::En, LoopStage::Preview) => "Preview",
            (Locale::En, LoopStage::Class) => "Class",
            (Locale::En, LoopStage::Homework) => "Homework",
            (Locale::En, LoopStage::Review) => "Review",
            (Locale::En, LoopStage::Correct) => "Correct",
        }
    }

    /// The one-line philosophy behind each stage.
    pub fn stage_saying(self, s: LoopStage) -> &'static str {
        match (self, s) {
            (Locale::Zh, LoopStage::Preview) => "找出不会的",
            (Locale::Zh, LoopStage::Class) => "解决不会的",
            (Locale::Zh, LoopStage::Homework) => "检验不会的",
            (Locale::Zh, LoopStage::Review) => "死磕不会的",
            (Locale::Zh, LoopStage::Correct) => "消灭不会的",
            (Locale::En, LoopStage::Preview) => "surface the unknowns",
            (Locale::En, LoopStage::Class) => "solve the unknowns",
            (Locale::En, LoopStage::Homework) => "test the unknowns",
            (Locale::En, LoopStage::Review) => "grind the unknowns",
            (Locale::En, LoopStage::Correct) => "eliminate the unknowns",
        }
    }

    pub fn header(self, total: usize, secs_ago: u64) -> String {
        match self {
            Locale::Zh => format!(" 私人教师 · {total} 个窗口 · {secs_ago}s 前刷新 "),
            Locale::En => format!(" tutor · {total} window(s) · updated {secs_ago}s ago "),
        }
    }

    pub fn footer(self, tab: usize) -> &'static str {
        match (self, tab) {
            (Locale::Zh, 0) => {
                " Tab/1-3 切换 · ↑↓/jk 选择 · m 挖掘学习 · r 刷新 · l 中/EN · q 退出 "
            }
            (Locale::Zh, 2) => " Tab/1-3 切换 · b 生成快报 · r 刷新 · l 中/EN · q 退出 ",
            (Locale::Zh, _) => " Tab/1-3 切换 · ←→ 列 · ↑↓/jk 选择 · r 刷新 · l 中/EN · q 退出 ",
            (Locale::En, 0) => {
                " Tab/1-3 switch · ↑↓/jk select · m mine · r refresh · l zh/EN · q quit "
            }
            (Locale::En, 2) => " Tab/1-3 switch · b brief · r refresh · l zh/EN · q quit ",
            (Locale::En, _) => {
                " Tab/1-3 switch · ←→ column · ↑↓/jk select · r refresh · l zh/EN · q quit "
            }
        }
    }

    pub fn live_tag(self) -> &'static str {
        match self {
            Locale::Zh => "在用",
            Locale::En => "LIVE",
        }
    }

    pub fn msgs(self, n: usize) -> String {
        match self {
            Locale::Zh => format!("{n} 条消息"),
            Locale::En => format!("{n} msgs"),
        }
    }

    pub fn ago(self, dur: &str) -> String {
        match self {
            Locale::Zh => format!("{dur}前"),
            Locale::En => format!("{dur} ago"),
        }
    }

    pub fn empty(self, tab: usize) -> &'static str {
        match (self, tab) {
            (Locale::Zh, 0) => "还没挖到\"不会的\"。按 m 让私人教师从最近会话里挖掘。",
            (Locale::Zh, 1) => "没有窗口/会话。启动一个 omp 会话后按 r 刷新。",
            (Locale::Zh, _) => "今天还没有可汇报的。按 b 生成快报。",
            (Locale::En, 0) => "No unknowns yet. Press m to mine them from recent sessions.",
            (Locale::En, 1) => "No windows/sessions. Start an omp session and press r.",
            (Locale::En, _) => "Nothing to report yet. Press b to generate the briefing.",
        }
    }

    pub fn study_due(self) -> &'static str {
        match self {
            Locale::Zh => "今日要看（不会的）",
            Locale::En => "Due today (things you don't know)",
        }
    }

    pub fn brief_section(self, which: usize) -> &'static str {
        match (self, which) {
            (Locale::Zh, 0) => "今日进行",
            (Locale::Zh, 1) => "今日完成",
            (Locale::Zh, 2) => "等待/未开始",
            (Locale::Zh, _) => "待复习",
            (Locale::En, 0) => "In progress today",
            (Locale::En, 1) => "Finished today",
            (Locale::En, 2) => "Waiting / not started",
            (Locale::En, _) => "To review",
        }
    }

    pub fn status_mining(self) -> &'static str {
        match self {
            Locale::Zh => "正在从最近会话挖掘\"不会的\"…（omp -p，稍候）",
            Locale::En => "Mining unknowns from recent sessions… (omp -p)",
        }
    }

    pub fn status_briefing(self) -> &'static str {
        match self {
            Locale::Zh => "正在生成每日快报…（omp -p，稍候）",
            Locale::En => "Generating daily briefing… (omp -p)",
        }
    }

    pub fn status_mined(self, n: usize) -> String {
        match self {
            Locale::Zh => format!("挖掘完成：学习库现有 {n} 条"),
            Locale::En => format!("Mined. Deck now holds {n} card(s)."),
        }
    }

    pub fn status_refreshed(self) -> &'static str {
        match self {
            Locale::Zh => "已刷新",
            Locale::En => "Refreshed",
        }
    }

    pub fn status_error(self, e: &str) -> String {
        match self {
            Locale::Zh => format!("出错：{e}"),
            Locale::En => format!("Error: {e}"),
        }
    }

    pub fn course_tab(self, idx: usize) -> &'static str {
        match (self, idx) {
            (Locale::Zh, 0) => "复习",
            (Locale::Zh, 1) => "课程",
            (Locale::Zh, _) => "进度",
            (Locale::En, 0) => "Review",
            (Locale::En, 1) => "Course",
            (Locale::En, _) => "Progress",
        }
    }

    pub fn course_header(self, subject: &str, mastered: usize, total: usize) -> String {
        match self {
            Locale::Zh => format!(" 私人教师 · {subject} · 已掌握 {mastered}/{total} "),
            Locale::En => format!(" tutor · {subject} · {mastered}/{total} mastered "),
        }
    }

    pub fn course_footer(self, tab: usize) -> &'static str {
        match (self, tab) {
            (Locale::Zh, 1) => {
                " Tab/1-3 · ←→ 列 · ↑↓ 选 · . 进阶 · , 退阶 · 空格 复习 · r 重载路线 · l 中/EN · q "
            }
            (Locale::Zh, _) => " Tab/1-3 切换 · r 重载路线(roadmap.md) · l 中/EN · q 退出 ",
            (Locale::En, 1) => {
                " Tab/1-3 · ←→ col · ↑↓ pick · . advance · , back · space review · r reload · l · q "
            }
            (Locale::En, _) => " Tab/1-3 switch · r reload roadmap.md · l zh/EN · q quit ",
        }
    }

    pub fn course_empty(self) -> &'static str {
        match self {
            Locale::Zh => "这门课还没有路线。编辑 roadmap.md 后按 r 重载。",
            Locale::En => "No roadmap yet. Edit roadmap.md and press r to reload.",
        }
    }

    /// Title of the dated review agenda (复习计划).
    pub fn review_plan(self) -> &'static str {
        match self {
            Locale::Zh => "复习计划",
            Locale::En => "Review plan",
        }
    }

    /// Bucket label: 0 overdue, 1 today, 2 this week, 3 later.
    pub fn review_bucket(self, which: usize) -> &'static str {
        match (self, which) {
            (Locale::Zh, 0) => "逾期",
            (Locale::Zh, 1) => "今天",
            (Locale::Zh, 2) => "本周",
            (Locale::Zh, _) => "以后",
            (Locale::En, 0) => "Overdue",
            (Locale::En, 1) => "Today",
            (Locale::En, 2) => "This week",
            (Locale::En, _) => "Later",
        }
    }

    /// "empty" note for the review agenda when nothing has reached Review yet.
    pub fn review_plan_empty(self) -> &'static str {
        match self {
            Locale::Zh => "暂无复习项 — 把话题推进到\"复习\"阶段后会自动排期。",
            Locale::En => "No reviews scheduled — walk a topic into Review to populate this.",
        }
    }
}
