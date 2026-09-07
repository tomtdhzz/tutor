//! One-shot CLI rendering: print the board or the daily briefing and exit.

use anyhow::Result;

use super::i18n::Locale;
use super::{bar, today_local, truncate};
use crate::app::{DeckStore, Summarizer, Tutor};
use crate::domain::{human_ago, Board, Column, DailyBriefing, WorkItem};

/// Render the work board once, to stdout.
pub fn board(tutor: &Tutor, locale: Locale) -> Result<()> {
    let scans = tutor.scan()?;
    let board = tutor.board(&scans);
    print!("{}", render_board(&board, locale, tutor.now()));
    Ok(())
}

pub fn render_board(board: &Board, locale: Locale, now: std::time::SystemTime) -> String {
    if board.total() == 0 {
        return format!("{}\n", locale.empty(0));
    }
    let mut s = String::new();
    for col in board.columns() {
        s.push_str(&render_column(col, locale, now));
    }
    s
}

fn render_column(col: &Column, locale: Locale, now: std::time::SystemTime) -> String {
    let mut s = format!("\n{} ({})\n", locale.column(col.state), col.items.len());
    if col.items.is_empty() {
        s.push_str("  —\n");
        return s;
    }
    for it in &col.items {
        s.push_str(&format!("  {}\n", render_item(it, locale, now)));
    }
    s
}

fn render_item(it: &WorkItem, locale: Locale, now: std::time::SystemTime) -> String {
    let (tag, gauge) = match it.progress.pct() {
        Some(p) => (format!("{p:>3}%"), bar(p)),
        None => (" ~  ".to_string(), bar(it.activity_level())),
    };
    let live = if it.terminal_id.is_some() {
        format!(" ({})", locale.live_tag())
    } else {
        String::new()
    };
    let recency = locale.ago(&human_ago(it.last_activity, now));
    let detail = match it.progress.pct() {
        Some(_) => it.project().to_string(),
        None => format!("{} · {}", it.project(), locale.msgs(it.activity.messages)),
    };
    format!(
        "[{tag}] {gauge}  {}{live}  ·  {detail}  ·  {recency}",
        truncate(&it.proposition, 44),
    )
}

/// Compose the daily briefing (optionally narrated) and print it as Markdown.
pub fn briefing(
    tutor: &Tutor,
    store: &dyn DeckStore,
    summarizer: Option<&dyn Summarizer>,
    locale: Locale,
) -> Result<()> {
    let scans = tutor.scan()?;
    let board = tutor.board(&scans);
    let deck = tutor.seed_heuristic(&scans, store.load().unwrap_or_default());
    let _ = store.save(&deck);
    let mut brief = tutor.briefing(&board, &deck);
    let date = today_local();
    if let Some(sum) = summarizer {
        // Best-effort narration; structured facts still print if it fails.
        let _ = tutor.narrate(sum, &mut brief, &date);
    }
    print!("{}", briefing_markdown(&brief, locale, &date));
    Ok(())
}

pub fn briefing_markdown(brief: &DailyBriefing, locale: Locale, date: &str) -> String {
    let mut s = format!("# {} · {date}\n", locale.tab(2));
    if let Some(prose) = &brief.prose {
        s.push('\n');
        s.push_str(prose);
        s.push('\n');
    }
    let sections = [
        (0usize, &brief.active),
        (1, &brief.finished),
        (2, &brief.waiting),
        (3, &brief.to_study),
    ];
    for (which, lines) in sections {
        s.push_str(&format!("\n## {}\n", locale.brief_section(which)));
        if lines.is_empty() {
            s.push_str("- —\n");
            continue;
        }
        for l in lines {
            if l.detail.is_empty() {
                s.push_str(&format!("- {} · {}\n", l.proposition, l.project));
            } else {
                s.push_str(&format!(
                    "- {} · {} · {}\n",
                    l.proposition, l.project, l.detail
                ));
            }
        }
    }
    s
}
