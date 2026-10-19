use termbonsai::draw::{ansi256, line, picture, status, ColorMode, Lang, Rgb, Season};
use termbonsai::state::{Health, State, DAY, HOUR};
use termbonsai::tree::{full_grown, grow};

const T0: i64 = 1_792_000_000;

fn count(lines: &[String], c: char) -> usize {
    lines.iter().map(|l| l.matches(c).count()).sum()
}

#[test]
fn a_seedling_and_a_grown_tree() {
    let parts = grow(7);
    let young = picture(
        &parts,
        0.0,
        Season::Summer,
        Health::Thriving,
        60,
        ColorMode::None,
    );
    assert_eq!(
        young.len(),
        2 + 2,
        "a sprout, the base, and the pot: {young:#?}"
    );
    assert!(young[0].contains("\\ /"));
    let old = picture(
        &parts,
        full_grown(&parts),
        Season::Summer,
        Health::Thriving,
        60,
        ColorMode::None,
    );
    assert!(old.len() > 8);
    assert!(count(&old, '&') > 50);
    for l in &old {
        assert!(l.chars().count() <= 60, "{l:?}");
        assert!(!l.contains('\x1b'));
    }
    assert!(old.last().unwrap().contains("\\___"));
}

#[test]
fn seasons_and_health_change_the_leaves() {
    let parts = grow(7);
    let full = full_grown(&parts);
    let pic = |season, health| picture(&parts, full, season, health, 60, ColorMode::None);
    let summer = count(&pic(Season::Summer, Health::Thriving), '&');
    assert!(
        count(&pic(Season::Spring, Health::Thriving), '*') > 0,
        "spring blossoms"
    );
    assert!(
        count(&pic(Season::Winter, Health::Thriving), '&') < summer * 2 / 3,
        "winter is sparse"
    );
    assert!(
        count(&pic(Season::Summer, Health::Wilting), '&') < summer * 2 / 3,
        "wilting drops leaves"
    );
    let dead = pic(Season::Summer, Health::Dead);
    assert_eq!(count(&dead, '&') + count(&dead, '*'), 0);
    assert!(count(&dead, '|') > 0, "the wood stays");
}

#[test]
fn colors() {
    let parts = grow(3);
    let tc = picture(
        &parts,
        50.0,
        Season::Autumn,
        Health::Thriving,
        60,
        ColorMode::TrueColor,
    );
    assert!(tc.iter().any(|l| l.contains("\x1b[38;2;")));
    assert!(tc
        .iter()
        .filter(|l| l.contains('\x1b'))
        .all(|l| l.ends_with("\x1b[0m")));
    let c256 = picture(
        &parts,
        50.0,
        Season::Autumn,
        Health::Thriving,
        60,
        ColorMode::Ansi256,
    );
    assert!(c256.iter().any(|l| l.contains("\x1b[38;5;")));
    assert_eq!(ansi256(Rgb(255, 0, 0)), 196);
    assert_eq!(ansi256(Rgb(128, 128, 128)), 244);
}

#[test]
fn narrow_pictures_stay_inside_their_width() {
    let parts = grow(11);
    for w in [24, 30, 45] {
        for l in picture(
            &parts,
            80.0,
            Season::Summer,
            Health::Thriving,
            w,
            ColorMode::None,
        ) {
            assert!(l.chars().count() <= w, "{w}: {l:?}");
        }
    }
}

#[test]
fn words() {
    let s = State::plant(7, "Momiji", T0).at(T0 + 3 * DAY + 2 * DAY);
    let full = full_grown(&grow(7));
    let en = status(&s, full, Lang::En);
    assert!(
        en.starts_with("Momiji, planted 5 days ago,") && en.contains("dry, not growing"),
        "{en}"
    );
    let ja = status(
        &State::plant(7, "もみじ", T0).at(T0 + 12 * HOUR),
        full,
        Lang::Ja,
    );
    assert!(
        ja.starts_with("もみじ(植えてから0日") && ja.contains("約60時間で土が乾きます"),
        "{ja}"
    );
    let dead = State::plant(7, "M", T0).at(T0 + 20 * DAY);
    assert!(status(&dead, full, Lang::En).contains("termbonsai plant starts a new one"));
    assert_eq!(
        line(&State::plant(7, "Momiji", T0), Lang::En, false),
        "Momiji 0d water 100% ok"
    );
    assert_eq!(
        line(&State::plant(7, "A#B", T0), Lang::En, true),
        "#[fg=green]A##B 0d water 100% ok#[default]"
    );
    assert_eq!(line(&dead, Lang::Ja, false), "M 20日 水0% 枯死");
}
