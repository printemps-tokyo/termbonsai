//! Drawing the tree: which parts show at its growth, health and season, in
//! what colors, and the words around it. Pure: everything returns a String.

use std::fmt::Write as _;

use crate::state::{Health, State, DAY};
use crate::tree::{Kind, Part};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ja,
    En,
}

impl Lang {
    /// Japanese when the first set of LC_ALL, LC_MESSAGES and LANG starts with "ja".
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Lang {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| get(k))
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        if locale.starts_with("ja") {
            Lang::Ja
        } else {
            Lang::En
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    /// The season of a month (1-12): March to May is spring, and so on; in
    /// the southern hemisphere, the opposite.
    pub fn of_month(month: u32, south: bool) -> Season {
        let m = if south { (month + 5) % 12 + 1 } else { month };
        match m {
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            9..=11 => Season::Autumn,
            _ => Season::Winter,
        }
    }

    pub fn parse(s: &str) -> Option<Season> {
        match s {
            "spring" => Some(Season::Spring),
            "summer" => Some(Season::Summer),
            "autumn" | "fall" => Some(Season::Autumn),
            "winter" => Some(Season::Winter),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// How colors are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
    None,
}

/// The nearest color in the xterm 256-color palette: the closest cube color
/// or the closest gray of the ramp 232-255, whichever is nearer.
pub fn ansi256(c: Rgb) -> u8 {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let Rgb(r, g, b) = c;
    let nearest = |v: u8| -> usize {
        (0..6)
            .min_by_key(|&i| (CUBE[i] as i32 - v as i32).abs())
            .unwrap()
    };
    let (ri, gi, bi) = (nearest(r), nearest(g), nearest(b));
    let cube = Rgb(CUBE[ri], CUBE[gi], CUBE[bi]);
    let avg = (r as i32 + g as i32 + b as i32) / 3;
    let ramp = ((avg - 8 + 5) / 10).clamp(0, 23);
    let gray = (8 + 10 * ramp) as u8;
    let dist = |p: Rgb| -> i32 {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(p.0, r) + d(p.1, g) + d(p.2, b)
    };
    if dist(Rgb(gray, gray, gray)) < dist(cube) {
        232 + ramp as u8
    } else {
        16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8
    }
}

fn paint(
    out: &mut String,
    ch: char,
    color: Option<Rgb>,
    mode: ColorMode,
    current: &mut Option<Rgb>,
) {
    if mode != ColorMode::None && color != *current {
        match (color, mode) {
            (None, _) => out.push_str("\x1b[0m"),
            (Some(Rgb(r, g, b)), ColorMode::TrueColor) => {
                let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
            }
            (Some(c), _) => {
                let _ = write!(out, "\x1b[38;5;{}m", ansi256(c));
            }
        }
        *current = color;
    }
    out.push(ch);
}

const TRUNK: Rgb = Rgb(0x8b, 0x5a, 0x2b);
const BRANCH: Rgb = Rgb(0xa0, 0x70, 0x3c);
const DEAD_WOOD: Rgb = Rgb(0x80, 0x78, 0x70);
const POT: Rgb = Rgb(0xb5, 0x5d, 0x3a);
const SPROUT: Rgb = Rgb(0x7c, 0xc8, 0x4a);

/// How a leaf looks (or None when it is not there) for the season and health.
fn leaf(p: &Part, season: Season, health: Health) -> Option<(char, Rgb)> {
    let roll = (p.tag % 1000) as f64 / 1000.0;
    match health {
        Health::Dead => return None,
        Health::Wilting if roll < 0.55 => return None,
        Health::Wilting => return Some(('&', Rgb(0x9c, 0x8a, 0x3c))),
        Health::Dry => return Some(('&', Rgb(0xb5, 0xa6, 0x42))),
        _ => {}
    }
    Some(match season {
        Season::Spring if roll < 0.35 => ('*', Rgb(0xf5, 0xa3, 0xc7)),
        Season::Spring => ('&', Rgb(0x6a, 0xbe, 0x45)),
        Season::Summer => ('&', Rgb(0x2e, 0x8b, 0x3a)),
        Season::Autumn if roll < 0.4 => ('&', Rgb(0xe0, 0x7b, 0x24)),
        Season::Autumn if roll < 0.75 => ('&', Rgb(0xc8, 0x40, 0x2a)),
        Season::Autumn => ('&', Rgb(0xe8, 0xb9, 0x23)),
        Season::Winter if roll < 0.6 => return None,
        Season::Winter => ('&', Rgb(0x3f, 0x6e, 0x4a)),
    })
}

/// The tree at `growth` days, its pot below, as lines of colored text,
/// `width` columns wide.
pub fn picture(
    parts: &[Part],
    growth: f64,
    season: Season,
    health: Health,
    width: usize,
    mode: ColorMode,
) -> Vec<String> {
    let width = width.max(24);
    let dead = health == Health::Dead;
    let shown: Vec<(i32, i32, char, Option<Rgb>)> = parts
        .iter()
        .filter(|p| p.born <= growth)
        .filter_map(|p| match p.kind {
            Kind::Trunk => Some((p.x, p.y, p.ch, Some(if dead { DEAD_WOOD } else { TRUNK }))),
            Kind::Branch => Some((p.x, p.y, p.ch, Some(if dead { DEAD_WOOD } else { BRANCH }))),
            Kind::Leaf => leaf(p, season, health).map(|(ch, c)| (p.x, p.y, ch, Some(c))),
        })
        .collect();
    // A young tree carries a sprout at its top until its own leaves come.
    let mut shown = shown;
    let has_leaves = shown.iter().any(|&(_, _, ch, _)| ch == '&' || ch == '*');
    if !dead && !has_leaves {
        if let Some(&(x, y, _, _)) = shown
            .iter()
            .filter(|s| s.3 == Some(TRUNK))
            .max_by_key(|s| s.1)
        {
            // The sprout yellows like the leaves when the soil is dry.
            let c = match health {
                Health::Dry => Rgb(0xb5, 0xa6, 0x42),
                Health::Wilting => Rgb(0x9c, 0x8a, 0x3c),
                _ => SPROUT,
            };
            shown.push((x - 1, y + 1, '\\', Some(c)));
            shown.push((x + 1, y + 1, '/', Some(c)));
        }
    }
    let top = shown.iter().map(|s| s.1).max().unwrap_or(0);
    let height = (top + 1) as usize;
    let center = (width / 2) as i32;
    let mut grid: Vec<Vec<(char, Option<Rgb>)>> = vec![vec![(' ', None); width]; height];
    for (x, y, ch, c) in shown {
        let col = center + x;
        if col >= 0 && (col as usize) < width && y >= 0 {
            grid[(top - y) as usize][col as usize] = (ch, c);
        }
    }
    let mut lines: Vec<String> = grid
        .iter()
        .map(|row| {
            let mut out = String::new();
            let mut current = None;
            let end = row.iter().rposition(|c| c.0 != ' ').map_or(0, |i| i + 1);
            for &(ch, c) in &row[..end] {
                paint(
                    &mut out,
                    ch,
                    if ch == ' ' { current } else { c },
                    mode,
                    &mut current,
                );
            }
            if current.is_some() {
                out.push_str("\x1b[0m");
            }
            out
        })
        .collect();
    // The pot: a rim and a bowl, centered under the trunk.
    let rim = 19.min(width - 2);
    let pot = [
        format!(":{}:", "=".repeat(rim)),
        format!(" \\{}/", "_".repeat(rim - 2)),
    ];
    for row in pot {
        let pad = " ".repeat((center as usize).saturating_sub(rim.div_ceil(2) + 1));
        let mut out = pad;
        let mut current = None;
        for ch in row.chars() {
            paint(
                &mut out,
                ch,
                if ch == ' ' { current } else { Some(POT) },
                mode,
                &mut current,
            );
        }
        if current.is_some() {
            out.push_str("\x1b[0m");
        }
        lines.push(out);
    }
    lines
}

fn pct(x: f64) -> String {
    format!("{}%", (x * 100.0).round() as i64)
}

/// The name's display: the stored name.
fn days(n: i64, lang: Lang) -> String {
    match (lang, n) {
        (Lang::En, 1) => "1 day".to_string(),
        (Lang::En, n) => format!("{n} days"),
        (Lang::Ja, n) => format!("{n}日"),
    }
}

/// Health in a few words.
pub fn health_words(h: Health, lang: Lang) -> &'static str {
    match (h, lang) {
        (Health::Thriving, Lang::En) => "growing",
        (Health::Thirsty, Lang::En) => "thirsty, water it soon",
        (Health::Dry, Lang::En) => "dry, not growing: water it",
        (Health::Wilting, Lang::En) => "wilting: water it now",
        (Health::Dead, Lang::En) => "dead",
        (Health::Thriving, Lang::Ja) => "成長中",
        (Health::Thirsty, Lang::Ja) => "水が少ない。そろそろ水やりを",
        (Health::Dry, Lang::Ja) => "土が乾いて成長が止まっています。水やりを",
        (Health::Wilting, Lang::Ja) => "しおれています。すぐに水やりを",
        (Health::Dead, Lang::Ja) => "枯れてしまいました",
    }
}

/// The lines under the picture.
pub fn status(s: &State, full: f64, lang: Lang) -> String {
    let h = s.health();
    let grown = (s.growth / full).min(1.0);
    let mut out = match lang {
        Lang::En => format!(
            "{}, planted {} ago, {} grown. Water {}: {}.",
            s.name,
            days(s.age_days(), lang),
            pct(grown),
            pct(s.water),
            health_words(h, lang)
        ),
        Lang::Ja => format!(
            "{}(植えてから{}、成長{})。水{}: {}。",
            s.name,
            days(s.age_days(), lang),
            pct(grown),
            pct(s.water),
            health_words(h, lang)
        ),
    };
    if h == Health::Dead {
        out.push_str(match lang {
            Lang::En => " termbonsai plant starts a new one.",
            Lang::Ja => "termbonsai plantで植え直せます。",
        });
    } else if let Some(left) = hours_until_dry(s) {
        let note = match lang {
            Lang::En => format!(" The soil dries out in about {}.", duration(left, lang)),
            Lang::Ja => format!("約{}で土が乾きます。", duration(left, lang)),
        };
        out.push_str(&note);
    }
    out
}

/// Hours until the soil is dry, if it is not dry yet.
pub fn hours_until_dry(s: &State) -> Option<f64> {
    (s.water > 0.0).then_some(s.water * crate::state::WATER_LASTS_HOURS)
}

fn duration(hours: f64, lang: Lang) -> String {
    let h = hours.round() as i64;
    // Whole days when it comes out even, hours otherwise (a full watering
    // lasts 72 hours, so this stays readable).
    match lang {
        Lang::En if h >= 48 && h % 24 == 0 => format!("{} days", h / 24),
        Lang::En if h == 1 => "1 hour".to_string(),
        Lang::En => format!("{h} hours"),
        Lang::Ja if h >= 48 && h % 24 == 0 => format!("{}日", h / 24),
        Lang::Ja => format!("{h}時間"),
    }
}

/// One line for a status bar. With `tmux`, wrapped in tmux color codes.
pub fn line(s: &State, lang: Lang, tmux: bool) -> String {
    let h = s.health();
    let word = match (h, lang) {
        (Health::Thriving, Lang::En) => "ok",
        (Health::Thirsty, Lang::En) => "thirsty",
        (Health::Dry, Lang::En) => "dry",
        (Health::Wilting, Lang::En) => "wilting",
        (Health::Dead, Lang::En) => "dead",
        (Health::Thriving, Lang::Ja) => "元気",
        (Health::Thirsty, Lang::Ja) => "水不足",
        (Health::Dry, Lang::Ja) => "乾燥",
        (Health::Wilting, Lang::Ja) => "しおれ",
        (Health::Dead, Lang::Ja) => "枯死",
    };
    let text = match lang {
        Lang::En => format!("{} {}d water {} {word}", s.name, s.age_days(), pct(s.water)),
        Lang::Ja => format!("{} {}日 水{} {word}", s.name, s.age_days(), pct(s.water)),
    };
    if !tmux {
        return text;
    }
    let color = match h {
        Health::Thriving => "green",
        Health::Thirsty | Health::Dry => "yellow",
        Health::Wilting | Health::Dead => "red",
    };
    // tmux reads #[...] as a style; a # in the name must be doubled.
    format!("#[fg={color}]{}#[default]", text.replace('#', "##"))
}

/// The seconds in a day, re-exported for the CLI.
pub const SECONDS_PER_DAY: i64 = DAY;
