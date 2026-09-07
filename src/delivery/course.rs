//! One-shot course CLI: initialize a course folder from a roadmap, and render a
//! course kanban to stdout. The interactive course dashboard lives in `tui`.

use anyhow::Result;

use super::bar;
use super::i18n::Locale;
use crate::adapters::CourseDir;
use crate::app::{Clock, DeckStore, Tutor};
use crate::domain::{course, CourseProgress, StudyDeck, WorkState};

/// Write the roadmap into `<dir>/roadmap.md` and seed `<dir>/.tutor/deck.json`.
pub fn init(course: &CourseDir, subject: &str, roadmap_md: &str, clock: &dyn Clock) -> Result<()> {
    course.write_roadmap(roadmap_md)?;
    let syllabus = course::Syllabus::parse(roadmap_md, subject);
    let store = course.deck_store()?;
    let mut deck = store.load().unwrap_or_default();
    let added = syllabus.merge_into(&mut deck, clock.now());
    store.save(&deck)?;

    let sections = section_count(&syllabus);
    println!("course ready in {}", course.dir().display());
    println!("  roadmap : {}", course.roadmap_path().display());
    println!("  topics  : {added} across {sections} section(s)");
    println!();
    println!("edit the roadmap, then open the board:");
    println!("  tutor course \"{}\"", course.dir().display());
    Ok(())
}

/// Load the course, re-merge any roadmap edits, and print the kanban.
pub fn board(tutor: &Tutor, course: &CourseDir, locale: Locale) -> Result<()> {
    if !course.exists() {
        anyhow::bail!(
            "no course in {} — create one with: tutor course new \"{}\" --subject \"<subject>\"",
            course.dir().display(),
            course.dir().display()
        );
    }
    let store = course.deck_store()?;
    let base = store.load().unwrap_or_default();
    let md = course.read_roadmap()?.unwrap_or_default();
    let subject = course::Syllabus::parse(&md, "").subject;
    let signals = super::dir_signals(course.dir());
    let deck = tutor.reconcile_course(course.dir(), &subject, &md, base, &signals);
    store.save(&deck)?;
    print!("{}", render(&deck, locale));
    Ok(())
}

fn render(deck: &StudyDeck, locale: Locale) -> String {
    let p = CourseProgress::of(deck);
    let mut s = format!(
        "{}  {}%  ·  {}/{}\n",
        bar(p.pct()),
        p.pct(),
        p.mastered,
        p.total
    );
    let cols = course::columns(deck);
    let states = [WorkState::Todo, WorkState::Doing, WorkState::Done];
    for (i, bucket) in cols.iter().enumerate() {
        s.push_str(&format!(
            "\n{} ({})\n",
            locale.column(states[i]),
            bucket.len()
        ));
        if bucket.is_empty() {
            s.push_str("  —\n");
        }
        for u in bucket {
            s.push_str(&format!(
                "  [{}] {}  ·  {}\n",
                locale.stage(u.stage),
                u.topic,
                u.detail
            ));
        }
    }
    s
}

fn section_count(s: &course::Syllabus) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for t in &s.topics {
        if !seen.contains(&t.section.as_str()) {
            seen.push(&t.section);
        }
    }
    seen.len()
}
