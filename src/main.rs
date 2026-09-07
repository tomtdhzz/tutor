//! Composition root: parse args, wire adapters into the `Tutor`, and dispatch to
//! a delivery adapter (work board, briefing, interactive tui, or a subject course).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;

use tutor::adapters::{CourseDir, FileDeckStore, OmpSessions, OmpSummarizer, SystemClock};
use tutor::app::Tutor;
use tutor::delivery::{cli, course as course_cli, tui, Locale};
use tutor::domain::course as course_domain;

const HELP: &str = "\
tutor — a local omp tutor (私人教师): study loop · work board · daily briefing · courses

USAGE:
    tutor [OPTIONS]                     One-shot: print the work board and exit
    tutor tui [OPTIONS]                 Interactive dashboard (学习/看板/快报)
    tutor briefing [OPTIONS]            Compose today's briefing (Markdown) and exit
    tutor course new <dir> --subject S  Draft a learning roadmap into <dir> and seed it
    tutor course <dir>                  Open the interactive course dashboard for <dir>
    tutor course board <dir>            Print the course kanban and exit

OPTIONS:
    --subject <text> Subject for `course new` (e.g. \"algorithms\")
    --lang <zh|en>   Display language (default: auto-detect from locale)
    --no-llm         Do not call `omp -p` (offline roadmap / rule-based briefing)
    -h, --help       Print this help
    -v, --version    Print version

Reads omp sessions under ~/.omp/agent (read-only). A course lives in its own folder:
<dir>/roadmap.md (editable) + <dir>/.tutor/deck.json (progress). The 'brain' shells out
to `omp -p` (study mining, briefing narration, roadmap drafting) via your logged-in accounts.
";

enum CourseAct {
    New,
    Open,
    Board,
}

enum Cmd {
    Board,
    Tui,
    Briefing,
    Course {
        action: CourseAct,
        dir: PathBuf,
        subject: Option<String>,
    },
}

struct Args {
    cmd: Cmd,
    locale: Locale,
    llm: bool,
}

fn main() -> ExitCode {
    match parse_args() {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(args)) => match run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("tutor: {e:#}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("tutor: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args() -> Result<Option<Args>> {
    let mut cmd = Cmd::Board;
    let mut locale = Locale::detect();
    let mut llm = true;
    let mut cmd_set = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-v" | "--version" => {
                println!("tutor {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--lang" => {
                let v = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--lang needs a value"))?;
                locale = Locale::from_flag(&v)
                    .ok_or_else(|| anyhow::anyhow!("invalid --lang '{v}' (use zh or en)"))?;
            }
            "--no-llm" => llm = false,
            "course" if !cmd_set => {
                cmd = parse_course(&mut it, &mut locale, &mut llm)?;
                cmd_set = true;
            }
            "tui" | "briefing" | "brief" | "board" if !cmd_set => {
                cmd = match arg.as_str() {
                    "tui" => Cmd::Tui,
                    "board" => Cmd::Board,
                    _ => Cmd::Briefing,
                };
                cmd_set = true;
            }
            other => anyhow::bail!("unexpected argument '{other}' (try --help)"),
        }
    }

    Ok(Some(Args { cmd, locale, llm }))
}

/// Parse the tail of `tutor course [new|board] <dir> [--subject S]`.
fn parse_course(
    it: &mut impl Iterator<Item = String>,
    locale: &mut Locale,
    llm: &mut bool,
) -> Result<Cmd> {
    let mut action = CourseAct::Open;
    let mut dir: Option<PathBuf> = None;
    let mut subject: Option<String> = None;
    let mut action_set = false;

    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "new" if !action_set && dir.is_none() => {
                action = CourseAct::New;
                action_set = true;
            }
            "board" if !action_set && dir.is_none() => {
                action = CourseAct::Board;
                action_set = true;
            }
            "--subject" => {
                subject = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--subject needs a value"))?,
                );
            }
            "--lang" => {
                let v = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--lang needs a value"))?;
                *locale = Locale::from_flag(&v)
                    .ok_or_else(|| anyhow::anyhow!("invalid --lang '{v}' (use zh or en)"))?;
            }
            "--no-llm" => *llm = false,
            other if !other.starts_with('-') && dir.is_none() => dir = Some(PathBuf::from(other)),
            other => anyhow::bail!("unexpected course argument '{other}' (try --help)"),
        }
    }

    let dir = dir.ok_or_else(|| {
        anyhow::anyhow!(
            "course needs a directory, e.g. `tutor course new ./algo --subject algorithms`"
        )
    })?;
    Ok(Cmd::Course {
        action,
        dir,
        subject,
    })
}

fn run(args: Args) -> Result<()> {
    let source = OmpSessions::new()?;
    let clock = SystemClock;
    let store = FileDeckStore::new()?;
    let summarizer = OmpSummarizer::new();
    let tutor = Tutor::new(&source, &clock);

    match args.cmd {
        Cmd::Board => cli::board(&tutor, args.locale),
        Cmd::Briefing => {
            let sum: Option<&dyn tutor::app::Summarizer> =
                if args.llm { Some(&summarizer) } else { None };
            cli::briefing(&tutor, &store, sum, args.locale)
        }
        Cmd::Tui => tui::run(&tutor, &store, &summarizer, args.locale),
        Cmd::Course {
            action,
            dir,
            subject,
        } => {
            let course = CourseDir::new(&dir);
            match action {
                CourseAct::New => {
                    if course.exists() {
                        anyhow::bail!(
                            "a course already exists at {} (edit roadmap.md, or open it with `tutor course`)",
                            dir.display()
                        );
                    }
                    let subject = subject.ok_or_else(|| {
                        anyhow::anyhow!("`course new` needs --subject \"<subject>\"")
                    })?;
                    let md = if args.llm {
                        match tutor.generate_roadmap(&summarizer, &subject) {
                            Ok(md) => md,
                            Err(e) => {
                                eprintln!(
                                    "tutor: roadmap drafting via omp -p failed ({e}); writing an offline starter to edit."
                                );
                                course_domain::starter_roadmap(&subject)
                            }
                        }
                    } else {
                        course_domain::starter_roadmap(&subject)
                    };
                    course_cli::init(&course, &subject, &md, &clock)
                }
                CourseAct::Board => course_cli::board(&course, &clock, args.locale),
                CourseAct::Open => {
                    if !course.exists() {
                        anyhow::bail!(
                            "no course at {} — create one: `tutor course new \"{}\" --subject \"<subject>\"`",
                            dir.display(),
                            dir.display()
                        );
                    }
                    let md = course.read_roadmap()?.unwrap_or_default();
                    let subject =
                        subject.unwrap_or_else(|| course_domain::Syllabus::parse(&md, "").subject);
                    let course_store = course.deck_store()?;
                    tui::run_course(
                        &tutor,
                        &course_store,
                        &summarizer,
                        args.locale,
                        dir,
                        subject,
                    )
                }
            }
        }
    }
}
