use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use termbonsai::draw::{self, ColorMode, Lang, Season};
use termbonsai::state::{self, Health, Pruned, State, Watered, DAY, HOUR};
use termbonsai::tree::{self, Species};

const HELP: &str = "\
termbonsai: a bonsai that lives in your terminal.

Usage: termbonsai [show]        Show your bonsai (plants one the first time)
       termbonsai water         Water it (the soil dries out in 3 days)
       termbonsai plant         Plant a new one (--force to replace a living tree)
       termbonsai prune         Cut the canopy back by a step (once a day, healthy trees)
       termbonsai grow          Watch it grow from a sprout to today (clears the screen)
       termbonsai line          One line for a status bar (--tmux for tmux colors)
       termbonsai status        Its state as JSON
       termbonsai pots          List every pot and how its tree is doing

It grows only while its soil is wet. Dry for 4 days it starts to drop its
leaves; dry for 10 days it dies. Pruning keeps it compact: growth goes on
inside the bounds it sets.

Options:
  --at OFFSET    Show it at a later time: +30d, +12h (nothing is saved)
  --watered      With --at: as if watered all along
  --seed N       With plant: the tree's shape (random when omitted)
  --name NAME    With plant: its name
  --species S    With plant: classic, pine, maple, cherry or random (classic)
  --pot NAME     Another pot, kept beside the first: letters, digits, - and _
  --all          With water: water every pot
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
    /// None: classic; Some(None): random by seed.
    species: Option<Option<Species>>,
    pot: Option<String>,
    all: bool,
}

/// Pot names are used in file names, so they are kept plain.
fn valid_pot(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
        species: None,
        pot: None,
        all: false,
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
                "--watered" | "--force" | "--tmux" | "--south" | "--all" | "--help" | "--version"
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
            "--species" => {
                let v = val("a species")?;
                a.species = Some(match v.as_str() {
                    "random" => None,
                    s => Some(
                        Species::parse(s)
                            .ok_or("--species must be classic, pine, maple, cherry or random")?,
                    ),
                });
            }
            "--pot" => {
                let v = val("a pot name")?;
                if !valid_pot(&v) {
                    return Err(
                        "--pot: 1 to 32 letters, digits, - or _ (it becomes part of a file name)"
                            .into(),
                    );
                }
                a.pot = Some(v);
            }
            "--all" => a.all = true,
            f if f.starts_with('-') => return Err(format!("unknown option {:?}; see --help", f)),
            c if !command_given
                && matches!(
                    c,
                    "show" | "water" | "plant" | "grow" | "line" | "status" | "prune" | "pots"
                ) =>
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
    only(
        a.command != "plant" && a.species.is_some(),
        "--species goes with plant",
    )?;
    only(a.command != "water" && a.all, "--all goes with water")?;
    only(
        a.all && a.pot.is_some(),
        "--all waters every pot; leave out --pot",
    )?;
    only(
        a.command == "pots" && (a.pot.is_some() || a.at.is_some()),
        "pots lists every pot as it is now; leave out --pot and --at",
    )?;
    only(a.watered && a.at.is_none(), "--watered goes with --at")?;
    only(
        a.at.is_some() && matches!(a.command.as_str(), "water" | "plant" | "prune"),
        "--at only previews; it cannot water, plant or prune",
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

/// The state file of a pot: the main file for the first pot, and next to it
/// `<name>@<pot>` for the others.
fn state_path(a: &Args) -> Result<PathBuf, String> {
    let base = base_path(a)?;
    Ok(match &a.pot {
        Some(pot) => pot_path(&base, pot),
        None => base,
    })
}

fn pot_path(base: &Path, pot: &str) -> PathBuf {
    let mut name = base
        .file_name()
        .map_or_else(|| "bonsai".into(), |n| n.to_os_string());
    name.push(format!("@{pot}"));
    base.with_file_name(name)
}

/// Every pot that has a state file: (pot name or None for the first, path).
fn pots(base: &Path) -> Result<Vec<(Option<String>, PathBuf)>, String> {
    let mut out = Vec::new();
    if base.exists() {
        out.push((None, base.to_path_buf()));
    }
    let prefix = format!(
        "{}@",
        base.file_name()
            .map_or("bonsai".into(), |n| n.to_string_lossy().into_owned())
    );
    let dir = base
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("cannot list {}: {e}", shown(dir))),
    };
    let mut named = Vec::new();
    for entry in entries.flatten() {
        // Plain files only: a symbolic link to another pot would be watered
        // twice under a different lock, and a directory is no tree.
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(pot) = name.strip_prefix(&prefix) {
            if valid_pot(pot) {
                named.push((Some(pot.to_string()), entry.path()));
            }
        }
    }
    named.sort();
    out.extend(named);
    Ok(out)
}

fn base_path(a: &Args) -> Result<PathBuf, String> {
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

/// Exclusive use of the state file while it is read and written: an
/// operating-system lock (flock on Unix, LockFileEx on Windows) on a file
/// next to it, the state file's name plus ".lock". The lock belongs to the
/// process holding it, so it is released when that process ends, even by a
/// crash, and the lock file itself is never removed or replaced.
struct Lock(#[allow(dead_code)] std::fs::File);

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
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock)
            .map_err(|e| format!("cannot open {}: {e}", shown(&lock)))?;
        // Wait up to about 3 seconds for another termbonsai to finish.
        for _ in 0..150 {
            match file.try_lock() {
                Ok(()) => return Ok(Lock(file)),
                Err(std::fs::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    return Err(format!("cannot lock {}: {e}", shown(&lock)))
                }
            }
        }
        Err(format!(
            "{} is in use by another termbonsai; try again",
            shown(path)
        ))
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
    let species = match a.species {
        None => Species::Classic,
        Some(Some(s)) => s,
        // The whole seed, mixed (splitmix64's finalizer), so the species
        // neither follows the name nor depends on a few bits that a
        // clock-based seed leaves at 0.
        Some(None) => {
            let mut z = seed ^ 0x5bd1_e995_9e37_79b9;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            Species::ALL[(z % Species::ALL.len() as u64) as usize]
        }
    };
    let pick = (seed % NAMES_EN.len() as u64) as usize;
    let name = match (&a.name, lang, species) {
        (Some(n), _, _) => state::clean_name(n),
        (None, _, Species::Classic) if lang == Lang::En => NAMES_EN[pick].to_string(),
        (None, _, Species::Classic) => NAMES_JA[pick].to_string(),
        (None, l, sp) => draw::species_name(sp, l).to_string(),
    };
    let mut s = State::plant(seed, &name, now);
    s.species = species;
    s
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
         \"water\":{},\"health\":\"{health}\",\"drySince\":{},\"diedAt\":{},\"waterings\":{},\"season\":\"{season}\",\
         \"species\":\"{}\",\"trim\":{},\"prunedAt\":{},\"prunings\":{}}}",
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
        s.waterings,
        s.species.id(),
        s.trim.map_or("null".to_string(), |t| format!(
            "{{\"x\":{},\"y\":{}}}",
            t.x, t.y
        )),
        s.pruned_at.map_or("null".to_string(), |t| t.to_string()),
        s.prunings
    )
}

fn planted_message(s: &State, lang: Lang) -> String {
    match lang {
        Lang::En => format!("{} is planted. Water it every couple of days.", s.name),
        Lang::Ja => format!("{}を植えました。2日に1回くらい水をやってください。", s.name),
    }
}

fn watered_message(result: Watered, name: &str, lang: Lang) -> String {
    match (result, lang) {
        (Watered::Done, Lang::En) => format!("You water {name}. The soil is wet again."),
        (Watered::Done, Lang::Ja) => format!("{name}に水をやりました。土が湿りました。"),
        (Watered::AlreadyMoist, Lang::En) => format!("{name}'s soil is still moist; no need yet."),
        (Watered::AlreadyMoist, Lang::Ja) => {
            format!("{name}の土はまだ湿っています。今はまだ不要です。")
        }
        (Watered::Dead, Lang::En) => format!("{name} has died. termbonsai plant starts a new one."),
        (Watered::Dead, Lang::Ja) => {
            format!("{name}は枯れてしまいました。termbonsai plantで植え直せます。")
        }
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

    if a.command == "pots" {
        let list = pots(&base_path(&a)?)?;
        if list.is_empty() {
            say(match lang {
                Lang::En => "No pots yet; termbonsai plant plants the first tree.",
                Lang::Ja => "まだ鉢がありません。termbonsai plantで最初の木を植えられます。",
            })?;
        }
        for (pot, p) in &list {
            let label = pot.clone().unwrap_or_else(|| match lang {
                Lang::En => "(first)".to_string(),
                Lang::Ja => "(最初の鉢)".to_string(),
            });
            let row = match load(p)? {
                Load::Tree(s) => draw::line(&s.at(now), lang, false),
                Load::Unreadable(e) => e,
                Load::Missing => continue,
            };
            say(&format!("{label:<12} {row}"))?;
        }
        return Ok(ExitCode::SUCCESS);
    }

    if a.command == "water" && a.all {
        let list = pots(&base_path(&a)?)?;
        if list.is_empty() {
            return Err(match lang {
                Lang::En => "there is no bonsai yet; termbonsai plant plants one".to_string(),
                Lang::Ja => "まだ盆栽がありません。termbonsai plantで植えられます".to_string(),
            });
        }
        // Every pot gets its turn; a pot that fails is reported and the run
        // ends with status 2 after the others.
        let mut failed = false;
        for (pot, p) in &list {
            let label = pot
                .as_deref()
                .map(|n| format!("[{n}] "))
                .unwrap_or_default();
            let attempt = (|| -> Result<Option<String>, String> {
                let _lock = Lock::take(p)?;
                let mut s = match load(p)? {
                    Load::Tree(s) => s,
                    Load::Unreadable(e) => return Err(e),
                    Load::Missing => return Ok(None),
                };
                let result = s.water(now);
                save(p, &s)?;
                Ok(Some(watered_message(result, &s.name, lang)))
            })();
            match attempt {
                Ok(Some(msg)) => say(&format!("{label}{msg}"))?,
                Ok(None) => {}
                Err(e) => {
                    eprintln!("termbonsai: {label}{e}");
                    failed = true;
                }
            }
        }
        return Ok(if failed {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        });
    }

    if a.command == "prune" {
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
        let parts = tree::grow_species(s.seed, s.species);
        let result = s.prune(now, &parts);
        if let Pruned::Done(_) = result {
            save(&path, &s)?;
        }
        say(&match (result, lang) {
            (Pruned::Done(n), Lang::En) => format!(
                "You prune {}: {n} shoot{} and leaves cut back. It keeps growing inside the new shape.",
                s.name,
                if n == 1 { "" } else { "s" }
            ),
            (Pruned::Done(n), Lang::Ja) => format!(
                "{}を剪定しました。{n}か所を切り戻しました。この形の中で育ち続けます。",
                s.name
            ),
            (Pruned::TooSoon(h), Lang::En) => format!(
                "{} was pruned recently; let it recover for another {h} hour{}.",
                s.name,
                if h == 1 { "" } else { "s" }
            ),
            (Pruned::TooSoon(h), Lang::Ja) => format!(
                "{}は剪定したばかりです。あと{h}時間ほど休ませてください。",
                s.name
            ),
            (Pruned::NotHealthy(Health::Dead), Lang::En) => {
                format!("{} has died; there is nothing to prune.", s.name)
            }
            (Pruned::NotHealthy(Health::Dead), Lang::Ja) => {
                format!("{}は枯れてしまいました。剪定はできません。", s.name)
            }
            (Pruned::NotHealthy(_), Lang::En) => format!(
                "{} is too dry to prune; water it, and prune once it is growing again.",
                s.name
            ),
            (Pruned::NotHealthy(_), Lang::Ja) => format!(
                "{}は乾きすぎていて剪定できません。水をやり、また育ち始めてから剪定してください。",
                s.name
            ),
            (Pruned::NothingToCut, Lang::En) => format!(
                "{} has nothing to cut back yet; let it grow out first.",
                s.name
            ),
            (Pruned::NothingToCut, Lang::Ja) => {
                format!("{}にはまだ切る枝がありません。もう少し育ててから。", s.name)
            }
        })?;
        return Ok(match result {
            Pruned::Done(_) => ExitCode::SUCCESS,
            _ => ExitCode::from(1),
        });
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
        say(&watered_message(result, &s.name, lang))?;
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
    let parts = tree::grow_species(view.seed, view.species);
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
                let pic = draw::picture_of(
                    &parts,
                    g,
                    season,
                    health,
                    a.width,
                    mode,
                    view.species,
                    view.trim,
                );
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
            let pic = draw::picture_of(
                &parts,
                view.growth,
                season,
                view.health(),
                a.width,
                mode,
                view.species,
                view.trim,
            );
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
