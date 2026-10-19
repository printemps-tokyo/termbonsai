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
    let leftovers: Vec<_> = std::fs::read_dir(&g.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "no temporary or lock files left: {leftovers:?}"
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
