//! Composition root: parse args, wire adapters into the `Tutor`, and dispatch
//! to a delivery adapter (one-shot board, daily briefing, or interactive tui).

use std::process::ExitCode;

use anyhow::Result;

use tutor::adapters::{FileDeckStore, OmpSessions, OmpSummarizer, SystemClock};
use tutor::app::Tutor;
use tutor::delivery::{cli, tui, Locale};

const HELP: &str = "\
tutor — a local omp tutor (私人教师): work board · study loop · daily briefing

USAGE:
    tutor [OPTIONS]            One-shot: print the work board and exit
    tutor tui [OPTIONS]        Interactive dashboard (板/学习/快报)
    tutor briefing [OPTIONS]   Compose today's briefing (Markdown) and exit

OPTIONS:
    --lang <zh|en>   Display language (default: auto-detect from locale)
    --no-llm         Do not call `omp -p` (briefing prints rule-based facts only)
    -h, --help       Print this help
    -v, --version    Print version

Reads omp sessions under ~/.omp/agent (read-only). The 'brain' shells out to
`omp -p` for study mining and briefing narration, using your logged-in accounts.
";

enum Cmd {
    Board,
    Tui,
    Briefing,
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
    }
}
