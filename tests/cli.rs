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
