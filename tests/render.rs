mod common;

use std::path::{Path, PathBuf};

use common::{TempDir, names, omatalk, read, run, write, write_voices_archive};

const WAV_HEADER: usize = 44;

/// A machine with a voices archive and the fake engine, and no Daemon: the
/// socket path is a file that does not exist.
struct Env {
    tmp: TempDir,
    env: Vec<(&'static str, PathBuf)>,
}

fn env() -> Env {
    let tmp = TempDir::new("render");
    let models = tmp.path().join("models");
    write_voices_archive(
        &models.join("voices-v1.0.bin"),
        &["af_heart", "af_bella", "am_test"],
    );
    let env = vec![
        ("OMATALK_CONFIG", tmp.path().join("config.toml")),
        ("OMATALK_MODELS", models),
        ("OMATALK_SOCKET", tmp.path().join("missing.sock")),
        ("OMATALK_TEST_FAKE_ENGINE", "1".into()),
    ];
    Env { tmp, env }
}

impl Env {
    fn out(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn render(&self, args: &[&str], stdin: &str) -> common::Run {
        run(omatalk(["render"]).args(args).envs(self.env.clone()), stdin)
    }
}

fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Checks the 44-byte header of 16-bit mono 24 kHz PCM against the file's
/// length, and returns the sample bytes.
fn pcm(wav: &[u8]) -> &[u8] {
    let data = &wav[WAV_HEADER..];
    let u32_at = |at: usize| u32::from_le_bytes(wav[at..at + 4].try_into().unwrap());
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(u32_at(4) as usize, wav.len() - 8);
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(16), 16);
    assert_eq!(wav[20..24], [1, 0, 1, 0], "PCM, mono");
    assert_eq!(u32_at(24), 24_000);
    assert_eq!(u32_at(28), 48_000);
    assert_eq!(wav[32..36], [2, 0, 16, 0]);
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32_at(40) as usize, data.len());
    data
}

#[test]
fn renders_words_to_a_wav_without_a_daemon() {
    let env = env();
    let out = env.out("hello.wav");

    let result = env.render(
        &["--out", out.to_str().unwrap(), "Hello", "there.", "Bye."],
        "",
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.stderr, "");
    assert_eq!(
        result.stdout,
        format!("{} 0.2 s\n", out.display()),
        "two sentences are 0.2 s of fake audio"
    );
    assert_eq!(pcm(&bytes(&out)).len(), 9600);
    assert_eq!(names(env.tmp.path()), ["hello.wav", "models"]);
}

#[test]
fn a_render_asks_the_engine_for_a_voiceover() {
    let env = env();
    let out = env.out("delivery.wav");

    let result = env.render(&["--out", out.to_str().unwrap(), "Hi."], "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    let wav = bytes(&out);
    assert_eq!(
        pcm(&wav),
        8191i16.to_le_bytes().repeat(2400),
        "the fake engine is silent unless the Delivery is a Voiceover"
    );
}

#[test]
fn reads_the_script_from_stdin_when_no_words_are_given() {
    let env = env();
    let out = env.out("piped.wav");

    let result = env.render(
        &["--out", out.to_str().unwrap()],
        "First.\n\nSecond.\nThird.\n",
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(pcm(&bytes(&out)).len(), 3 * 4800);
}

#[test]
fn a_relative_path_is_the_callers_not_the_repos() {
    let env = env();
    let caller = env.out("caller");
    std::fs::create_dir(&caller).unwrap();

    let result = run(
        omatalk(["render", "--out", "rel.wav", "Hi."])
            .envs(env.env.clone())
            .current_dir(&caller),
        "",
    );

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "rel.wav 0.1 s\n");
    pcm(&bytes(&caller.join("rel.wav")));
    assert!(!common::repo("rel.wav").exists());
}

#[test]
fn rendering_over_an_existing_file_replaces_it() {
    let env = env();
    let out = env.out("again.wav");
    write(&out, "old");

    let result = env.render(&["--out", out.to_str().unwrap(), "One. Two. Three."], "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(pcm(&bytes(&out)).len(), 3 * 4800);
    assert_eq!(names(env.tmp.path()), ["again.wav", "models"]);
}

#[test]
fn a_failed_render_keeps_the_existing_target_and_leaves_no_temp() {
    let env = env();
    let out = env.out("keep.wav");
    write(&out, "precious");
    let before = names(env.tmp.path());

    let unknown = env.render(
        &["--out", out.to_str().unwrap(), "--voice", "af_nope", "Hi."],
        "",
    );
    let bad_speed = env.render(
        &["--out", out.to_str().unwrap(), "--speed", "fast", "Hi."],
        "",
    );

    assert_eq!(unknown.code, 1);
    assert_eq!(unknown.stderr, "af_nope: not a known voice\n");
    assert_eq!(bad_speed.code, 1);
    assert_eq!(bad_speed.stderr, "fast: speed must be a number\n");
    assert_eq!(read(&out), "precious");
    assert_eq!(names(env.tmp.path()), before);
}

#[test]
fn a_write_failure_reports_the_path_and_removes_the_temp() {
    let env = env();
    let nowhere = env.out("no/such/dir/out.wav");
    let target_is_a_dir = env.out("dir.wav");
    std::fs::create_dir(&target_is_a_dir).unwrap();
    let before = names(env.tmp.path());

    let missing = env.render(&["--out", nowhere.to_str().unwrap(), "Hi."], "");
    let blocked = env.render(&["--out", target_is_a_dir.to_str().unwrap(), "Hi."], "");

    for (result, path) in [(missing, &nowhere), (blocked, &target_is_a_dir)] {
        assert_eq!(result.code, 1, "{}", path.display());
        assert!(
            result.stderr.starts_with(&format!("{}: ", path.display())),
            "{}",
            result.stderr
        );
        assert_eq!(result.stderr.lines().count(), 1);
        assert_eq!(result.stdout, "");
    }
    assert_eq!(names(env.tmp.path()), before);
    assert!(names(&target_is_a_dir).is_empty());
}

#[test]
fn only_wav_is_written_and_the_model_is_never_loaded_for_anything_else() {
    let tmp = TempDir::new("render");
    let out = tmp.path().join("voice.mp3");

    // No fake engine and no models: loading the real engine would fail with a
    // different message.
    let result = run(
        omatalk(["render", "--out", out.to_str().unwrap(), "Hi."])
            .env("OMATALK_MODELS", tmp.path().join("models"))
            .env("OMATALK_CONFIG", tmp.path().join("config.toml")),
        "",
    );

    assert_eq!(result.code, 1);
    assert_eq!(result.stderr.lines().count(), 1, "{}", result.stderr);
    assert!(result.stderr.contains("ffmpeg"), "{}", result.stderr);
    assert!(names(tmp.path()).is_empty());
}

#[test]
fn the_wav_extension_is_case_insensitive() {
    let env = env();
    let out = env.out("LOUD.WAV");

    let result = env.render(&["--out", out.to_str().unwrap(), "Hi."], "");

    assert_eq!(result.code, 0, "{}", result.stderr);
    pcm(&bytes(&out));
}

#[test]
fn blank_text_is_nothing_to_read() {
    let env = env();
    let out = env.out("blank.wav");
    let out = out.to_str().unwrap();

    let words = env.render(&["--out", out, " ", "\n"], "");
    let piped = env.render(&["--out", out], " \n\n\t\n");

    for result in [words, piped] {
        assert_eq!(result.code, 1);
        assert_eq!(result.stderr, "nothing to read\n");
        assert_eq!(result.stdout, "");
    }
    assert!(!Path::new(out).exists());
}

#[test]
fn a_missing_out_or_an_unknown_flag_is_usage() {
    let env = env();
    let out = env.out("x.wav");

    let no_out = env.render(&["Hi."], "");
    let unknown = env.render(&["--out", out.to_str().unwrap(), "--loud", "Hi."], "");

    for result in [no_out, unknown] {
        assert_eq!(result.code, 2);
        assert!(
            result.stderr.starts_with("usage: omatalk "),
            "{}",
            result.stderr
        );
        assert!(result.stderr.contains("render --out PATH"));
    }
    assert!(!out.exists());
}

#[test]
fn the_default_voice_comes_from_the_config_and_is_checked_too() {
    let env = env();
    let out = env.out("cfg.wav");
    write(env.out("config.toml"), "voice = \"am_gone\"\n");

    let result = env.render(&["--out", out.to_str().unwrap(), "Hi."], "");

    assert_eq!(result.code, 1);
    assert_eq!(result.stderr, "am_gone: not a known voice\n");
    assert!(!out.exists());

    write(env.out("config.toml"), "voice = \"am_test\"\n");
    let result = env.render(&["--out", out.to_str().unwrap(), "Hi."], "");
    assert_eq!(result.code, 0, "{}", result.stderr);
}
