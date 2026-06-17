// The scoreboard for this project — itself a Rust program (on-theme).
//
// Reads progress.json (appended to by the rust-practice skill whenever a ladder
// rung's check passes) and renders rank, XP, per-phase progress, achievements,
// and your practice streak.
//
// Run with: cargo run --bin stats

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct Progress {
    #[serde(default)]
    events: Vec<Event>,
}

#[derive(Deserialize, Clone)]
struct Event {
    date: String, // YYYY-MM-DD
    phase: u8,
    concept: String,
    #[allow(dead_code)]
    rung: u32,
    tier: String,
    #[serde(default)]
    hints: u32,
    #[serde(default)]
    first_try: bool,
    #[serde(default)]
    miri_clean: bool,
}

// ---- XP model -------------------------------------------------------------
fn base_xp(tier: &str) -> u32 {
    match tier {
        "foundations" => 10,
        "mechanics" => 15,
        "footgun" => 20,
        "real-world" | "real_world" => 25,
        "capstone" => 50,
        _ => 10,
    }
}

fn event_xp(e: &Event) -> u32 {
    base_xp(&e.tier)
        + if e.hints == 0 { 10 } else { 0 } // no-hint solve
        + if e.first_try { 5 } else { 0 } // first-try compile
        + if e.miri_clean { 25 } else { 0 } // sound unsafe
}

// ---- ranks (XP thresholds, flavored by roadmap phase) ---------------------
const RANKS: &[(u32, &str, &str)] = &[
    (0, "🥚", "Fledgling"),
    (100, "🦀", "Crab"),
    (400, "⚙️", "Trait Smith"),
    (800, "📐", "API Architect"),
    (1400, "🧵", "Fearless Concurrent"),
    (2200, "⏳", "Async Adept"),
    (3200, "☢️", "Unsafe Operator"),
    (4400, "🚀", "Zero-Cost Wizard"),
    (5800, "🧙", "Macro Sorcerer"),
    (7500, "👑", "Rustacean Master"),
];

// ladders per phase, from ROADMAP.md (for progress bars)
const PHASE_TOTALS: &[(u8, &str, u32)] = &[
    (0, "Tooling & testing", 8),
    (1, "Ownership & types", 13),
    (2, "Traits & generics", 13),
    (3, "API & error design", 9),
    (4, "Concurrency", 8),
    (5, "Async internals", 11),
    (6, "Unsafe & the machine", 10),
    (7, "Performance", 8),
    (8, "Metaprogramming", 5),
    (9, "Specialization", 8),
];

// ---- terminal styling -----------------------------------------------------
type Rgb = (u8, u8, u8);

// One Dark-ish palette: easy on the eyes, good contrast on dark terminals.
const RUST: Rgb = (222, 113, 47); // signature rust orange
const GOLD: Rgb = (229, 192, 123); // XP / highlights
const GREEN: Rgb = (152, 195, 121); // completed
const RED: Rgb = (224, 108, 117); // streak fire
const BLUE: Rgb = (97, 175, 239);
const PURPLE: Rgb = (198, 120, 221);
const CYAN: Rgb = (86, 182, 194);
const FG: Rgb = (220, 223, 228); // primary text
const DIMC: Rgb = (92, 99, 112); // muted / empty cells

const PANEL_WIDTH: usize = 50;

// Color is on only for a real TTY and when NO_COLOR is unset (so piped output
// stays clean ASCII). Cached so we resolve the environment exactly once.
fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn styled(s: &str, (r, g, b): Rgb, bold: bool) -> String {
    if !color_enabled() {
        return s.to_string();
    }
    let weight = if bold { "1;" } else { "" };
    format!("\x1b[{weight}38;2;{r};{g};{b}m{s}\x1b[0m")
}

fn lerp((ar, ag, ab): Rgb, (br, bg, bb): Rgb, t: f32) -> Rgb {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    (mix(ar, br), mix(ag, bg), mix(ab, bb))
}

fn filled_cells(done: u32, total: u32, width: u32) -> u32 {
    if total == 0 {
        0
    } else {
        ((done * width) / total).min(width)
    }
}

// Solid-color bar: filled cells in `color`, the rest dimmed.
fn solid_bar(done: u32, total: u32, width: u32, color: Rgb) -> String {
    let filled = filled_cells(done, total, width);
    format!(
        "{}{}",
        styled(&"█".repeat(filled as usize), color, false),
        styled(&"░".repeat((width - filled) as usize), DIMC, false),
    )
}

// Gradient bar: filled cells fade `start` → `end` across their span.
fn grad_bar(done: u32, total: u32, width: u32, start: Rgb, end: Rgb) -> String {
    let filled = filled_cells(done, total, width);
    let mut out = String::new();
    for i in 0..width {
        if i < filled {
            let t = if filled <= 1 {
                0.0
            } else {
                i as f32 / (filled - 1) as f32
            };
            out.push_str(&styled("█", lerp(start, end, t), false));
        } else {
            out.push_str(&styled("░", DIMC, false));
        }
    }
    out
}

fn rule(width: usize, heavy: bool, color: Rgb) -> String {
    let ch = if heavy { "━" } else { "─" };
    styled(&ch.repeat(width), color, false)
}

// ---- date helpers (no chrono) --------------------------------------------
// Howard Hinnant's days_from_civil: Y-M-D -> days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn parse_day(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    Some(days_from_civil(y, m, d))
}

fn today_day() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs / 86_400) as i64
}

// current streak (consecutive days ending today or yesterday) + longest run
fn streaks(days: &BTreeSet<i64>) -> (u32, u32) {
    if days.is_empty() {
        return (0, 0);
    }
    let mut longest = 1;
    let mut run = 1;
    let mut prev: Option<i64> = None;
    for &d in days {
        if let Some(p) = prev {
            if d == p + 1 {
                run += 1;
            } else {
                run = 1;
            }
        }
        longest = longest.max(run);
        prev = Some(d);
    }
    let today = today_day();
    let mut current = 0;
    let mut cursor = if days.contains(&today) {
        today
    } else if days.contains(&(today - 1)) {
        today - 1
    } else {
        return (0, longest);
    };
    while days.contains(&cursor) {
        current += 1;
        cursor -= 1;
    }
    (current, longest)
}

fn main() {
    let raw = std::fs::read_to_string("progress.json").unwrap_or_else(|_| {
        eprintln!("(no progress.json yet — solve a ladder rung to start earning XP)");
        String::from("{\"events\":[]}")
    });
    let prog: Progress = serde_json::from_str(&raw).expect("progress.json is malformed JSON");
    let events = prog.events;

    let total_xp: u32 = events.iter().map(event_xp).sum();

    // rank
    let (emoji, name) = RANKS
        .iter()
        .rev()
        .find(|(t, _, _)| total_xp >= *t)
        .map(|(_, e, n)| (*e, *n))
        .unwrap_or(("🥚", "Fledgling"));
    let next = RANKS.iter().find(|(t, _, _)| total_xp < *t);

    // completed concepts = those with a capstone rung
    let completed: BTreeSet<&str> = events
        .iter()
        .filter(|e| e.tier == "capstone")
        .map(|e| e.concept.as_str())
        .collect();
    let phase_of: BTreeMap<&str, u8> = events
        .iter()
        .map(|e| (e.concept.as_str(), e.phase))
        .collect();
    let mut done_per_phase: BTreeMap<u8, u32> = BTreeMap::new();
    for c in &completed {
        if let Some(p) = phase_of.get(c) {
            *done_per_phase.entry(*p).or_insert(0) += 1;
        }
    }

    // achievements
    let one_shot = events.iter().filter(|e| e.first_try).count();
    let miri = events.iter().filter(|e| e.miri_clean).count();
    let capstones = completed.len();
    let hint_free = completed
        .iter()
        .filter(|c| {
            events
                .iter()
                .filter(|e| &e.concept == *c)
                .all(|e| e.hints == 0)
        })
        .count();
    let phase_clears = PHASE_TOTALS
        .iter()
        .filter(|(p, _, total)| done_per_phase.get(p).copied().unwrap_or(0) >= *total)
        .count();

    // streaks
    let days: BTreeSet<i64> = events.iter().filter_map(|e| parse_day(&e.date)).collect();
    let (cur_streak, best_streak) = streaks(&days);

    // ---- render ----------------------------------------------------------
    // Layout is a stack of sections separated by horizontal rules — no closed
    // box, so emoji double-width never breaks right-edge alignment.
    println!();
    println!(
        "  {}  {}",
        styled("🦀", RUST, true),
        styled("RUST  MASTERY", FG, true)
    );
    println!("  {}", rule(PANEL_WIDTH, true, RUST));
    println!();

    // Rank headline: emoji · bold name · gold XP.
    println!(
        "  {emoji}  {}        {}",
        styled(name, RUST, true),
        styled(&format!("{total_xp} XP"), GOLD, true)
    );
    match next {
        Some((t, ne, nn)) => {
            let span_lo = RANKS
                .iter()
                .rev()
                .find(|(x, _, _)| total_xp >= *x)
                .map(|(x, _, _)| *x)
                .unwrap_or(0);
            let into = total_xp - span_lo;
            let span = t - span_lo;
            let pct = if span == 0 { 100 } else { into * 100 / span };
            println!(
                "  {}",
                styled(
                    &format!("next  {ne} {nn}  ·  {} XP to go", t - total_xp),
                    DIMC,
                    false
                )
            );
            println!(
                "  {}  {}",
                grad_bar(into, span, 28, RUST, GOLD),
                styled(&format!("{into}/{span}  ·  {pct}%"), FG, false)
            );
        }
        None => println!("  {}", styled("👑  MAX RANK", GOLD, true)),
    }
    println!();
    println!("  {}", rule(PANEL_WIDTH, false, DIMC));
    println!();

    // Phases.
    let total_done: u32 = completed.len() as u32;
    let total_ladders: u32 = PHASE_TOTALS.iter().map(|(_, _, n)| *n).sum::<u32>() + 6; // +capstones
    println!(
        "  {}{}{}",
        styled("PHASES", FG, true),
        " ".repeat(PANEL_WIDTH.saturating_sub(6 + 17)),
        styled(
            &format!("{total_done} / {total_ladders} ladders cleared"),
            DIMC,
            false
        )
    );
    println!();
    for (p, label, total) in PHASE_TOTALS {
        let done = done_per_phase.get(p).copied().unwrap_or(0);
        let complete = done >= *total;
        let started = done > 0;
        let (mark, bar_color, label_color, count_color) = if complete {
            (styled("✓", GREEN, true), GREEN, FG, GREEN)
        } else if started {
            (styled("▸", RUST, true), RUST, FG, GOLD)
        } else {
            (styled("·", DIMC, false), DIMC, DIMC, DIMC)
        };
        println!(
            "  {mark} {}  {}  {}  {}",
            styled(&format!("P{p}"), DIMC, false),
            styled(&format!("{label:<22}"), label_color, false),
            solid_bar(done, *total, 16, bar_color),
            styled(&format!("{done}/{total}"), count_color, false)
        );
    }
    println!();
    println!("  {}", rule(PANEL_WIDTH, false, DIMC));
    println!();

    // Achievements — each present badge becomes a styled chip, two per row.
    println!("  {}", styled("ACHIEVEMENTS", FG, true));
    println!();
    let mut chips: Vec<String> = Vec::new();
    let mut chip = |n: usize, icon: &str, label: &str, color: Rgb| {
        if n > 0 {
            chips.push(format!(
                "{icon}  {} {}",
                styled(label, FG, false),
                styled(&format!("×{n}"), color, true)
            ));
        }
    };
    chip(hint_free, "🏅", "Hint-Free", GOLD);
    chip(one_shot, "🎯", "One-Shot", BLUE);
    chip(miri, "🧪", "Miri-Clean", CYAN);
    chip(capstones, "🏛️", "Capstone", PURPLE);
    chip(phase_clears, "⭐", "Phase Clear", GOLD);
    if best_streak >= 7 {
        chips.push(format!("🔥  {}", styled("Streak-7", RED, true)));
    }
    if chips.is_empty() {
        println!("  {}", styled("(none yet — go solve a rung!)", DIMC, false));
    } else {
        for row in chips.chunks(2) {
            println!("  {}", row.join("       "));
        }
    }
    println!();

    // Streak footer.
    if cur_streak > 0 {
        println!(
            "  {} {}   {}",
            styled("🔥", RED, false),
            styled(&format!("{cur_streak}-day streak"), RED, true),
            styled(&format!("·  best {best_streak}"), DIMC, false)
        );
    } else {
        println!(
            "  {} {}",
            styled("🔥", DIMC, false),
            styled(
                &format!("no active streak — practice today to start one  ·  best {best_streak}"),
                DIMC,
                false
            )
        );
    }
    println!();
}
