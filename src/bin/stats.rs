// The scoreboard for this project — itself a Rust program (on-theme).
//
// Reads progress.json (appended to by the rust-practice skill whenever a ladder
// rung's check passes) and renders rank, XP, per-phase progress, achievements,
// and your practice streak.
//
// Two backends, one layout (built as a list of colored spans):
//   cargo run --bin stats                       -> ANSI dashboard in the terminal
//   cargo run --bin stats -- --svg <path.svg>   -> write a colored SVG "screenshot"
//
// The SVG path powers the auto-updating badge in README.md (see
// .github/workflows/stats.yml). FORCE_COLOR=1 / CLICOLOR_FORCE=1 force ANSI
// color even when stdout isn't a TTY; NO_COLOR disables it.

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
// Thresholds are scaled to the FULL 93-ladder roadmap (~32k XP at completion),
// and themed so the rank roughly tracks which phase you're conquering. A clean
// 9-rung ladder is ~325 XP, so the top rank means you genuinely finished — not
// that you ground bonuses on a third of the project.
const RANKS: &[(u32, &str, &str)] = &[
    (0, "🥚", "Fledgling"),
    (300, "🦀", "Crab"),
    (1500, "⚙️", "Trait Smith"),
    (3500, "📐", "API Architect"),
    (7000, "🧵", "Fearless Concurrent"),
    (12000, "⏳", "Async Adept"),
    (17000, "☢️", "Unsafe Operator"),
    (22000, "🚀", "Zero-Cost Wizard"),
    (27000, "🧙", "Macro Sorcerer"),
    (32000, "👑", "Rustacean Master"),
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

// ---- palette --------------------------------------------------------------
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
const BG: Rgb = (24, 26, 31); // svg background
const BG2: Rgb = (33, 37, 43); // svg window header

const PANEL_WIDTH: usize = 50;

// ---- styled spans (backend-agnostic) --------------------------------------
// The dashboard is built as a Vec<Line>, Line = Vec<Span>. Both the ANSI and
// the SVG renderer walk the same spans, so the two outputs can never drift.
#[derive(Clone)]
struct Span {
    text: String,
    color: Rgb,
    bold: bool,
}

type Line = Vec<Span>;

fn sp(text: impl Into<String>, color: Rgb, bold: bool) -> Span {
    Span {
        text: text.into(),
        color,
        bold,
    }
}

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if std::env::var_os("CLICOLOR_FORCE").is_some() || std::env::var_os("FORCE_COLOR").is_some()
        {
            return true;
        }
        std::io::stdout().is_terminal()
    })
}

fn ansi(s: &str, (r, g, b): Rgb, bold: bool) -> String {
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

// Solid bar: filled cells in `color`, the rest dimmed — as spans.
fn solid_bar(done: u32, total: u32, width: u32, color: Rgb) -> Vec<Span> {
    let filled = filled_cells(done, total, width);
    let mut v = Vec::new();
    if filled > 0 {
        v.push(sp("█".repeat(filled as usize), color, false));
    }
    if width > filled {
        v.push(sp("░".repeat((width - filled) as usize), DIMC, false));
    }
    v
}

// Gradient bar: filled cells fade `start` -> `end`; one span per cell.
fn grad_bar(done: u32, total: u32, width: u32, start: Rgb, end: Rgb) -> Vec<Span> {
    let filled = filled_cells(done, total, width);
    let mut v = Vec::new();
    for i in 0..width {
        if i < filled {
            let t = if filled <= 1 {
                0.0
            } else {
                i as f32 / (filled - 1) as f32
            };
            v.push(sp("█", lerp(start, end, t), false));
        } else {
            v.push(sp("░", DIMC, false));
        }
    }
    v
}

fn rule(width: usize, heavy: bool, color: Rgb) -> Span {
    let ch = if heavy { "━" } else { "─" };
    sp(ch.repeat(width), color, false)
}

fn heading(label: &str) -> Span {
    sp(label, CYAN, true)
}

// 11655 -> "11,655" so big XP totals stay readable.
fn commafy(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
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

// ---- backends -------------------------------------------------------------
fn print_ansi(lines: &[Line]) {
    let mut out = String::new();
    for line in lines {
        for s in line {
            out.push_str(&ansi(&s.text, s.color, s.bold));
        }
        out.push('\n');
    }
    print!("{out}");
}

// Rough terminal-cell width of a char (for sizing the SVG canvas only).
fn char_cols(c: char) -> usize {
    let u = c as u32;
    if u == 0xFE0F {
        0 // variation selector renders no cell of its own
    } else if u >= 0x1F000 || matches!(u, 0x2699 | 0x2622 | 0x2B50 | 0x23F3 | 0x2728 | 0x1F004) {
        2 // emoji presentation
    } else {
        1
    }
}

fn line_cols(line: &Line) -> usize {
    line.iter()
        .flat_map(|s| s.text.chars())
        .map(char_cols)
        .sum()
}

fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// Render the same spans to a self-contained, colored SVG "screenshot".
fn render_svg(lines: &[Line]) -> String {
    let char_w = 9.4_f32;
    let line_h = 21.0_f32;
    let pad_x = 22.0_f32;
    let pad_y = 18.0_f32;
    let chrome = 34.0_f32; // window header strip

    let cols = lines.iter().map(line_cols).max().unwrap_or(40).max(46);
    let width = (cols as f32) * char_w + pad_x * 2.0;
    let height = chrome + pad_y + (lines.len() as f32) * line_h + pad_y;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" \
         viewBox=\"0 0 {w:.0} {h:.0}\" font-family=\"'JetBrains Mono','Fira Code','Cascadia Code',ui-monospace,'SFMono-Regular',Menlo,Consolas,monospace\" font-size=\"14\">\n",
        w = width,
        h = height
    ));
    // window background + header strip + traffic-light dots
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{w:.0}\" height=\"{h:.0}\" rx=\"10\" fill=\"{bg}\"/>\n",
        w = width,
        h = height,
        bg = hex(BG)
    ));
    svg.push_str(&format!(
        "<path d=\"M0,10 a10,10 0 0 1 10,-10 H{x:.0} a10,10 0 0 1 10,10 V{c:.0} H0 Z\" fill=\"{bg2}\"/>\n",
        x = width - 10.0,
        c = chrome,
        bg2 = hex(BG2)
    ));
    for (i, dot) in [
        (0xe0_u8, 0x6c, 0x75),
        (0xe5, 0xc0, 0x7b),
        (0x98, 0xc3, 0x79),
    ]
    .iter()
    .enumerate()
    {
        svg.push_str(&format!(
            "<circle cx=\"{cx:.0}\" cy=\"17\" r=\"6\" fill=\"{c}\"/>\n",
            cx = 20.0 + i as f32 * 20.0,
            c = hex((dot.0, dot.1, dot.2))
        ));
    }
    svg.push_str(&format!(
        "<text x=\"{x:.0}\" y=\"21\" fill=\"{c}\" font-size=\"12\" text-anchor=\"middle\">cargo run --bin stats</text>\n",
        x = width / 2.0,
        c = hex(DIMC)
    ));

    // text lines
    let mut y = chrome + pad_y + line_h * 0.7;
    for line in lines {
        if !line.is_empty() {
            svg.push_str(&format!(
                "<text x=\"{x:.0}\" y=\"{y:.1}\" xml:space=\"preserve\">",
                x = pad_x,
                y = y
            ));
            for s in line {
                let weight = if s.bold { " font-weight=\"bold\"" } else { "" };
                svg.push_str(&format!(
                    "<tspan fill=\"{c}\"{weight}>{t}</tspan>",
                    c = hex(s.color),
                    t = xml_escape(&s.text)
                ));
            }
            svg.push_str("</text>\n");
        }
        y += line_h;
    }
    svg.push_str("</svg>\n");
    svg
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

    // overall roadmap completion (capstoned ladders / total ladders across phases)
    let total_done = completed.len() as u32;
    let total_ladders: u32 = PHASE_TOTALS.iter().map(|(_, _, n)| *n).sum();
    let overall_pct = if total_ladders == 0 {
        0
    } else {
        total_done * 100 / total_ladders
    };
    let rungs_solved = events.len();

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

    // ---- build the layout as a list of colored spans ---------------------
    let indent = || sp("  ", DIMC, false);
    let mut lines: Vec<Line> = Vec::new();

    // Title row: brand on the left, XP total on the right edge of the panel.
    let title = "RUST MASTERY";
    let xp_label = format!("{} XP", commafy(total_xp));
    let title_pad = PANEL_WIDTH.saturating_sub(4 + title.len() + xp_label.len());
    lines.push(vec![
        indent(),
        sp("🦀", RUST, true),
        sp("  ", FG, false),
        sp(title, FG, true),
        sp(" ".repeat(title_pad), FG, false),
        sp(&xp_label, GOLD, true),
    ]);
    lines.push(vec![indent(), rule(PANEL_WIDTH, true, RUST)]);
    lines.push(Vec::new());

    // RANK
    lines.push(vec![
        indent(),
        sp(emoji, RUST, true),
        sp("  ", FG, false),
        sp(name, FG, true),
    ]);
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
            let mut bar = vec![indent()];
            bar.extend(grad_bar(into, span, 22, RUST, GOLD));
            bar.push(sp("  ", FG, false));
            bar.push(sp(format!("{pct}%"), FG, true));
            lines.push(bar);
            lines.push(vec![
                indent(),
                sp(
                    format!("next  {ne} {nn}  ·  {} XP to go", commafy(t - total_xp)),
                    DIMC,
                    false,
                ),
            ]);
        }
        None => {
            let mut bar = vec![indent()];
            bar.extend(grad_bar(1, 1, 22, RUST, GOLD));
            bar.push(sp("  ", FG, false));
            bar.push(sp(
                format!("MAX RANK  ·  {overall_pct}% of roadmap done"),
                GOLD,
                true,
            ));
            lines.push(bar);
        }
    }
    lines.push(Vec::new());

    // ROADMAP
    let right = format!("{total_done}/{total_ladders} ladders  ·  {overall_pct}%");
    let pad = PANEL_WIDTH.saturating_sub("ROADMAP".len() + right.len());
    lines.push(vec![
        indent(),
        heading("ROADMAP"),
        sp(" ".repeat(pad), DIMC, false),
        sp(&right, DIMC, false),
    ]);
    let mut bar = vec![indent()];
    bar.extend(grad_bar(
        total_done,
        total_ladders,
        PANEL_WIDTH as u32,
        RUST,
        GOLD,
    ));
    lines.push(bar);
    lines.push(vec![
        indent(),
        sp(format!("{rungs_solved} rungs solved"), DIMC, false),
    ]);
    lines.push(Vec::new());

    // PHASES — columns: mark · Pn · label(20) · bar(16) · right-aligned count.
    lines.push(vec![indent(), heading("PHASES")]);
    for (p, label, total) in PHASE_TOTALS {
        let done = done_per_phase.get(p).copied().unwrap_or(0);
        let complete = done >= *total;
        let started = done > 0;
        let (mark, mark_c, bar_color, label_color, count_color) = if complete {
            ("✓", GREEN, GREEN, FG, GREEN)
        } else if started {
            ("▸", RUST, RUST, FG, GOLD)
        } else {
            ("·", DIMC, DIMC, DIMC, DIMC)
        };
        let count = format!("{:>5}", format!("{done}/{total}"));
        let mut row = vec![
            indent(),
            sp(mark, mark_c, true),
            sp(format!(" P{p} "), DIMC, false),
            sp(format!("{label:<20}"), label_color, false),
            sp("  ", FG, false),
        ];
        row.extend(solid_bar(done, *total, 16, bar_color));
        row.push(sp("  ", FG, false));
        row.push(sp(count, count_color, false));
        lines.push(row);
    }
    lines.push(Vec::new());

    // ACHIEVEMENTS
    lines.push(vec![indent(), heading("ACHIEVEMENTS")]);
    let mut chips: Vec<Vec<Span>> = Vec::new();
    let mut chip = |n: usize, icon: &str, label: &str, color: Rgb| {
        if n > 0 {
            chips.push(vec![
                sp(format!("{icon}  "), FG, false),
                sp(label, FG, false),
                sp(" ", FG, false),
                sp(format!("×{n}"), color, true),
            ]);
        }
    };
    chip(hint_free, "🏅", "Hint-Free", GOLD);
    chip(one_shot, "🎯", "One-Shot", BLUE);
    chip(miri, "🧪", "Miri-Clean", CYAN);
    chip(capstones, "🏛️", "Capstone", PURPLE);
    chip(phase_clears, "⭐", "Phase Clear", GOLD);
    if best_streak >= 7 {
        chips.push(vec![sp("🔥  ", RED, false), sp("Streak-7", RED, true)]);
    }
    if chips.is_empty() {
        lines.push(vec![
            indent(),
            sp("(none yet — go solve a rung!)", DIMC, false),
        ]);
    } else {
        for pair in chips.chunks(2) {
            let mut row = vec![indent()];
            for (i, c) in pair.iter().enumerate() {
                if i > 0 {
                    row.push(sp("        ", FG, false));
                }
                row.extend(c.clone());
            }
            lines.push(row);
        }
    }
    lines.push(Vec::new());

    // STREAK footer
    lines.push(vec![indent(), rule(PANEL_WIDTH, false, DIMC)]);
    if cur_streak > 0 {
        lines.push(vec![
            indent(),
            sp("🔥", RED, false),
            sp("  ", FG, false),
            sp(format!("{cur_streak}-day streak"), RED, true),
            sp(format!("  ·  best {best_streak}"), DIMC, false),
        ]);
    } else {
        lines.push(vec![
            indent(),
            sp("🔥", DIMC, false),
            sp("  ", FG, false),
            sp(
                format!("no active streak — practice today  ·  best {best_streak}"),
                DIMC,
                false,
            ),
        ]);
    }

    // ---- dispatch to a backend -------------------------------------------
    let args: Vec<String> = std::env::args().collect();
    let svg_out = args
        .iter()
        .position(|a| a == "--svg")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| std::env::var("STATS_SVG_OUT").ok());

    match svg_out {
        Some(path) => {
            let svg = render_svg(&lines);
            if let Some(dir) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::write(&path, svg).expect("failed to write SVG");
            eprintln!("wrote dashboard SVG -> {path}");
        }
        None => {
            println!();
            print_ansi(&lines);
            println!();
        }
    }
}
