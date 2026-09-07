//! Delivery layer: user-facing rendering over the `Tutor` use case.
//! `cli` (one-shot) and `tui` (interactive) are interchangeable adapters; the
//! domain/app layers never know which is in use.

pub mod cli;
pub mod i18n;
pub mod tui;

pub use i18n::Locale;

/// Width of the unicode progress gauge, in cells.
pub(crate) const BAR_WIDTH: usize = 14;

/// A fixed-width unicode gauge, e.g. `██████░░░░`.
pub(crate) fn bar(pct: u8) -> String {
    let filled = ((pct as usize) * BAR_WIDTH + 50) / 100;
    let filled = filled.min(BAR_WIDTH);
    let mut s = String::with_capacity(BAR_WIDTH * 3);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..BAR_WIDTH {
        s.push('░');
    }
    s
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

/// Local calendar date `YYYY-MM-DD`. Prefers the system `date` (honors TZ),
/// falling back to a UTC computation so it never fails.
pub(crate) fn today_local() -> String {
    if let Ok(out) = std::process::Command::new("date").arg("+%Y-%m-%d").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.len() == 10 {
                return s;
            }
        }
    }
    // UTC fallback.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = now.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Inverse of `days_from_civil` (Howard Hinnant).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
