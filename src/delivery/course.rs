//! One-shot course CLI: initialize a course folder from a roadmap, and render a
//! course kanban to stdout. The interactive course dashboard lives in `tui`.

use anyhow::Result;

use super::i18n::Locale;
use super::{bar, date_of};
use crate::adapters::{CourseDir, FileDeckStore};
use crate::app::{Clock, DeckStore, Tutor};
use crate::domain::{course, CourseProgress, LoopStage, ReviewPlan, StudyDeck, WorkState};

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

/// One manual stage action, mirroring the interactive TUI keys `.` `,` space.
#[derive(Clone, Copy)]
pub enum Act {
    Advance,
    Demote,
    Review,
}

/// Load the course deck, re-merge the (possibly hand-edited) roadmap + session
/// signals, and persist. Shared by `state` and the manual actions so every entry
/// point sees the same reconciled view.
fn load(tutor: &Tutor, course: &CourseDir) -> Result<(StudyDeck, String, FileDeckStore)> {
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
    Ok((deck, subject, store))
}

/// Print the deterministic course state (text or JSON) — the observable effect
/// view a verifier asserts against, no interactive TUI required.
pub fn state(tutor: &Tutor, course: &CourseDir, locale: Locale, json: bool) -> Result<()> {
    let (deck, subject, _store) = load(tutor, course)?;
    let now = tutor.now();
    if json {
        print!("{}", state_json(&deck, &subject, course, now));
    } else {
        print!("{}", state_text(&deck, &subject, locale, now));
    }
    Ok(())
}

/// Apply one manual stage action to the first card matching `topic`, persist, and
/// print `old → new`. Mutates only the course's own deck file.
pub fn act(
    tutor: &Tutor,
    course: &CourseDir,
    locale: Locale,
    topic: &str,
    action: Act,
) -> Result<()> {
    let (mut deck, _subject, store) = load(tutor, course)?;
    let id = select(&deck, topic).ok_or_else(|| {
        anyhow::anyhow!("no topic matching \"{topic}\" — see `tutor course state` for topics")
    })?;
    let now = tutor.now();
    let card = deck.get_mut(&id).expect("selected id exists");
    let title = card.topic.clone();
    let before = card.stage;
    match action {
        Act::Advance => card.promote(now),
        Act::Demote => card.demote(now),
        Act::Review => card.reschedule(now),
    }
    let after = card.stage;
    let reviews = card.reviews;
    let next = date_of(card.next_review);
    store.save(&deck)?;
    match action {
        Act::Review => {
            let label = match locale {
                Locale::Zh => "复习已记录",
                Locale::En => "review recorded",
            };
            let next_word = match locale {
                Locale::Zh => "下次",
                Locale::En => "next",
            };
            println!(
                "{title}: {label} (×{reviews}) → {next_word} {next}  [{}]",
                locale.stage(after)
            );
        }
        _ => println!(
            "{title}: {} → {}",
            locale.stage(before),
            locale.stage(after)
        ),
    }
    Ok(())
}

/// First card whose id or topic contains `needle` (case-insensitive), in deck order.
fn select(deck: &StudyDeck, needle: &str) -> Option<String> {
    let n = needle.to_lowercase();
    deck.cards
        .iter()
        .find(|c| c.id.contains(&n) || c.topic.to_lowercase().contains(&n))
        .map(|c| c.id.clone())
}

/// Stable lowercase stage key used in JSON and action output.
fn stage_key(s: LoopStage) -> &'static str {
    match s {
        LoopStage::Preview => "preview",
        LoopStage::Class => "class",
        LoopStage::Homework => "homework",
        LoopStage::Review => "review",
        LoopStage::Correct => "correct",
    }
}

/// The frozen JSON contract (see `.ai-work/spec-tui-and-algo.md`).
fn state_json(
    deck: &StudyDeck,
    subject: &str,
    course: &CourseDir,
    now: std::time::SystemTime,
) -> String {
    let p = CourseProgress::of(deck);
    let counts = deck.counts();
    let plan = ReviewPlan::of(deck, now);
    let entry = |e: &crate::domain::ReviewEntry| {
        serde_json::json!({
            "topic": e.card.topic,
            "stage": stage_key(e.card.stage),
            "next_review": date_of(e.card.next_review),
            "days_until": e.days_until,
        })
    };
    let bucket = |v: &[crate::domain::ReviewEntry]| v.iter().map(entry).collect::<Vec<_>>();
    let cards: Vec<_> = deck
        .cards
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "topic": c.topic,
                "section": c.project,
                "stage": stage_key(c.stage),
                "times_seen": c.times_seen,
                "reviews": c.reviews,
                "next_review": date_of(c.next_review),
                "due": c.is_due(now),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "subject": subject,
        "dir": course.dir().display().to_string(),
        "progress": {"mastered": p.mastered, "total": p.total, "pct": p.pct()},
        "stages": {
            "preview": counts[0], "class": counts[1], "homework": counts[2],
            "review": counts[3], "correct": counts[4],
        },
        "review_plan": {
            "overdue": bucket(&plan.overdue),
            "today": bucket(&plan.today),
            "week": bucket(&plan.week),
            "later": bucket(&plan.later),
        },
        "cards": cards,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    )
}

/// Human-readable state: progress bar, per-stage counts, and the dated review plan.
fn state_text(
    deck: &StudyDeck,
    subject: &str,
    locale: Locale,
    now: std::time::SystemTime,
) -> String {
    let p = CourseProgress::of(deck);
    let counts = deck.counts();
    let mut s = format!(
        "{subject}\n{}  {}%  ·  {}/{} 掌握\n\n",
        bar(p.pct()),
        p.pct(),
        p.mastered,
        p.total
    );
    for st in LoopStage::ALL {
        s.push_str(&format!(
            "  {:<8} {:<16} {}\n",
            locale.stage(st),
            locale.stage_saying(st),
            counts[st.index()]
        ));
    }
    s.push('\n');
    let plan = ReviewPlan::of(deck, now);
    s.push_str(&format!("{} ({})\n", locale.review_plan(), plan.total()));
    if plan.is_empty() {
        s.push_str(&format!("  {}\n", locale.review_plan_empty()));
    } else {
        let buckets = [&plan.overdue, &plan.today, &plan.week, &plan.later];
        for (i, b) in buckets.iter().enumerate() {
            if b.is_empty() {
                continue;
            }
            s.push_str(&format!("  {} ({})\n", locale.review_bucket(i), b.len()));
            for e in b.iter() {
                s.push_str(&format!(
                    "    · {}  ·  {}\n",
                    e.card.topic,
                    date_of(e.card.next_review)
                ));
            }
        }
    }
    s
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
