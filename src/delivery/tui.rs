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
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::i18n::Locale;
use super::{bar, today_local, truncate};
use crate::app::{DeckStore, ScannedSession, Summarizer, Tutor};
use crate::domain::study::{LoopStage, Unknown};
use crate::domain::{
    course, human_ago, Board, Column, CourseProgress, DailyBriefing, StudyDeck, WorkItem, WorkState,
};

const ACCENT: Color = Color::Green;

#[derive(Clone, Copy, PartialEq)]
enum Pending {
    None,
    Mine,
    Brief,
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
            Pending::None => {}
        }

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
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

    let status_line = Paragraph::new(Line::from(Span::styled(
        format!(" {}", app.status),
        Style::default().fg(Color::Yellow),
    )));
    frame.render_widget(status_line, status);

    let footer_text = if app.course.is_some() {
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
    for (i, bucket) in cols.iter().enumerate() {
        let focused = i == app.col;
        let items: Vec<ListItem> = bucket
            .iter()
            .map(|u| {
                let head = Line::from(vec![
                    Span::styled(
                        format!("[{}] ", app.locale.stage(u.stage)),
                        Style::default().fg(state_color(states[i])),
                    ),
                    Span::styled(
                        truncate(&u.topic, 26),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]);
                let sub = Line::from(Span::styled(
                    format!("  {}", truncate(&u.detail, 24)),
                    Style::default().fg(Color::DarkGray),
                ));
                ListItem::new(Text::from(vec![head, sub]))
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
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<8}", app.locale.stage(s)),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!("{:<14}", app.locale.stage_saying(s)),
                Style::default().fg(Color::Gray),
            ),
            Span::raw(format!("{}", counts[s.index()])),
        ]));
    }
    lines.push(Line::raw(""));
    let now = app.tutor.now();
    lines.push(Line::from(Span::styled(
        format!(" {} ", app.locale.study_due()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )));
    for u in app.deck.due(now).into_iter().take(8) {
        lines.push(Line::from(vec![
            Span::raw("  • "),
            Span::styled(
                format!("[{}] ", app.locale.stage(u.stage)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(truncate(&u.topic, 48), Style::default().fg(Color::White)),
        ]));
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
}
