//! The bonsai's life: when it was planted, how much it has grown, how wet its
//! soil is. Pure: the state at any moment follows from the saved state and
//! the time, so showing the tree never has to write anything.

use std::fmt::Write as _;

pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;

/// A full watering lasts this long before the soil is dry.
pub const WATER_LASTS_HOURS: f64 = 72.0;
/// Dry this long and the leaves start to drop.
pub const WILT_AFTER_DAYS: f64 = 4.0;
/// Dry this long and the tree dies.
pub const DIE_AFTER_DAYS: f64 = 10.0;
/// Watering wetter soil than this does nothing.
pub const MOIST_ENOUGH: f64 = 0.9;
/// Below this the tree asks for water.
pub const THIRSTY_BELOW: f64 = 0.25;

/// Times are accepted from 1900 to 2400 (Unix seconds), far inside i64, so
/// no difference of two of them can overflow.
pub const EARLIEST: i64 = -2_208_988_800;
pub const LATEST: i64 = 13_574_563_200;

pub fn valid_time(t: i64) -> bool {
    (EARLIEST..=LATEST).contains(&t)
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub seed: u64,
    pub name: String,
    /// Unix seconds.
    pub planted_at: i64,
    /// The moment `water`, `growth` and `dry_since` describe.
    pub updated_at: i64,
    /// Soil moisture, 0 (dry) to 1 (just watered).
    pub water: f64,
    /// Days of growth: time spent with water in the soil.
    pub growth: f64,
    /// When the soil last ran dry, if it is dry.
    pub dry_since: Option<i64>,
    pub waterings: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Thriving,
    Thirsty,
    /// Dry: not growing, leaves still on.
    Dry,
    /// Dry for a while: leaves dropping.
    Wilting,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Watered {
    Done,
    /// The soil was still moist; nothing changed.
    AlreadyMoist,
    Dead,
}

impl State {
    pub fn plant(seed: u64, name: &str, now: i64) -> State {
        State {
            seed,
            name: name.to_string(),
            planted_at: now,
            updated_at: now,
            water: 1.0,
            growth: 0.0,
            dry_since: None,
            waterings: 0,
        }
    }

    /// When it died, if it has: `DIE_AFTER_DAYS` after the soil ran dry.
    pub fn died_at(&self) -> Option<i64> {
        self.dry_since
            .map(|d| d + (DIE_AFTER_DAYS * DAY as f64) as i64)
    }

    /// The state at `now`. The soil dries linearly; the tree grows only while
    /// there is water. A time before `updated_at` gives the state unchanged.
    pub fn at(&self, now: i64) -> State {
        let mut s = self.clone();
        if now <= self.updated_at {
            return s;
        }
        let elapsed_h = (now - self.updated_at) as f64 / HOUR as f64;
        let wet_h = self.water * WATER_LASTS_HOURS;
        s.growth += elapsed_h.min(wet_h) / 24.0;
        s.water = (self.water - elapsed_h / WATER_LASTS_HOURS).max(0.0);
        if s.water == 0.0 && s.dry_since.is_none() {
            s.dry_since = Some(self.updated_at + (wet_h * HOUR as f64).round() as i64);
        }
        s.updated_at = now;
        s
    }

    pub fn health(&self) -> Health {
        let dry_days = |since: i64| (self.updated_at - since) as f64 / DAY as f64;
        match self.dry_since {
            Some(since) if dry_days(since) >= DIE_AFTER_DAYS => Health::Dead,
            Some(since) if dry_days(since) >= WILT_AFTER_DAYS => Health::Wilting,
            Some(_) => Health::Dry,
            None if self.water < THIRSTY_BELOW => Health::Thirsty,
            None => Health::Thriving,
        }
    }

    /// Water it at `now` (the state is first brought up to `now`).
    pub fn water(&mut self, now: i64) -> Watered {
        let s = self.at(now);
        *self = s;
        if self.health() == Health::Dead {
            return Watered::Dead;
        }
        if self.water >= MOIST_ENOUGH {
            return Watered::AlreadyMoist;
        }
        self.water = 1.0;
        self.dry_since = None;
        self.waterings = self.waterings.saturating_add(1);
        Watered::Done
    }

    /// Whole days since planting.
    pub fn age_days(&self) -> i64 {
        ((self.updated_at - self.planted_at) / DAY).max(0)
    }

    /// The saved form: one `key=value` per line.
    pub fn to_text(&self) -> String {
        let mut s = String::from("termbonsai 1\n");
        let _ = writeln!(s, "seed={}", self.seed);
        let _ = writeln!(s, "name={}", self.name);
        let _ = writeln!(s, "planted_at={}", self.planted_at);
        let _ = writeln!(s, "updated_at={}", self.updated_at);
        let _ = writeln!(s, "water={}", self.water);
        let _ = writeln!(s, "growth={}", self.growth);
        if let Some(d) = self.dry_since {
            let _ = writeln!(s, "dry_since={d}");
        }
        let _ = writeln!(s, "waterings={}", self.waterings);
        s
    }

    pub fn from_text(text: &str) -> Result<State, String> {
        let mut lines = text.lines();
        if lines.next() != Some("termbonsai 1") {
            return Err("not a termbonsai state file (or a newer version)".into());
        }
        let mut get = std::collections::HashMap::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("bad line {line:?}"))?;
            if get.insert(k.to_string(), v.to_string()).is_some() {
                return Err(format!("{k} appears twice"));
            }
        }
        fn num<T: std::str::FromStr>(
            get: &std::collections::HashMap<String, String>,
            k: &str,
        ) -> Result<T, String> {
            get.get(k)
                .ok_or_else(|| format!("missing {k}"))?
                .parse()
                .map_err(|_| format!("bad {k}"))
        }
        let s = State {
            seed: num(&get, "seed")?,
            name: clean_name(get.get("name").map(String::as_str).unwrap_or("bonsai")),
            planted_at: num(&get, "planted_at")?,
            updated_at: num(&get, "updated_at")?,
            water: num(&get, "water")?,
            growth: num(&get, "growth")?,
            dry_since: get
                .get("dry_since")
                .map(|v| v.parse().map_err(|_| "bad dry_since".to_string()))
                .transpose()?,
            waterings: num(&get, "waterings")?,
        };
        let times_ok = valid_time(s.planted_at)
            && valid_time(s.updated_at)
            && s.planted_at <= s.updated_at
            && s.dry_since
                .is_none_or(|d| valid_time(d) && s.planted_at <= d && d <= s.updated_at);
        // Dry soil and a dry-since time go together.
        let dry_ok = (s.water == 0.0) == s.dry_since.is_some();
        // No more growth than the time it has been alive.
        // Only once the times are known to be in range can they be subtracted.
        let growth_ok = times_ok && s.growth.is_finite() && s.growth >= 0.0 && {
            let alive_days = (s.updated_at - s.planted_at) as f64 / DAY as f64;
            s.growth <= alive_days + 1e-6
        };
        if !(0.0..=1.0).contains(&s.water) || !times_ok || !dry_ok || !growth_ok {
            return Err("the values in the state file do not fit together".into());
        }
        Ok(s)
    }
}

/// A name safe to print: control characters dropped (so a name cannot carry
/// terminal escape sequences), at most 40 characters, never empty.
pub fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "bonsai".to_string()
    } else {
        s
    }
}

/// The month (1-12) of a Unix time, in UTC.
pub fn month_of(unix: i64) -> u32 {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = unix.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (if mp < 10 { mp + 3 } else { mp - 9 }) as u32
}
