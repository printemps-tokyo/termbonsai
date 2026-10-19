use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use termbonsai::draw::{self, ColorMode, Lang, Season};
use termbonsai::state::{self, Health, State, Watered, DAY, HOUR};
use termbonsai::tree;

const HELP: &str = "\
termbonsai: a bonsai that lives in your terminal.

Usage: termbonsai [show]        Show your bonsai (plants one the first time)
       termbonsai water         Water it (the soil dries out in 3 days)
       termbonsai plant         Plant a new one (--force to replace a living tree)
       termbonsai grow          Watch it grow from a seed to today
       termbonsai line          One line for a status bar (--tmux for tmux colors)
       termbonsai status        Its state as JSON

It grows only while its soil is wet. Dry for 4 days it starts to drop its
leaves; dry for 10 days it dies.

Options:
  --at OFFSET    Show it at another time: +30d, +12h, -2d (nothing is saved)
  --watered      With --at: as if watered all along
  --seed N       With plant: the tree's shape (random when omitted)
  --name NAME    With plant: its name
  --force        With plant: replace a living tree
  --tmux         With line: tmux color codes
  --season S     auto, spring, summer, autumn or winter (auto follows the month)
  --south        Southern hemisphere seasons
  --width N      Picture width, 24 to 200 (60)
  --color C      auto, always or never
  --lang L       auto, ja or en (auto reads LC_ALL, LC_MESSAGES, LANG)
  --state PATH   The state file (or TERMBONSAI_STATE; default
                 $XDG_STATE_HOME/termbonsai/bonsai or ~/.local/state/termbonsai/bonsai)
  -h, --help     This help
  -V, --version  Version
";

const NAMES_EN: [&str; 8] = [
    "Momiji", "Matsu", "Sakura", "Kaede", "Ume", "Hinoki", "Sugi", "Keyaki",
];
const NAMES_JA: [&str; 8] = [
    "もみじ",
    "まつ",
    "さくら",
    "かえで",
    "うめ",
    "ひのき",
    "すぎ",
    "けやき",
];

struct Args {
    command: String,
    at: Option<i64>,
    watered: bool,
    seed: Option<u64>,
    name: Option<String>,
    force: bool,
    tmux: bool,
    season: Option<Season>,
    south: bool,
    width: usize,
    color: String,
    lang: Option<Lang>,
    state: Option<PathBuf>,
}

/// "+30d", "12h", "-2d" in seconds.
fn offset(v: &str) -> Result<i64, String> {
    let bad = || format!("--at {v:?}: use a number of days or hours, like +30d or -12h");
    let (num, unit) = v.split_at(v.len().checked_sub(1).ok_or_else(bad)?);
    let n: i64 = num.trim_start_matches('+').parse().map_err(|_| bad())?;
    let unit = match unit {
        "d" => DAY,
        "h" => HOUR,
        _ => return Err(bad()),
    };
    if n.abs() > 100 * 365 * DAY / unit {
        return Err("--at: at most 100 years either way".into());
    }
    Ok(n * unit)
}

fn parse(argv: Vec<String>) -> Result<Option<Args>, String> {
    let mut a = Args {
        command: "show".into(),
        at: None,
        watered: false,
        seed: None,
        name: None,
        force: false,
        tmux: false,
        season: None,
        south: false,
        width: 60,
        color: "auto".into(),
        lang: None,
        state: None,
    };
    let mut command_given = false;
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        if inline.is_some()
            && matches!(
                flag.as_str(),
                "--watered" | "--force" | "--tmux" | "--south" | "--help" | "--version"
            )
        {
            return Err(format!("{flag} takes no value"));
        }
        let mut val = |what: &str| {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or(format!("{flag} needs {what}"))
        };
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("termbonsai {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--at" => a.at = Some(offset(&val("an offset")?)?),
            "--watered" => a.watered = true,
            "--seed" => {
                a.seed = Some(
                    val("a number")?
                        .parse()
                        .map_err(|_| "--seed must be a whole number".to_string())?,
                )
            }
            "--name" => a.name = Some(val("a name")?),
            "--force" => a.force = true,
            "--tmux" => a.tmux = true,
            "--season" => {
                let v = val("a season")?;
                a.season = match v.as_str() {
                    "auto" => None,
                    s => Some(
                        Season::parse(s)
                            .ok_or("--season must be auto, spring, summer, autumn or winter")?,
                    ),
                }
            }
            "--south" => a.south = true,
            "--width" => {
                let w: usize = val("a number")?
                    .parse()
                    .map_err(|_| "--width must be a number".to_string())?;
                if !(24..=200).contains(&w) {
                    return Err("--width must be from 24 to 200".into());
                }
                a.width = w;
            }
            "--color" => {
                let v = val("auto, always or never")?;
                if !matches!(v.as_str(), "auto" | "always" | "never") {
                    return Err("--color must be auto, always or never".into());
                }
                a.color = v;
            }
            "--lang" => {
                a.lang = match val("auto, ja or en")?.as_str() {
                    "auto" => None,
                    "ja" => Some(Lang::Ja),
                    "en" => Some(Lang::En),
                    _ => return Err("--lang must be auto, ja or en".into()),
                }
            }
            "--state" => a.state = Some(PathBuf::from(val("a file name")?)),
            f if f.starts_with('-') => return Err(format!("unknown option {f:?}; see --help")),
            c if !command_given
                && matches!(c, "show" | "water" | "plant" | "grow" | "line" | "status") =>
            {
                a.command = c.to_string();
                command_given = true;
            }
            other => return Err(format!("unknown command {other:?}; see --help")),
        }
    }
    let only = |cond: bool, what: &str| if cond { Err(what.to_string()) } else { Ok(()) };
    only(
        a.command != "plant" && (a.seed.is_some() || a.name.is_some() || a.force),
        "--seed, --name and --force go with plant",
    )?;
    only(a.command != "line" && a.tmux, "--tmux goes with line")?;
    only(a.watered && a.at.is_none(), "--watered goes with --at")?;
    only(
        a.at.is_some() && matches!(a.command.as_str(), "water" | "plant"),
        "--at only previews; it cannot water or plant",
    )?;
    Ok(Some(a))
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// The current time; TERMBONSAI_NOW (Unix seconds) overrides it, for tests.
fn now() -> Result<i64, String> {
    match env("TERMBONSAI_NOW") {
        Some(v) => v
            .parse()
            .map_err(|_| "TERMBONSAI_NOW must be Unix seconds".into()),
        None => Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)),
    }
}

fn state_path(a: &Args) -> Result<PathBuf, String> {
    if let Some(p) = &a.state {
        return Ok(p.clone());
    }
    if let Some(p) = env("TERMBONSAI_STATE") {
        return Ok(PathBuf::from(p));
    }
    if let Some(d) = env("XDG_STATE_HOME") {
        return Ok(PathBuf::from(d).join("termbonsai").join("bonsai"));
    }
    let home = env("HOME").ok_or("cannot find a home directory; use --state PATH")?;
    Ok(PathBuf::from(home).join(".local/state/termbonsai/bonsai"))
}

fn load(path: &PathBuf) -> Result<Option<State>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => State::from_text(&text).map(Some).map_err(|e| {
            format!(
                "{}: {e} (termbonsai plant --force starts over)",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Write by a temporary file and a rename, so an interrupted save never
/// leaves half a file.
fn save(path: &PathBuf, s: &State) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, s.to_text())
        .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot save {}: {e}", path.display()))
}

fn color_mode(arg: &str, tty: bool) -> ColorMode {
    let rich = || match env("COLORTERM").as_deref() {
        Some("truecolor") | Some("24bit") => ColorMode::TrueColor,
        _ => ColorMode::Ansi256,
    };
    match arg {
        "never" => ColorMode::None,
        "always" => rich(),
        _ if env("NO_COLOR").is_some() || !tty || env("TERM").as_deref() == Some("dumb") => {
            ColorMode::None
        }
        _ => rich(),
    }
}

fn new_tree(a: &Args, now: i64, lang: Lang) -> State {
    let seed = a.seed.unwrap_or_else(|| {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        t ^ (std::process::id() as u64).rotate_left(32)
    });
    let pick = (seed % NAMES_EN.len() as u64) as usize;
    let name = match (&a.name, lang) {
        (Some(n), _) => state::clean_name(n),
        (None, Lang::En) => NAMES_EN[pick].to_string(),
        (None, Lang::Ja) => NAMES_JA[pick].to_string(),
    };
    State::plant(seed, &name, now)
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn status_json(s: &State, full: f64, season: Season) -> String {
    let health = match s.health() {
        Health::Thriving => "thriving",
        Health::Thirsty => "thirsty",
        Health::Dry => "dry",
        Health::Wilting => "wilting",
        Health::Dead => "dead",
    };
    let season = match season {
        Season::Spring => "spring",
        Season::Summer => "summer",
        Season::Autumn => "autumn",
        Season::Winter => "winter",
    };
    format!(
        "{{\"name\":{},\"seed\":{},\"plantedAt\":{},\"at\":{},\"ageDays\":{},\"growthDays\":{},\"fullGrownDays\":{},\
         \"water\":{},\"health\":\"{health}\",\"drySince\":{},\"diedAt\":{},\"waterings\":{},\"season\":\"{season}\"}}",
        json_str(&s.name),
        s.seed,
        s.planted_at,
        s.updated_at,
        s.age_days(),
        s.growth,
        full,
        s.water,
        s.dry_since.map_or("null".to_string(), |d| d.to_string()),
        if s.health() == Health::Dead { s.died_at().map_or("null".to_string(), |d| d.to_string()) } else { "null".to_string() },
        s.waterings
    )
}

fn run(a: Args) -> Result<ExitCode, String> {
    let lang = a
        .lang
        .unwrap_or_else(|| Lang::from_env(|k| std::env::var(k).ok()));
    let tty = std::io::stdout().is_terminal();
    let mode = color_mode(&a.color, tty);
    let path = state_path(&a)?;
    let now = now()?;
    let mut out = std::io::stdout().lock();
    let mut say = |s: &str| writeln!(out, "{s}").map_err(|e| e.to_string());

    if a.command == "plant" {
        if let Some(old) = load(&path)? {
            if old.at(now).health() != Health::Dead && !a.force {
                return Err(format!(
                    "{} is still alive; termbonsai plant --force replaces it",
                    old.name
                ));
            }
        }
        let s = new_tree(&a, now, lang);
        save(&path, &s)?;
        say(&match lang {
            Lang::En => format!("{} is planted. Water it every couple of days.", s.name),
            Lang::Ja => format!("{}を植えました。2日に1回くらい水をやってください。", s.name),
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut s = match load(&path)? {
        Some(s) => s,
        None => {
            // The first time: plant one, so a plain `termbonsai` just works.
            let s = new_tree(&a, now, lang);
            save(&path, &s)?;
            if a.command != "line" && a.command != "status" {
                say(&match lang {
                    Lang::En => format!("A new bonsai, {}, is planted.\n", s.name),
                    Lang::Ja => format!("新しい盆栽、{}を植えました。\n", s.name),
                })?;
            }
            s
        }
    };

    if a.command == "water" {
        let result = s.water(now);
        save(&path, &s)?;
        say(&match (result, lang) {
            (Watered::Done, Lang::En) => format!("You water {}. The soil is wet again.", s.name),
            (Watered::Done, Lang::Ja) => format!("{}に水をやりました。土が湿りました。", s.name),
            (Watered::AlreadyMoist, Lang::En) => {
                format!("{}'s soil is still moist; no need yet.", s.name)
            }
            (Watered::AlreadyMoist, Lang::Ja) => {
                format!("{}の土はまだ湿っています。今はまだ不要です。", s.name)
            }
            (Watered::Dead, Lang::En) => {
                format!("{} has died. termbonsai plant starts a new one.", s.name)
            }
            (Watered::Dead, Lang::Ja) => format!(
                "{}は枯れてしまいました。termbonsai plantで植え直せます。",
                s.name
            ),
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    // What to show: now, or a preview at another time.
    let when = now + a.at.unwrap_or(0);
    let mut view = s.at(when.max(s.updated_at));
    if a.watered {
        if let Some(off) = a.at.filter(|o| *o > 0) {
            // As if watered all along: every hour from now counts as growth.
            let base = s.at(now);
            if base.health() != Health::Dead {
                view = State {
                    growth: base.growth + off as f64 / DAY as f64,
                    water: 1.0,
                    dry_since: None,
                    updated_at: when,
                    ..base
                };
            }
        }
    }
    let season = a
        .season
        .unwrap_or_else(|| Season::of_month(state::month_of(when), a.south));
    let parts = tree::grow(view.seed);
    let full = tree::full_grown(&parts);

    match a.command.as_str() {
        "line" => {
            say(&draw::line(&view, lang, a.tmux))?;
        }
        "status" => {
            say(&status_json(&view, full, season))?;
        }
        "grow" => {
            // From a seed to today's size, a frame every 40 ms; on a terminal
            // each frame replaces the last.
            let frames = 60;
            let mut previous = 0;
            for i in 0..=frames {
                let g = view.growth * i as f64 / frames as f64;
                let health = if i == frames {
                    view.health()
                } else {
                    Health::Thriving
                };
                let pic = draw::picture(&parts, g, season, health, a.width, mode);
                if tty && previous > 0 {
                    // Up to the top of the last frame, and clear below.
                    write!(out, "\x1b[{previous}A\x1b[J").map_err(|e| e.to_string())?;
                }
                if tty || i == frames {
                    writeln!(out, "{}", pic.join("\n")).map_err(|e| e.to_string())?;
                    out.flush().map_err(|e| e.to_string())?;
                    previous = pic.len();
                }
                if tty && i < frames {
                    std::thread::sleep(Duration::from_millis(40));
                }
            }
            writeln!(out, "\n{}", draw::status(&view, full, lang)).map_err(|e| e.to_string())?;
        }
        _ => {
            let pic = draw::picture(&parts, view.growth, season, view.health(), a.width, mode);
            let mut text = pic.join("\n");
            text.push_str("\n\n");
            text.push_str(&draw::status(&view, full, lang));
            if a.at.is_some() {
                text.push_str(match lang {
                    Lang::En => "\n(a preview; nothing was saved)",
                    Lang::Ja => "\n(プレビューです。保存はしていません)",
                });
            }
            say(&text)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match parse(argv).and_then(|a| match a {
        None => Ok(ExitCode::SUCCESS),
        Some(a) => run(a),
    }) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("termbonsai: {e}");
            ExitCode::from(2)
        }
    }
}
