//! Interactive full-screen TUI (ratatui + crossterm), localized via `Locale`.
//!
//! Three tabs over the same `Tutor`:
//! - Board (看板): live windows/tabs as kanban cards in To do / Doing / Done.
//! - Study (学习): the 预习→改错 loop over auto-mined unknowns, with a due list.
//! - Brief (快报): today's structured rollup, optionally narrated by `omp -p`.
//!
//! Rule-based data renders instantly. `m` (mine) and `b` (brief) are the only
//! actions that call the LLM; both draw a status frame first, then block briefly.

use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::i18n::Locale;
use super::{bar, date_of, today_local, truncate};
use crate::adapters::CourseDir;
use crate::app::{DeckStore, ScannedSession, Summarizer, Tutor};
use crate::domain::lesson::Lesson;
use crate::domain::study::{LoopStage, Unknown};
use crate::domain::{
    course, days_between, human_ago, Board, Column, CourseProgress, DailyBriefing, ReviewPlan,
    StudyDeck, WorkItem, WorkState,
};

const ACCENT: Color = Color::Green;

#[derive(Clone, Copy, PartialEq)]
enum Pending {
    None,
    Mine,
    Brief,
    Lesson,
}

/// The lesson overlay: a topic's 题目/题解, scrollable, with a self-test toggle.
struct LessonView {
    id: String,
    topic: String,
    lesson: Lesson,
    scroll: u16,
    show_solutions: bool,
}

impl LessonView {
    fn new(id: String, topic: String, lesson: Lesson) -> LessonView {
        LessonView {
            id,
            topic,
            lesson,
            scroll: 0,
            show_solutions: true,
        }
    }
}
/// Course mode binds the dashboard to a subject folder instead of the omp windows.
struct CourseCtx {
    dir: PathBuf,
    subject: String,
}

struct App<'a> {
    tutor: &'a Tutor<'a>,
    store: &'a dyn DeckStore,
    summarizer: &'a dyn Summarizer,
    locale: Locale,
    tab: usize,
    scans: Vec<ScannedSession>,
    board: Board,
    deck: StudyDeck,
    brief: Option<DailyBriefing>,
    updated: SystemTime,
    status: String,
    pending: Pending,
    col: usize,
    rows: [usize; 3],
    study_row: usize,
    /// `Some` when the dashboard is scoped to a subject course.
    course: Option<CourseCtx>,
    /// `Some` while the lesson overlay is open over the course kanban.
    lesson: Option<LessonView>,
    /// A queued lesson to draft via the brain: `(topic id, topic title)`.
    lesson_req: Option<(String, String)>,
}

impl<'a> App<'a> {
    fn new(
        tutor: &'a Tutor<'a>,
        store: &'a dyn DeckStore,
        summarizer: &'a dyn Summarizer,
        locale: Locale,
        course: Option<CourseCtx>,
    ) -> Result<App<'a>> {
        let course_mode = course.is_some();
        let mut app = App {
            tutor,
            store,
            summarizer,
            locale,
            // Course opens on its kanban (tab 1); window mode opens on Study (tab 0).
            tab: if course_mode { 1 } else { 0 },
            scans: Vec::new(),
            board: Board::build(Vec::new(), tutor.now()),
            deck: StudyDeck::default(),
            brief: None,
            updated: tutor.now(),
            status: String::new(),
            pending: Pending::None,
            col: if course_mode { 0 } else { 1 },
            rows: [0; 3],
            study_row: 0,
            course,
            lesson: None,
            lesson_req: None,
        };
        app.refresh()?;
        app.status.clear();
        Ok(app)
    }

    /// Reload state. In course mode: load the deck, re-merge the (possibly hand-
    /// edited) roadmap, persist. In window mode: re-scan omp and reseed heuristics.
    fn refresh(&mut self) -> Result<()> {
        if let Some(c) = &self.course {
            let base = self.store.load().unwrap_or_default();
            let md = std::fs::read_to_string(c.dir.join("roadmap.md")).unwrap_or_default();
            let signals = super::dir_signals(&c.dir);
            self.deck = self
                .tutor
                .reconcile_course(&c.dir, &c.subject, &md, base, &signals);
            let _ = self.store.save(&self.deck);
        } else {
            self.scans = self.tutor.scan()?;
            self.board = self.tutor.board(&self.scans);
            let base = self.store.load().unwrap_or_default();
            self.deck = self.tutor.seed_heuristic(&self.scans, base);
            let _ = self.store.save(&self.deck);
        }
        self.brief = None;
        self.updated = self.tutor.now();
        self.status = self.locale.status_refreshed().to_string();
        self.clamp();
        Ok(())
    }

    fn mine(&mut self) {
        let base = self.deck.clone();
        match self.tutor.mine(self.summarizer, &self.scans, base) {
            Ok(deck) => {
                let _ = self.store.save(&deck);
                self.deck = deck;
                self.status = self.locale.status_mined(self.deck.cards.len());
            }
            Err(e) => self.status = self.locale.status_error(&e.to_string()),
        }
        self.clamp();
    }

    fn make_brief(&mut self) {
        let mut brief = self.tutor.briefing(&self.board, &self.deck);
        let date = today_local();
        if let Err(e) = self.tutor.narrate(self.summarizer, &mut brief, &date) {
            self.status = self.locale.status_error(&e.to_string());
        } else {
            self.status.clear();
        }
        self.brief = Some(brief);
    }

    fn columns(&self) -> [&Column; 3] {
        self.board.columns()
    }

    /// Item counts of the active board's three columns (course kanban or windows).
    fn board_col_lens(&self) -> [usize; 3] {
        if self.course.is_some() {
            let [a, b, c] = course::columns(&self.deck);
            [a.len(), b.len(), c.len()]
        } else {
            self.board.columns().map(|c| c.items.len())
        }
    }

    /// The course card currently selected on the kanban (course mode, tab 1).
    fn selected_card_id(&self) -> Option<String> {
        self.course.as_ref()?;
        let cols = course::columns(&self.deck);
        cols[self.col.min(2)]
            .get(self.rows[self.col.min(2)])
            .map(|u| u.id.clone())
    }

    fn edit_selected(&mut self, f: impl Fn(&mut Unknown, SystemTime)) {
        let now = self.tutor.now();
        if let Some(id) = self.selected_card_id() {
            if let Some(card) = self.deck.get_mut(&id) {
                f(card, now);
                let _ = self.store.save(&self.deck);
            }
        }
        self.clamp();
    }

    /// The topic title of the currently selected kanban card, if any.
    fn selected_topic(&self) -> Option<(String, String)> {
        let id = self.selected_card_id()?;
        let topic = self
            .deck
            .cards
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.topic.clone())
            .unwrap_or_default();
        Some((id, topic))
    }

    /// Open the lesson overlay for the selected topic. Shows a cached lesson
    /// instantly; otherwise queues a draft via the brain (blocking, like `mine`).
    fn open_lesson(&mut self) {
        let Some(c) = &self.course else { return };
        let Some((id, topic)) = self.selected_topic() else {
            return;
        };
        let course = CourseDir::new(&c.dir);
        match course.read_lesson(&id) {
            Ok(Some(md)) => {
                let lesson = Lesson::parse(&md, &topic);
                self.lesson = Some(LessonView::new(id, topic, lesson));
            }
            _ => {
                self.status = self.locale.lesson_generating(&topic);
                self.lesson_req = Some((id, topic));
                self.pending = Pending::Lesson;
            }
        }
    }

    /// Re-draft the lesson currently on screen, overwriting its cache.
    fn regen_lesson(&mut self) {
        if let Some(v) = &self.lesson {
            self.status = self.locale.lesson_generating(&v.topic);
            self.lesson_req = Some((v.id.clone(), v.topic.clone()));
            self.pending = Pending::Lesson;
        }
    }

    /// Draft the queued lesson via the brain, cache it, and open the overlay.
    fn make_lesson(&mut self) {
        let Some((id, topic)) = self.lesson_req.take() else {
            return;
        };
        let Some(c) = &self.course else { return };
        let course = CourseDir::new(&c.dir);
        let subject = c.subject.clone();
        match self
            .tutor
            .generate_lesson(self.summarizer, &subject, &topic, self.locale.lang_hint())
        {
            Ok(md) => {
                let _ = course.write_lesson(&id, &md);
                let lesson = Lesson::parse(&md, &topic);
                self.lesson = Some(LessonView::new(id, topic, lesson));
                self.status.clear();
            }
            Err(e) => {
                self.status = self.locale.status_error(&e.to_string());
            }
        }
    }

    /// Upper bound for the lesson overlay's scroll offset (keeps at least the last
    /// line reachable). Zero when no lesson is open.
    fn lesson_scroll_max(&self) -> u16 {
        match &self.lesson {
            Some(v) => (lesson_lines(v, self.locale).len().saturating_sub(1)) as u16,
            None => 0,
        }
    }

    fn clamp(&mut self) {
        self.col = self.col.min(2);
        let lens = self.board_col_lens();
        for (row, len) in self.rows.iter_mut().zip(lens) {
            *row = (*row).min(len.saturating_sub(1));
        }
        let due = self.deck.due(self.tutor.now()).len();
        self.study_row = self.study_row.min(due.saturating_sub(1));
    }

    fn down(&mut self) {
        match self.tab {
            0 => {
                let due = self.deck.due(self.tutor.now()).len();
                if due > 0 && self.study_row + 1 < due {
                    self.study_row += 1;
                }
            }
            1 => {
                let len = self.board_col_lens()[self.col];
                if len > 0 && self.rows[self.col] + 1 < len {
                    self.rows[self.col] += 1;
                }
            }
            _ => {}
        }
    }

    fn up(&mut self) {
        match self.tab {
            0 => self.study_row = self.study_row.saturating_sub(1),
            1 => self.rows[self.col] = self.rows[self.col].saturating_sub(1),
            _ => {}
        }
    }

    fn left(&mut self) {
        if self.tab == 1 {
            self.col = self.col.saturating_sub(1);
        }
    }
    fn right(&mut self) {
        if self.tab == 1 {
            self.col = (self.col + 1).min(2);
        }
    }
}

/// Enter the alternate screen, run the event loop, and always restore.
pub fn run(
    tutor: &Tutor,
    store: &dyn DeckStore,
    summarizer: &dyn Summarizer,
    locale: Locale,
) -> Result<()> {
    let mut app = App::new(tutor, store, summarizer, locale, None)?;
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

/// Enter course mode: the dashboard is scoped to a subject folder.
pub fn run_course(
    tutor: &Tutor,
    store: &dyn DeckStore,
    summarizer: &dyn Summarizer,
    locale: Locale,
    dir: PathBuf,
    subject: String,
) -> Result<()> {
    let mut app = App::new(
        tutor,
        store,
        summarizer,
        locale,
        Some(CourseCtx { dir, subject }),
    )?;
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        // Execute a queued LLM action right after its status frame is drawn.
        match app.pending {
            Pending::Mine => {
                app.pending = Pending::None;
                app.mine();
                continue;
            }
            Pending::Brief => {
                app.pending = Pending::None;
                app.make_brief();
                continue;
            }
            Pending::Lesson => {
                app.pending = Pending::None;
                app.make_lesson();
                continue;
            }
            Pending::None => {}
        }

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                // While the lesson overlay is open it captures every key.
                if app.lesson.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => app.lesson = None,
                        KeyCode::Char('l') => app.locale = app.locale.toggle(),
                        KeyCode::Char('s') => {
                            if let Some(v) = app.lesson.as_mut() {
                                v.show_solutions = !v.show_solutions;
                            }
                        }
                        KeyCode::Char('g') if app.course.is_some() => app.regen_lesson(),
                        KeyCode::Down | KeyCode::Char('j') => {
                            let max = app.lesson_scroll_max();
                            if let Some(v) = app.lesson.as_mut() {
                                v.scroll = (v.scroll + 1).min(max);
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if let Some(v) = app.lesson.as_mut() {
                                v.scroll = v.scroll.saturating_sub(1);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('l') => app.locale = app.locale.toggle(),
                    KeyCode::Char('r') => app.refresh()?,
                    KeyCode::Tab => app.tab = (app.tab + 1) % 3,
                    KeyCode::Char('1') => app.tab = 0,
                    KeyCode::Char('2') => app.tab = 1,
                    KeyCode::Char('3') => app.tab = 2,
                    KeyCode::Char('m') if app.tab == 0 && app.course.is_none() => {
                        app.status = app.locale.status_mining().to_string();
                        app.pending = Pending::Mine;
                    }
                    KeyCode::Char('b') if app.tab == 2 && app.course.is_none() => {
                        app.status = app.locale.status_briefing().to_string();
                        app.pending = Pending::Brief;
                    }
                    // Course kanban (tab 1): manual stage advancement.
                    KeyCode::Char('.') | KeyCode::Char('>')
                        if app.course.is_some() && app.tab == 1 =>
                    {
                        app.edit_selected(|c, now| c.promote(now));
                    }
                    KeyCode::Char(',') | KeyCode::Char('<')
                        if app.course.is_some() && app.tab == 1 =>
                    {
                        app.edit_selected(|c, now| c.demote(now));
                    }
                    KeyCode::Char(' ') if app.course.is_some() && app.tab == 1 => {
                        app.edit_selected(|c, now| c.reschedule(now));
                    }
                    KeyCode::Char('p') if app.course.is_some() && app.tab == 1 => {
                        app.open_lesson();
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.down(),
                    KeyCode::Up | KeyCode::Char('k') => app.up(),
                    KeyCode::Left | KeyCode::Char('h') => app.left(),
                    KeyCode::Right => app.right(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn ui(frame: &mut Frame, app: &App) {
    let [header, body, status, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(header_line(app), header);
    match app.tab {
        0 => study_tab(frame, app, body),
        1 if app.course.is_some() => course_board_tab(frame, app, body),
        1 => board_tab(frame, app, body),
        _ if app.course.is_some() => course_brief_tab(frame, app, body),
        _ => brief_tab(frame, app, body),
    }
    if let Some(view) = &app.lesson {
        lesson_overlay(frame, view, app.locale, body);
    }

    let status_line = Paragraph::new(Line::from(Span::styled(
        format!(" {}", app.status),
        Style::default().fg(Color::Yellow),
    )));
    frame.render_widget(status_line, status);

    let footer_text = if app.lesson.is_some() {
        app.locale.lesson_footer()
    } else if app.course.is_some() {
        app.locale.course_footer(app.tab)
    } else {
        app.locale.footer(app.tab)
    };
    let hint = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(hint, footer);
}

fn header_line(app: &App) -> Paragraph<'static> {
    let secs = app
        .tutor
        .now()
        .duration_since(app.updated)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut spans = vec![Span::styled(
        " 私人教师 ",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];
    for i in 0..3 {
        let active = i == app.tab;
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let label = if app.course.is_some() {
            app.locale.course_tab(i)
        } else {
            app.locale.tab(i)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    let summary = if let Some(c) = &app.course {
        let p = CourseProgress::of(&app.deck);
        app.locale.course_header(&c.subject, p.mastered, p.total)
    } else {
        app.locale.header(app.board.total(), secs)
    };
    spans.push(Span::styled(summary, Style::default().fg(Color::DarkGray)));
    Paragraph::new(Line::from(spans))
}

fn board_tab(frame: &mut Frame, app: &App, area: Rect) {
    if app.board.total() == 0 {
        frame.render_widget(empty(app, 1), area);
        return;
    }
    let cols = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(area);
    let now = app.tutor.now();
    for (i, col) in app.columns().iter().enumerate() {
        let focused = i == app.col;
        let items: Vec<ListItem> = col
            .items
            .iter()
            .map(|it| ListItem::new(item_text(it, app.locale, now, state_color(col.state))))
            .collect();
        let title = format!(" {} ({}) ", app.locale.column(col.state), col.items.len());
        let border = if focused { ACCENT } else { Color::DarkGray };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border))
                    .title(Span::styled(
                        title,
                        Style::default().fg(border).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default();
        if focused && !col.items.is_empty() {
            state.select(Some(app.rows[i]));
        }
        frame.render_stateful_widget(list, cols[i], &mut state);
    }
}

fn state_color(state: crate::domain::WorkState) -> Color {
    match state {
        crate::domain::WorkState::Done => ACCENT,
        crate::domain::WorkState::Doing => Color::Cyan,
        crate::domain::WorkState::Todo => Color::Gray,
    }
}

/// Per-stage accent color for the 预习→改错 loop, so a card's stage reads at a
/// glance independent of which kanban column it sits in.
fn stage_color(s: LoopStage) -> Color {
    match s {
        LoopStage::Preview => Color::Gray,
        LoopStage::Class => Color::Cyan,
        LoopStage::Homework => Color::Yellow,
        LoopStage::Review => Color::Magenta,
        LoopStage::Correct => ACCENT,
    }
}

fn item_text(it: &WorkItem, locale: Locale, now: SystemTime, fill: Color) -> Text<'static> {
    let live = if it.terminal_id.is_some() {
        format!("  [{}]", locale.live_tag())
    } else {
        String::new()
    };
    let head = Line::from(vec![
        Span::styled(
            truncate(&it.proposition, 30),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(live, Style::default().fg(ACCENT)),
    ]);
    let gauge = match it.progress.pct() {
        Some(p) => Line::from(vec![
            Span::styled(bar(p), Style::default().fg(fill)),
            Span::raw(format!(" {p}%")),
        ]),
        None => Line::from(vec![
            Span::styled(bar(it.activity_level()), Style::default().fg(fill)),
            Span::styled(" ~", Style::default().fg(Color::DarkGray)),
        ]),
    };
    let meta = Line::from(Span::styled(
        format!(
            "{} · {}",
            truncate(it.project(), 16),
            locale.ago(&human_ago(it.last_activity, now)),
        ),
        Style::default().fg(Color::DarkGray),
    ));
    Text::from(vec![head, gauge, meta, Line::raw("")])
}

fn study_tab(frame: &mut Frame, app: &App, area: Rect) {
    let [top, bottom] = Layout::vertical([Constraint::Length(7), Constraint::Min(1)]).areas(area);

    // Stage overview: 预习 · 找出不会的 · N ▍▍▍
    let counts = app.deck.counts();
    let mut lines = Vec::new();
    for s in LoopStage::ALL {
        let n = counts[s.index()];
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<8}", app.locale.stage(s)),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<14}", app.locale.stage_saying(s)),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(format!("{n:>3} ", n = n), Style::default().fg(Color::White)),
            Span::styled("▍".repeat(n.min(24)), Style::default().fg(ACCENT)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 预习→听课→作业→复习→改错 "),
        ),
        top,
    );

    let now = app.tutor.now();
    let due = app.deck.due(now);
    if due.is_empty() {
        frame.render_widget(empty(app, 0), bottom);
        return;
    }
    let items: Vec<ListItem> = due
        .iter()
        .map(|u| {
            let head = Line::from(vec![
                Span::styled(
                    format!("[{}] ", app.locale.stage(u.stage)),
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    truncate(&u.topic, 48),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ×{}", u.times_seen),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let detail = Line::from(Span::styled(
                format!("    {}", truncate(&u.detail, 72)),
                Style::default().fg(Color::Gray),
            ));
            ListItem::new(Text::from(vec![head, detail]))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(Span::styled(
                    format!(" {} ({}) ", app.locale.study_due(), due.len()),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default();
    state.select(Some(app.study_row.min(due.len() - 1)));
    frame.render_stateful_widget(list, bottom, &mut state);
}

fn brief_tab(frame: &mut Frame, app: &App, area: Rect) {
    let date = today_local();
    let brief = match &app.brief {
        Some(b) if !b.is_empty() || b.prose.is_some() => b,
        _ => {
            // Show the rule-based rollup even before narration.
            let composed = app.tutor.briefing(&app.board, &app.deck);
            if composed.is_empty() {
                frame.render_widget(empty(app, 2), area);
                return;
            }
            return frame.render_widget(brief_paragraph(&composed, app.locale, &date), area);
        }
    };
    frame.render_widget(brief_paragraph(brief, app.locale, &date), area);
}

fn brief_paragraph(brief: &DailyBriefing, locale: Locale, date: &str) -> Paragraph<'static> {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(prose) = &brief.prose {
        for l in prose.lines() {
            lines.push(Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::White),
            )));
        }
        lines.push(Line::raw(""));
    }
    let sections = [
        (0usize, &brief.active),
        (1, &brief.finished),
        (2, &brief.waiting),
        (3, &brief.to_study),
    ];
    for (which, items) in sections {
        lines.push(Line::from(Span::styled(
            format!("{} ", locale.brief_section(which)),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        if items.is_empty() {
            lines.push(Line::from(Span::styled(
                "  —",
                Style::default().fg(Color::DarkGray),
            )));
        }
        for l in items {
            let detail = if l.detail.is_empty() {
                l.project.clone()
            } else {
                format!("{} · {}", l.project, l.detail)
            };
            lines.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    truncate(&l.proposition, 44),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("  · {detail}"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        lines.push(Line::raw(""));
    }
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} · {date} ", locale.tab(2))),
        )
        .wrap(Wrap { trim: false })
}

fn empty(app: &App, tab: usize) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        app.locale.empty(tab),
        Style::default().fg(Color::DarkGray),
    )))
    .block(Block::default().borders(Borders::ALL))
    .wrap(Wrap { trim: false })
}

fn course_board_tab(frame: &mut Frame, app: &App, area: Rect) {
    if app.deck.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                app.locale.course_empty(),
                Style::default().fg(Color::DarkGray),
            )))
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let cols = course::columns(&app.deck);
    let states = [WorkState::Todo, WorkState::Doing, WorkState::Done];
    let areas = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(area);
    let now = app.tutor.now();
    for (i, bucket) in cols.iter().enumerate() {
        let focused = i == app.col;
        let items: Vec<ListItem> = bucket
            .iter()
            .map(|u| {
                let mut head = vec![
                    Span::styled(
                        format!("[{}] ", app.locale.stage(u.stage)),
                        Style::default()
                            .fg(stage_color(u.stage))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        truncate(&u.topic, 24),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ];
                if u.stage == LoopStage::Review {
                    let d = days_between(now, u.next_review);
                    let (mark, color) = if d < 0 {
                        (" ⚑".to_string(), Color::Red)
                    } else if d == 0 {
                        (" ●".to_string(), Color::Yellow)
                    } else {
                        (format!(" +{d}d"), Color::DarkGray)
                    };
                    head.push(Span::styled(mark, Style::default().fg(color)));
                }
                let sub = Line::from(Span::styled(
                    format!("  {}", truncate(&u.detail, 24)),
                    Style::default().fg(Color::DarkGray),
                ));
                ListItem::new(Text::from(vec![Line::from(head), sub]))
            })
            .collect();
        let border = if focused { ACCENT } else { Color::DarkGray };
        let title = format!(" {} ({}) ", app.locale.column(states[i]), bucket.len());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border))
                    .title(Span::styled(
                        title,
                        Style::default().fg(border).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default();
        if focused && !bucket.is_empty() {
            state.select(Some(app.rows[i].min(bucket.len() - 1)));
        }
        frame.render_stateful_widget(list, areas[i], &mut state);
    }
}

/// Build the styled lines of a lesson overlay: topic, overview, then each problem
/// with its 题解 (or a hidden note when solutions are toggled off for self-test).
fn lesson_lines(view: &LessonView, locale: Locale) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    if view.lesson.is_empty() {
        lines.push(Line::from(Span::styled(
            locale.lesson_absent(),
            Style::default().fg(Color::DarkGray),
        )));
        return lines;
    }
    if !view.lesson.overview.is_empty() {
        for l in view.lesson.overview.lines() {
            lines.push(Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Gray),
            )));
        }
        lines.push(Line::from(""));
    }
    for (i, p) in view.lesson.problems.iter().enumerate() {
        let label = if p.title.is_empty() {
            locale.lesson_problem(i + 1)
        } else {
            p.title.clone()
        };
        lines.push(Line::from(Span::styled(
            format!("── {label} ──"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for l in p.prompt.lines() {
            lines.push(Line::from(l.to_string()));
        }
        lines.push(Line::from(Span::styled(
            format!("[{}]", locale.lesson_solution()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        if view.show_solutions {
            for l in p.solution.lines() {
                lines.push(Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::Cyan),
                )));
            }
        } else {
            lines.push(Line::from(Span::styled(
                locale.lesson_solution_hidden().to_string(),
                Style::default().fg(Color::DarkGray),
            )));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn lesson_overlay(frame: &mut Frame, view: &LessonView, locale: Locale, area: Rect) {
    let title = format!(" {} ", view.topic);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let body = Text::from(lesson_lines(view, locale));
    let para = Paragraph::new(body)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((view.scroll, 0));
    frame.render_widget(Clear, area);
    frame.render_widget(para, area);
}

fn course_brief_tab(frame: &mut Frame, app: &App, area: Rect) {
    let p = CourseProgress::of(&app.deck);
    let counts = app.deck.counts();
    let subject = app
        .course
        .as_ref()
        .map(|c| c.subject.clone())
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {subject}  "),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(bar(p.pct()), Style::default().fg(ACCENT)),
            Span::raw(format!("  {}% · {}/{} 掌握", p.pct(), p.mastered, p.total)),
        ]),
        Line::raw(""),
    ];
    for s in LoopStage::ALL {
        let n = counts[s.index()];
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<8}", app.locale.stage(s)),
                Style::default()
                    .fg(stage_color(s))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<14}", app.locale.stage_saying(s)),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(format!("{n:>3} "), Style::default().fg(Color::White)),
            Span::styled("▍".repeat(n.min(20)), Style::default().fg(stage_color(s))),
        ]));
    }
    lines.push(Line::raw(""));
    let now = app.tutor.now();
    let plan = ReviewPlan::of(&app.deck, now);
    lines.push(Line::from(Span::styled(
        format!(" {} ({}) ", app.locale.review_plan(), plan.total()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )));
    if plan.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", app.locale.review_plan_empty()),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let buckets = [&plan.overdue, &plan.today, &plan.week, &plan.later];
        let colors = [Color::Red, Color::Yellow, Color::Cyan, Color::DarkGray];
        for (bi, b) in buckets.iter().enumerate() {
            if b.is_empty() {
                continue;
            }
            lines.push(Line::from(Span::styled(
                format!("  {} ({})", app.locale.review_bucket(bi), b.len()),
                Style::default().fg(colors[bi]).add_modifier(Modifier::BOLD),
            )));
            for e in b.iter().take(6) {
                lines.push(Line::from(vec![
                    Span::raw("    • "),
                    Span::styled(
                        truncate(&e.card.topic, 40),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!("  {}", date_of(e.card.next_review)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", app.locale.tab(2))),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Clock, DeckStore, SessionSource, Snippet, Summarizer};
    use crate::domain::{Activity, Lifecycle, Progress, StudyDeck};
    use anyhow::anyhow;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::time::Duration;

    struct FakeSource(Vec<ScannedSession>);
    impl SessionSource for FakeSource {
        fn collect(&self) -> Result<Vec<ScannedSession>> {
            Ok(self.0.clone())
        }
    }
    struct FixedClock(SystemTime);
    impl Clock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }
    struct NoBrain;
    impl Summarizer for NoBrain {
        fn run(&self, _p: &str) -> Result<String> {
            Err(anyhow!("no brain in tests"))
        }
    }
    struct MemStore;
    impl DeckStore for MemStore {
        fn load(&self) -> Result<StudyDeck> {
            Ok(StudyDeck::default())
        }
        fn save(&self, _d: &StudyDeck) -> Result<()> {
            Ok(())
        }
    }

    fn sample() -> Vec<ScannedSession> {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        vec![ScannedSession {
            terminal_id: Some("ttys000".into()),
            session_id: "s1".into(),
            cwd: "/home/u/tom/tutor".into(),
            title: "Build the tutor".into(),
            first_prompt: "help".into(),
            progress: Progress {
                done: 1,
                in_progress: 1,
                total: 3,
            },
            activity: Activity {
                messages: 12,
                tool_starts: 30,
            },
            lifecycle: Lifecycle::Active,
            created: now - Duration::from_secs(3600),
            last_activity: now - Duration::from_secs(60),
            snippets: vec![Snippet {
                text: "为什么 borrow checker 报错".into(),
                at: now - Duration::from_secs(120),
            }],
        }]
    }

    fn buffer_string(term: &Terminal<TestBackend>) -> String {
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn render_tab(tab: usize) -> String {
        let src = FakeSource(sample());
        let clock = FixedClock(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000));
        let store = MemStore;
        let brain = NoBrain;
        let tutor = Tutor::new(&src, &clock);
        let mut app = App::new(&tutor, &store, &brain, Locale::Zh, None).unwrap();
        app.tab = tab;
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| ui(f, &app)).unwrap();
        buffer_string(&term)
    }

    #[test]
    fn lesson_overlay_toggles_solution_visibility() {
        let md = "# 二分\n\n概述文字。\n\n## 题目 1\n找 x\n### 题解\n取中点\n";
        let view = LessonView::new("二分".into(), "二分".into(), Lesson::parse(md, "二分"));
        let flat = |lines: Vec<Line>| -> String {
            lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.to_string())
                .collect()
        };
        let shown = flat(lesson_lines(&view, Locale::Zh));
        assert!(shown.contains("取中点"));
        assert!(shown.contains("题解"));

        let mut hidden_view = view;
        hidden_view.show_solutions = false;
        let hidden = flat(lesson_lines(&hidden_view, Locale::Zh));
        assert!(!hidden.contains("取中点"));
        assert!(hidden.contains("题解已隐藏"));
    }
    #[test]
    fn board_tab_renders_columns_and_card() {
        let s = render_tab(1);
        let cjk: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(cjk.contains("私人教师"));
        assert!(cjk.contains("进行中"));
        assert!(s.contains("Build the tutor"));
        assert!(cjk.contains("在用")); // live tag
    }

    #[test]
    fn study_tab_renders_loop_and_due() {
        let s = render_tab(0);
        let cjk: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(cjk.contains("预习"));
        assert!(cjk.contains("改错"));
        // The heuristic-seeded Preview card is always due.
        assert!(s.contains("borrow checker"));
    }

    #[test]
    fn brief_tab_renders_sections() {
        let s = render_tab(2);
        let cjk: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(cjk.contains("快报"));
        assert!(cjk.contains("今日进行"));
    }

    #[test]
    fn course_mode_renders_kanban_from_roadmap() {
        // A course dir with a hand-written roadmap.
        let dir = std::env::temp_dir().join(format!("tutor-course-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("roadmap.md"),
            "# Algorithms — roadmap\n## Arrays\n- [ ] Two Sum\n- [x] Contains Duplicate\n",
        )
        .unwrap();

        let src = FakeSource(vec![]);
        let clock = FixedClock(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000));
        let store = MemStore;
        let brain = NoBrain;
        let tutor = Tutor::new(&src, &clock);
        let ctx = CourseCtx {
            dir: dir.clone(),
            subject: "Algorithms".into(),
        };
        let mut app = App::new(&tutor, &store, &brain, Locale::Zh, Some(ctx)).unwrap();

        // Deck seeded from the roadmap: one Preview (todo), one Correct (done).
        assert_eq!(app.deck.cards.len(), 2);
        app.tab = 1; // course kanban
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| ui(f, &app)).unwrap();
        let s = buffer_string(&term);
        let cjk: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(cjk.contains("待办")); // To do column
        assert!(cjk.contains("已完成")); // Done column
        assert!(s.contains("Two Sum"));
        assert!(cjk.contains("Algorithms") || s.contains("Algorithms")); // header subject

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn course_brief_tab_renders_review_plan() {
        let dir = std::env::temp_dir().join(format!("tutor-rp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("roadmap.md"),
            "# Algorithms — roadmap\n## Arrays\n- [ ] Two Sum\n",
        )
        .unwrap();

        let src = FakeSource(vec![]);
        let clock = FixedClock(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000));
        let store = MemStore;
        let brain = NoBrain;
        let tutor = Tutor::new(&src, &clock);
        let ctx = CourseCtx {
            dir: dir.clone(),
            subject: "Algorithms".into(),
        };
        let mut app = App::new(&tutor, &store, &brain, Locale::Zh, Some(ctx)).unwrap();
        app.tab = 2; // progress/brief tab

        // Before any topic reaches Review, the plan is empty.
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| ui(f, &app)).unwrap();
        let empty: String = buffer_string(&term)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(empty.contains("复习计划"));
        assert!(empty.contains("暂无复习项"));

        // Walk "Two Sum" into Review, then it appears on the dated plan.
        let id = crate::domain::study::normalize_id("Two Sum");
        for _ in 0..3 {
            app.deck.get_mut(&id).unwrap().promote(tutor.now());
        }
        assert_eq!(app.deck.get_mut(&id).unwrap().stage, LoopStage::Review);
        term.draw(|f| ui(f, &app)).unwrap();
        let s = buffer_string(&term);
        assert!(s.contains("Two Sum"));
        let cjk: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!cjk.contains("暂无复习项")); // now scheduled

        let _ = std::fs::remove_dir_all(&dir);
    }
}
