use termbonsai::draw::Season;
use termbonsai::state::{clean_name, month_of, Health, State, Watered, DAY, HOUR};
use termbonsai::tree::{full_grown, grow, Kind};

const T0: i64 = 1_792_000_000;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_soil_dries_and_growth_stops() {
    let s = State::plant(7, "Momiji", T0);
    assert_eq!(
        (s.water, s.growth, s.health()),
        (1.0, 0.0, Health::Thriving)
    );
    let a = s.at(T0 + 24 * HOUR);
    assert!(close(a.water, 2.0 / 3.0) && close(a.growth, 1.0));
    let thirsty = s.at(T0 + 60 * HOUR);
    assert_eq!(thirsty.health(), Health::Thirsty);
    let dry = s.at(T0 + 100 * HOUR);
    assert_eq!(dry.water, 0.0);
    assert!(
        close(dry.growth, 3.0),
        "growth stops when the soil is dry: {}",
        dry.growth
    );
    assert_eq!(dry.dry_since, Some(T0 + 72 * HOUR));
    assert_eq!(dry.health(), Health::Dry);
    assert_eq!(s.at(T0 + 72 * HOUR + 4 * DAY).health(), Health::Wilting);
    assert_eq!(s.at(T0 + 72 * HOUR + 10 * DAY).health(), Health::Dead);
    assert_eq!(
        s.at(T0 + 72 * HOUR + 10 * DAY).died_at(),
        Some(T0 + 72 * HOUR + 10 * DAY)
    );
    // Looking back changes nothing.
    assert_eq!(a.at(T0), a);
}

#[test]
fn stepping_in_pieces_gives_the_same_state() {
    let s = State::plant(1, "x", T0);
    let whole = s.at(T0 + 90 * HOUR);
    let pieces = s.at(T0 + 10 * HOUR).at(T0 + 50 * HOUR).at(T0 + 90 * HOUR);
    assert!(close(whole.growth, pieces.growth) && close(whole.water, pieces.water));
    assert_eq!(whole.dry_since, pieces.dry_since);
}

#[test]
fn watering() {
    let mut s = State::plant(7, "Momiji", T0);
    assert_eq!(s.water(T0 + HOUR), Watered::AlreadyMoist);
    assert_eq!(s.waterings, 0);
    assert_eq!(s.water(T0 + 48 * HOUR), Watered::Done);
    assert_eq!(
        (s.water, s.waterings, s.updated_at),
        (1.0, 1, T0 + 48 * HOUR)
    );
    assert!(close(s.growth, 2.0));
    // A dry tree recovers when watered in time, keeping its growth.
    let mut d = State::plant(7, "x", T0);
    assert_eq!(d.water(T0 + 72 * HOUR + 5 * DAY), Watered::Done);
    assert_eq!((d.health(), d.dry_since), (Health::Thriving, None));
    assert!(close(d.growth, 3.0));
    // Too late.
    let mut dead = State::plant(7, "x", T0);
    assert_eq!(dead.water(T0 + 72 * HOUR + 11 * DAY), Watered::Dead);
}

#[test]
fn saved_and_read_back() {
    let mut s = State::plant(42, "Kaede", T0);
    s.water(T0 + 50 * HOUR);
    let s = s.at(T0 + 200 * HOUR);
    let back = State::from_text(&s.to_text()).unwrap();
    assert_eq!(back, s);
    assert!(State::from_text("something else").is_err());
    assert!(State::from_text("termbonsai 1\nseed=1\n").is_err());
    let mut bad = s.to_text();
    bad = bad.replace(&format!("water={}", s.water), "water=7");
    assert!(State::from_text(&bad).is_err());
}

#[test]
fn names_cannot_carry_escape_sequences() {
    assert_eq!(clean_name("Mo\x1b[2Jmiji"), "Mo[2Jmiji");
    assert_eq!(clean_name("  "), "bonsai");
    assert_eq!(clean_name(&"x".repeat(100)).len(), 40);
    let s = State::plant(1, "a\nb", T0);
    // The name is cleaned on reading, so a hand-edited file is safe too.
    let text = s.to_text().replace("name=a\nb", "name=a\x07b");
    assert_eq!(State::from_text(&text).unwrap().name, "ab");
}

#[test]
fn months_and_seasons() {
    assert_eq!(month_of(0), 1);
    assert_eq!(month_of(1_704_067_200), 1); // 2024-01-01
    assert_eq!(month_of(1_709_164_800), 2); // 2024-02-29
    assert_eq!(month_of(1_719_792_000), 7); // 2024-07-01
    assert_eq!(month_of(T0), 10); // 2026-10-14
    assert_eq!(month_of(-DAY), 12); // 1969-12-31
    assert_eq!(Season::of_month(4, false), Season::Spring);
    assert_eq!(Season::of_month(10, false), Season::Autumn);
    assert_eq!(Season::of_month(10, true), Season::Spring);
    assert_eq!(Season::of_month(1, true), Season::Summer);
    assert_eq!(Season::of_month(7, true), Season::Winter);
}

#[test]
fn the_same_seed_grows_the_same_tree() {
    let (a, b) = (grow(5), grow(5));
    assert_eq!(a, b);
    assert_ne!(a, grow(6));
    for seed in 0..200 {
        let t = grow(seed);
        assert!(t
            .iter()
            .all(|p| p.y >= 0 && p.born >= 0.0 && p.born.is_finite()));
        assert!(t.iter().any(|p| p.kind == Kind::Leaf), "seed {seed}");
        let full = full_grown(&t);
        assert!((10.0..120.0).contains(&full), "seed {seed}: {full}");
        // Every leaf comes after the wood it grows on starts.
        let first_wood = t
            .iter()
            .filter(|p| p.kind != Kind::Leaf)
            .map(|p| p.born)
            .fold(f64::MAX, f64::min);
        assert!(t
            .iter()
            .filter(|p| p.kind == Kind::Leaf)
            .all(|p| p.born > first_wood));
    }
}

#[test]
fn state_files_that_do_not_fit_together_are_refused() {
    let s = State::plant(1, "x", T0).at(T0 + 100 * HOUR);
    let good = s.to_text();
    assert!(State::from_text(&good).is_ok());
    let dry = s.dry_since.unwrap();
    for (from, to) in [
        // A dry time in the future, or before planting.
        (
            format!("dry_since={dry}"),
            format!("dry_since={}", T0 + 1000 * HOUR),
        ),
        (
            format!("dry_since={dry}"),
            format!("dry_since={}", T0 - HOUR),
        ),
        // Dry soil without a dry time.
        (format!("dry_since={dry}\n"), String::new()),
        // Times far outside 1900-2400.
        (
            format!("planted_at={T0}"),
            format!("planted_at={}", i64::MIN),
        ),
        // More growth than days alive.
        (format!("growth={}", s.growth), "growth=500".to_string()),
        // A missing count.
        (format!("waterings={}", s.waterings), String::new()),
    ] {
        let text = good.replace(&from, &to);
        assert_ne!(text, good, "{from}");
        assert!(State::from_text(&text).is_err(), "{to}");
    }
    let twice = format!("{good}seed=9\n");
    assert!(State::from_text(&twice)
        .unwrap_err()
        .contains("appears twice"));
}

#[test]
fn a_tree_takes_about_a_month_to_fill_out() {
    let days: Vec<f64> = (0..300).map(|seed| full_grown(&grow(seed))).collect();
    assert!(days.iter().all(|d| (25.0..50.0).contains(d)), "{days:?}");
}
