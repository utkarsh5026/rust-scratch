// The scoreboard for this project — itself a Rust program (on-theme).
//
// Reads progress.json (appended to by the rust-practice skill whenever a ladder
// rung's check passes) and renders rank, XP, per-phase progress, achievements,
// and your practice streak.
//
// Run with: cargo run --bin stats

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
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

fn bar(done: u32, total: u32, width: u32) -> String {
    let filled = if total == 0 {
        0
    } else {
        (done * width) / total
    };
    let filled = filled.min(width);
    format!(
        "{}{}",
        "█".repeat(filled as usize),
        "░".repeat((width - filled) as usize)
    )
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
    println!();
    println!("  ╔══════════════════════════════════════════════╗");
    println!("  ║            🦀  RUST MASTERY  🦀               ║");
    println!("  ╚══════════════════════════════════════════════╝");
    println!();
    print!("   {emoji}  {name}   ·   {total_xp} XP");
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
            println!("   →  {ne} {nn} in {} XP", t - total_xp);
            println!("   {}  {into}/{span}", bar(into, span, 30));
        }
        None => println!("   →  MAX RANK 👑"),
    }
    println!();

    let total_done: u32 = completed.len() as u32;
    let total_ladders: u32 = PHASE_TOTALS.iter().map(|(_, _, n)| *n).sum::<u32>() + 6; // +capstones
    println!("   Ladders cleared: {total_done} / ~{total_ladders}");
    println!();
    println!("   Phase progress");
    for (p, label, total) in PHASE_TOTALS {
        let done = done_per_phase.get(p).copied().unwrap_or(0);
        let mark = if done >= *total { "✓" } else { " " };
        println!(
            "   {mark} P{p} {:<22} {}  {done}/{total}",
            label,
            bar(done, *total, 16)
        );
    }
    println!();

    println!("   Achievements");
    let badge = |n: usize, icon: &str, label: &str| {
        if n > 0 {
            println!("   {icon}  {label} ×{n}");
        }
    };
    badge(hint_free, "🏅", "Hint-Free ladder");
    badge(one_shot, "🎯", "One-Shot rung");
    badge(miri, "🧪", "Miri-Clean");
    badge(capstones, "🏛️ ", "Capstone");
    badge(phase_clears, "⭐", "Phase Clear");
    if best_streak >= 7 {
        println!("   🔥  Streak-7 unlocked");
    }
    if hint_free + one_shot + miri + capstones + phase_clears == 0 {
        println!("   (none yet — go solve a rung!)");
    }
    println!();

    if cur_streak > 0 {
        println!("   🔥  {cur_streak}-day streak   (best: {best_streak})");
    } else {
        println!("   🔥  no active streak — practice today to start one (best: {best_streak})");
    }
    println!();
}
