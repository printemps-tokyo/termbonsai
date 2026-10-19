use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
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
       termbonsai grow          Watch it grow from a sprout to today (clears the screen)
       termbonsai line          One line for a status bar (--tmux for tmux colors)
       termbonsai status        Its state as JSON

It grows only while its soil is wet. Dry for 4 days it starts to drop its
leaves; dry for 10 days it dies.

Options:
  --at OFFSET    Show it at a later time: +30d, +12h (nothing is saved)
  --watered      With --at: as if watered all along
  --seed N       With plant: the tree's shape (random when omitted)
  --name NAME    With plant: its name
  --force        With plant: replace a living tree, or a state file that
                 cannot be read
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

/// "+30d", "12h" in seconds. Only the future: the tree keeps no history of
/// its past, so a look back could not be drawn honestly.
fn offset(v: &str) -> Result<i64, String> {
    let bad = || format!("--at {v:?}: use a number of days or hours ahead, like +30d or +12h");
    let (num, unit, max) = if let Some(n) = v.strip_suffix('d') {
        (n, DAY, 36_500)
    } else if let Some(n) = v.strip_suffix('h') {
        (n, HOUR, 876_000)
    } else {
        return Err(bad());
    };
    let n: i64 = num
        .strip_prefix('+')
        .unwrap_or(num)
        .parse()
        .map_err(|_| bad())?;
    if n < 0 {
        return Err("--at looks ahead only: the tree keeps no history of its past".into());
    }
    if n > max {
        return Err("--at: at most 100 years ahead".into());
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
            f if f.starts_with('-') => return Err(format!("unknown option {:?}; see --help", f)),
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
    let t = match env("TERMBONSAI_NOW") {
        Some(v) => v
            .parse::<i64>()
            .map_err(|_| "TERMBONSAI_NOW must be Unix seconds".to_string())?,
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
    };
    if !state::valid_time(t) {
        return Err("the clock is outside 1900 to 2400".into());
    }
    Ok(t)
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

/// A path for messages, with any control characters escaped.
fn shown(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

enum Load {
    Missing,
    Tree(State),
    /// The file is there but is not a state this version can read.
    Unreadable(String),
}

fn load(path: &Path) -> Result<Load, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(match State::from_text(&text) {
            Ok(s) => Load::Tree(s),
            Err(e) => Load::Unreadable(format!(
                "{}: {e} (termbonsai plant --force starts over)",
                shown(path)
            )),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Load::Missing),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Load::Unreadable(format!(
            "{}: not text (termbonsai plant --force starts over)",
            shown(path)
        ))),
        Err(e) => Err(format!("cannot read {}: {e}", shown(path))),
    }
}

/// Exclusive use of the state file while it is read and written, through a
/// lock file next to it (the state file's name plus ".lock"). The lock holds
/// a token of its own; a lock older than 30 seconds, left by a crash, is
/// taken over by renaming it away, which only one process can do, and a
/// lock is removed on release only while it still holds our token.
struct Lock {
    path: PathBuf,
    token: String,
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| "bonsai".into(), |n| n.to_os_string());
    name.push(".lock");
    path.with_file_name(name)
}

impl Lock {
    fn take(path: &Path) -> Result<Lock, String> {
        let lock = lock_path(path);
        if let Some(dir) = lock.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", shown(dir)))?;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let token = format!("{} {nanos}", std::process::id());
        for attempt in 0..150u32 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(mut f) => {
                    f.write_all(token.as_bytes())
                        .map_err(|e| format!("cannot lock {}: {e}", shown(&lock)))?;
                    return Ok(Lock { path: lock, token });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&lock)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30));
                    if stale {
                        // Rename, not remove: of several processes finding the
                        // same stale lock, exactly one moves it away.
                        let aside =
                            lock.with_extension(format!("stale.{}.{attempt}", std::process::id()));
                        if std::fs::rename(&lock, &aside).is_ok() {
                            let _ = std::fs::remove_file(&aside);
                        }
                    } else {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
                Err(e) => return Err(format!("cannot lock {}: {e}", shown(&lock))),
            }
        }
        Err(format!(
            "{} is locked by another termbonsai; try again",
            shown(path)
        ))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if std::fs::read_to_string(&self.path).is_ok_and(|t| t == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Write to a new temporary file with a name of its own, then rename it over
/// the state file, so an interrupted save never leaves half a file and two
/// saves never share a temporary file.
fn save(path: &Path, s: &State) -> Result<(), String> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", shown(dir)))?;
    let base = path
        .file_name()
        .map_or("bonsai".into(), |n| n.to_string_lossy().into_owned());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let tmp = dir.join(format!(".{base}.{}.{nanos}.tmp", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(s.to_text().as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot save {}: {e}", shown(path))
    })
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
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
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
    let died = if s.health() == Health::Dead {
        s.died_at()
    } else {
        None
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
        died.map_or("null".to_string(), |d| d.to_string()),
        s.waterings
    )
}

fn planted_message(s: &State, lang: Lang) -> String {
    match lang {
        Lang::En => format!("{} is planted. Water it every couple of days.", s.name),
        Lang::Ja => format!("{}を植えました。2日に1回くらい水をやってください。", s.name),
    }
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
        let _lock = Lock::take(&path)?;
        // With --force even a file that cannot be read at all is replaced.
        let existing = match load(&path) {
            Err(_) if a.force => Load::Missing,
            other => other?,
        };
        match existing {
            Load::Tree(old) if old.at(now).health() != Health::Dead && !a.force => {
                return Err(format!(
                    "{} is still alive; termbonsai plant --force replaces it",
                    old.name
                ));
            }
            Load::Unreadable(e) if !a.force => return Err(e),
            _ => {}
        }
        let s = new_tree(&a, now, lang);
        save(&path, &s)?;
        say(&planted_message(&s, lang))?;
        return Ok(ExitCode::SUCCESS);
    }

    if a.command == "water" {
        let _lock = Lock::take(&path)?;
        let mut s = match load(&path)? {
            Load::Tree(s) => s,
            Load::Unreadable(e) => return Err(e),
            Load::Missing => {
                return Err(match lang {
                    Lang::En => "there is no bonsai yet; termbonsai plant plants one".to_string(),
                    Lang::Ja => "まだ盆栽がありません。termbonsai plantで植えられます".to_string(),
                })
            }
        };
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

    // The tree to look at. The first look plants one, so a plain
    // `termbonsai` just works; a preview never saves.
    let (s, fresh) = match load(&path)? {
        Load::Tree(s) => (s, false),
        Load::Unreadable(e) => return Err(e),
        Load::Missing if a.at.is_some() => (new_tree(&a, now, lang), true),
        Load::Missing => {
            let _lock = Lock::take(&path)?;
            match load(&path)? {
                // Another termbonsai planted it meanwhile.
                Load::Tree(s) => (s, false),
                Load::Unreadable(e) => return Err(e),
                Load::Missing => {
                    let s = new_tree(&a, now, lang);
                    save(&path, &s)?;
                    if a.command != "line" && a.command != "status" {
                        say(&match lang {
                            Lang::En => format!("A new bonsai, {}, is planted.\n", s.name),
                            Lang::Ja => format!("新しい盆栽、{}を植えました。\n", s.name),
                        })?;
                    }
                    (s, false)
                }
            }
        }
    };

    // What to show: now, or a preview at a later time.
    let when = now + a.at.unwrap_or(0);
    let mut view = s.at(when);
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
    // The season of the moment shown.
    let season = a
        .season
        .unwrap_or_else(|| Season::of_month(state::month_of(view.updated_at), a.south));
    let parts = tree::grow(view.seed);
    let full = tree::full_grown(&parts);
    let preview_note = match (fresh, lang) {
        (true, Lang::En) => "\n(a preview of a tree not yet planted; nothing was saved)",
        (true, Lang::Ja) => "\n(まだ植えていない盆栽のプレビューです。保存はしていません)",
        (false, Lang::En) => "\n(a preview; nothing was saved)",
        (false, Lang::Ja) => "\n(プレビューです。保存はしていません)",
    };

    match a.command.as_str() {
        "line" => say(&draw::line(&view, lang, a.tmux))?,
        "status" => say(&status_json(&view, full, season))?,
        "grow" => {
            // From a sprout to today's size, a frame every 40 ms. On a
            // terminal every frame is drawn from the top of a cleared screen,
            // so lines that wrap cannot throw the next frame off.
            let frames = 60;
            for i in 0..=frames {
                let g = view.growth * i as f64 / frames as f64;
                let health = if i == frames {
                    view.health()
                } else {
                    Health::Thriving
                };
                let pic = draw::picture(&parts, g, season, health, a.width, mode);
                if tty {
                    write!(out, "\x1b[H\x1b[2J").map_err(|e| e.to_string())?;
                }
                if tty || i == frames {
                    writeln!(out, "{}", pic.join("\n")).map_err(|e| e.to_string())?;
                    out.flush().map_err(|e| e.to_string())?;
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
                text.push_str(preview_note);
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
