use std::process::Command;

const T0: i64 = 1_792_000_000;

struct Garden {
    dir: std::path::PathBuf,
}

impl Garden {
    fn new(name: &str) -> Garden {
        let dir = std::env::temp_dir().join(format!("termbonsai-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Garden { dir }
    }

    fn run(&self, at: i64, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_termbonsai"))
            .args(args)
            .env("TERMBONSAI_STATE", self.dir.join("bonsai"))
            .env("TERMBONSAI_NOW", at.to_string())
            .env("LANG", "C")
            .env_remove("LC_ALL")
            .env_remove("XDG_STATE_HOME")
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    }
}

impl Drop for Garden {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_life() {
    let g = Garden::new("life");
    let (code, out, _) = g.run(T0, &["plant", "--seed", "7", "--name", "Momiji"]);
    assert_eq!(code, 0);
    assert_eq!(out, "Momiji is planted. Water it every couple of days.\n");
    let (_, out, _) = g.run(T0 + 2 * 86_400, &[]);
    assert!(
        out.contains("Momiji, planted 2 days ago") && out.contains("growing"),
        "{out}"
    );
    assert!(!out.contains('\x1b'), "no color when not a terminal");
    let (_, out, _) = g.run(T0 + 2 * 86_400, &["water"]);
    assert_eq!(out, "You water Momiji. The soil is wet again.\n");
    let (_, out, _) = g.run(T0 + 2 * 86_400 + 3_600, &["water"]);
    assert!(out.contains("still moist"));
    let (_, line, _) = g.run(T0 + 3 * 86_400, &["line"]);
    assert_eq!(line, "Momiji 3d water 67% ok\n");
    let (_, json, _) = g.run(T0 + 30 * 86_400, &["status"]);
    assert!(
        json.contains("\"name\":\"Momiji\",\"seed\":7") && json.contains("\"health\":\"dead\""),
        "{json}"
    );
    // A living tree is not replaced without --force; a dead one is.
    let (code, _, err) = g.run(T0 + 3 * 86_400, &["plant"]);
    assert_eq!(code, 2);
    assert!(err.contains("still alive"), "{err}");
    assert_eq!(g.run(T0 + 30 * 86_400, &["plant", "--seed", "8"]).0, 0);
}

#[test]
fn the_first_look_plants_a_tree() {
    let g = Garden::new("first");
    let (code, out, _) = g.run(T0, &[]);
    assert_eq!(code, 0);
    assert!(out.starts_with("A new bonsai, "), "{out}");
    assert!(g.dir.join("bonsai").exists());
    let (_, line, _) = Garden::new("first-line").run(T0, &["line"]);
    assert!(line.ends_with(" 0d water 100% ok\n"), "{line}");
}

#[test]
fn previews_save_nothing() {
    let g = Garden::new("preview");
    g.run(T0, &["plant", "--seed", "3"]);
    let before = std::fs::read_to_string(g.dir.join("bonsai")).unwrap();
    let (_, out, _) = g.run(T0, &["--at", "+60d", "--watered", "--season", "autumn"]);
    assert!(
        out.contains("100% grown") && out.contains("nothing was saved"),
        "{out}"
    );
    let (_, dry, _) = g.run(T0, &["--at", "+60d"]);
    assert!(dry.contains("dead"), "unwatered for 60 days: {dry}");
    assert_eq!(
        std::fs::read_to_string(g.dir.join("bonsai")).unwrap(),
        before
    );
}

#[test]
fn grow_replays_without_a_terminal() {
    let g = Garden::new("grow");
    g.run(T0, &["plant", "--seed", "3"]);
    let (code, out, _) = g.run(T0 + 2 * 86_400, &["grow"]);
    assert_eq!(code, 0);
    assert_eq!(
        out.matches(":===").count(),
        1,
        "one frame when not a terminal"
    );
}

#[test]
fn japanese() {
    let g = Garden::new("ja");
    let out = Command::new(env!("CARGO_BIN_EXE_termbonsai"))
        .args(["plant", "--seed", "2"])
        .env("TERMBONSAI_STATE", g.dir.join("bonsai"))
        .env("TERMBONSAI_NOW", T0.to_string())
        .env("LANG", "ja_JP.UTF-8")
        .env_remove("LC_ALL")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("を植えました。"), "{text}");
}

#[test]
fn usage_errors_exit_2() {
    let g = Garden::new("errors");
    for args in [
        &["prune"][..],
        &["--bogus"][..],
        &["water", "--seed", "3"][..],
        &["show", "--tmux"][..],
        &["--watered"][..],
        &["water", "--at", "+1d"][..],
        &["--at", "tomorrow"][..],
        &["--at", "+99999999d"][..],
        &["--width", "10"][..],
        &["--season", "monsoon"][..],
        &["--color", "maybe"][..],
        &["--force=yes"][..],
    ] {
        let (code, _, err) = g.run(T0, args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.starts_with("termbonsai: "), "{err}");
    }
    std::fs::write(g.dir.join("bonsai"), "garbage").unwrap();
    let (code, _, err) = g.run(T0, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("plant --force starts over"), "{err}");
}

#[test]
fn a_corrupt_state_can_be_replaced() {
    let g = Garden::new("corrupt");
    std::fs::write(g.dir.join("bonsai"), "garbage").unwrap();
    assert_eq!(g.run(T0, &["plant"]).0, 2, "not without --force");
    let (code, out, err) = g.run(T0, &["plant", "--force", "--seed", "4", "--name", "Ume"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("Ume is planted."));
    std::fs::write(g.dir.join("bonsai"), [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(
        g.run(T0, &["plant", "--force"]).0,
        0,
        "even when it is not text"
    );
}

#[test]
fn previews_of_a_tree_not_yet_planted_save_nothing() {
    let g = Garden::new("noplant");
    let (code, out, _) = g.run(T0, &["--at", "+10d", "--watered"]);
    assert_eq!(code, 0);
    assert!(out.contains("not yet planted; nothing was saved"), "{out}");
    assert!(!g.dir.join("bonsai").exists());
    assert_eq!(g.run(T0, &["water"]).0, 2, "nothing to water yet");
}

#[test]
fn bad_offsets_and_times_are_usage_errors() {
    let g = Garden::new("times");
    for at in [
        "-2d",
        "日",
        "+9223372036854775807h",
        "-9223372036854775808h",
        "+36501d",
        "d",
    ] {
        let (code, _, err) = g.run(T0, &["--at", at]);
        assert_eq!(code, 2, "{at}: {err}");
    }
    let (code, _, err) = g.run(i64::MIN / 2, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("1900 to 2400"), "{err}");
}

#[test]
fn paths_in_errors_are_escaped() {
    let g = Garden::new("escape");
    let evil = g.dir.join("a\x1b[2Jb").join("x").join("bonsai");
    std::fs::write(g.dir.join("a\x1b[2Jb"), "a file, not a directory").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_termbonsai"))
        .args(["plant", "--state", evil.to_str().unwrap()])
        .env("TERMBONSAI_NOW", T0.to_string())
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!err.contains('\x1b'), "{err:?}");
}

#[test]
fn many_waterings_at_once_leave_a_good_file() {
    let g = Garden::new("race");
    g.run(T0, &["plant", "--seed", "5"]);
    // All started before any is waited for, so they contend for the lock.
    let children: Vec<_> = (0..8)
        .map(|i| {
            Command::new(env!("CARGO_BIN_EXE_termbonsai"))
                .args(["water"])
                .env("TERMBONSAI_STATE", g.dir.join("bonsai"))
                .env("TERMBONSAI_NOW", (T0 + 50 * 3_600 + i).to_string())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success());
    }
    let (_, json, _) = g.run(T0 + 50 * 3_600 + 10, &["status"]);
    assert!(
        json.contains("\"waterings\":1"),
        "one watering counts, the rest find moist soil: {json}"
    );
    let mut left: Vec<String> = std::fs::read_dir(&g.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec!["bonsai", "bonsai.lock"],
        "no temporary files left"
    );
}

#[test]
fn a_state_file_named_like_a_lock_is_safe() {
    let g = Garden::new("lockname");
    let path = g.dir.join("bonsai.lock");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_termbonsai"))
            .args(args)
            .args(["--state", path.to_str().unwrap()])
            .env("TERMBONSAI_NOW", T0.to_string())
            .output()
            .unwrap()
    };
    assert!(run(&["plant", "--seed", "1"]).status.success());
    assert!(path.exists(), "the state file survives its own save");
    assert!(run(&["plant", "--force", "--seed", "2"]).status.success());
    assert!(std::fs::read_to_string(&path).unwrap().contains("seed=2"));
}

#[cfg(unix)]
#[test]
fn force_replaces_a_file_that_cannot_be_read() {
    use std::os::unix::fs::PermissionsExt;
    let g = Garden::new("unreadable");
    let path = g.dir.join("bonsai");
    std::fs::write(&path, "x").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&path).is_ok() {
        return; // running as root: permissions do not stop reading
    }
    assert_eq!(g.run(T0, &["plant"]).0, 2);
    assert_eq!(g.run(T0, &["plant", "--force", "--seed", "3"]).0, 0);
    assert!(std::fs::read_to_string(&path).unwrap().contains("seed=3"));
}

const DAY: i64 = 86_400;

#[test]
fn pots_species_and_watering_them_all() {
    let g = Garden::new("pots");
    let (c, out, err) = g.run(T0, &["plant", "--seed", "7"]);
    assert_eq!(c, 0, "{err}");
    assert!(out.contains("Keyaki is planted"), "{out}");
    let (c, out, _) = g.run(
        T0,
        &["plant", "--pot", "pine", "--species", "pine", "--seed", "3"],
    );
    assert_eq!(c, 0);
    assert!(out.contains("Matsu is planted"), "{out}");
    let (c, _, _) = g.run(T0, &["plant", "--pot", "cherry", "--species", "cherry"]);
    assert_eq!(c, 0);
    assert!(g.dir.join("bonsai@pine").exists());
    let (c, out, _) = g.run(T0 + DAY, &["pots"]);
    assert_eq!(c, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(
        lines[0].starts_with("(first)") && lines[0].contains("Keyaki"),
        "{out}"
    );
    assert!(
        lines[1].starts_with("cherry") && lines[2].starts_with("pine"),
        "{out}"
    );

    let (c, out, _) = g.run(T0 + 2 * DAY, &["water", "--all"]);
    assert_eq!(c, 0);
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(out.contains("[pine] You water Matsu"), "{out}");
    let (_, status, _) = g.run(T0 + 2 * DAY, &["status", "--pot", "pine"]);
    assert!(status.contains("\"species\":\"pine\""), "{status}");
    assert!(status.contains("\"waterings\":1"), "{status}");
    let (_, show, _) = g.run(
        T0 + 2 * DAY,
        &[
            "--pot",
            "pine",
            "--at",
            "+60d",
            "--watered",
            "--color",
            "never",
        ],
    );
    assert!(
        show.contains('^') && show.contains("Matsu (pine)"),
        "{show}"
    );
}

#[test]
fn pruning_a_grown_tree() {
    let g = Garden::new("prune");
    g.run(T0, &["plant", "--seed", "11"]);
    // Water it every other day for 40 days.
    for d in (2..=40).step_by(2) {
        g.run(T0 + d * DAY, &["water"]);
    }
    let at = T0 + 40 * DAY;
    let width = |s: &str| {
        s.lines()
            .take_while(|l| !l.contains(":==="))
            .map(|l| {
                let t = l.trim_end();
                (t.len() - t.trim_start().len(), t.len())
            })
            .fold((usize::MAX, 0), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)))
    };
    let (_, before, _) = g.run(at, &["--color", "never", "--season", "summer"]);
    let (c, out, _) = g.run(at, &["prune"]);
    assert_eq!(c, 0, "{out}");
    assert!(out.contains("You prune"), "{out}");
    let (_, after, _) = g.run(at, &["--color", "never", "--season", "summer"]);
    let (b, a) = (width(&before), width(&after));
    assert!(
        a.1 - a.0 < b.1 - b.0,
        "narrower after pruning: {before}\n---\n{after}"
    );
    assert!(after.contains("Pruned 1 time."), "{after}");

    // Once a day.
    let (c, out, _) = g.run(at + 3_600, &["prune"]);
    assert_eq!(c, 1);
    assert!(out.contains("23 hours"), "{out}");
    // Not a dry tree.
    let (c, out, _) = g.run(at + 6 * DAY, &["prune"]);
    assert_eq!(c, 1);
    assert!(out.contains("too dry"), "{out}");
    let text = std::fs::read_to_string(g.dir.join("bonsai")).unwrap();
    assert!(
        text.contains("trim_x=") && text.contains("prunings=1"),
        "{text}"
    );
}

#[test]
fn pot_and_species_usage_errors() {
    let g = Garden::new("potusage");
    for args in [
        &["--pot", "../x"][..],
        &["--pot", ""][..],
        &["plant", "--species", "oak"][..],
        &["water", "--species", "pine"][..],
        &["show", "--all"][..],
        &["water", "--all", "--pot", "a"][..],
        &["pots", "--pot", "a"][..],
        &["prune", "--at", "+1d"][..],
    ] {
        let (c, _, err) = g.run(T0, args);
        assert_eq!(c, 2, "{args:?}: {err}");
    }
    let (c, out, _) = g.run(T0, &["pots"]);
    assert_eq!(c, 0);
    assert!(out.contains("No pots yet"), "{out}");
}

#[test]
fn random_species_vary_and_pots_skip_links() {
    let g = Garden::new("random");
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..40u64 {
        g.run(
            T0,
            &[
                "plant",
                "--force",
                "--species",
                "random",
                "--seed",
                &seed.to_string(),
            ],
        );
        let (_, status, _) = g.run(T0, &["status"]);
        let sp = status
            .split("\"species\":\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string();
        seen.insert(sp);
    }
    assert_eq!(seen.len(), 4, "{seen:?}");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(g.dir.join("bonsai"), g.dir.join("bonsai@alias")).unwrap();
        std::fs::create_dir(g.dir.join("bonsai@dir")).unwrap();
        let (c, out, _) = g.run(T0 + 2 * DAY, &["pots"]);
        assert_eq!(c, 0);
        assert_eq!(out.lines().count(), 1, "{out}");
        let (c, out, _) = g.run(T0 + 2 * DAY, &["water", "--all"]);
        assert_eq!(c, 0);
        assert_eq!(out.lines().count(), 1, "{out}");
    }
    // A pot that cannot be read fails the run after the others are watered.
    std::fs::write(g.dir.join("bonsai@broken"), "nonsense").unwrap();
    let (c, out, err) = g.run(T0 + 3 * DAY, &["water", "--all"]);
    assert_eq!(c, 2, "{out}{err}");
    assert!(err.contains("[broken]"), "{err}");
    assert_eq!(out.lines().count(), 1, "{out}");
}
