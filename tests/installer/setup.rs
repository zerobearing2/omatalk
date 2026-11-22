use crate::common::{names, read, write};
use crate::fake::*;

fn mutating_commands(fake: &Fake) -> Vec<String> {
    fake.commands()
        .into_iter()
        .filter(|l| {
            l.starts_with("curl ")
                || l.starts_with("systemctl --user daemon-reload")
                || l.starts_with("systemctl --user enable")
                || l.starts_with("systemctl --user restart")
                || l.starts_with("omarchy plugin enable")
                || l.starts_with("omarchy-shell")
        })
        .collect()
}

#[test]
fn fresh_setup_fetches_each_model_once_then_enables_and_starts_the_unit() {
    let fake = Fake::new();

    let result = fake.setup("n\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.model_requests(FP32), 1);
    assert_eq!(fake.model_requests(VOICES), 1);
    assert_eq!(read(fake.model(FP32)), "fake model");
    assert_eq!(names(fake.install_home()), ["models"]);
    assert!(!fake.home(".config/omatalk/config.toml").exists());
    assert!(!fake.logged("omarchy pkg"));
    let last_download = fake.last_index("curl ");
    let reload = fake.last_index("systemctl --user daemon-reload");
    let enable = fake.last_index("systemctl --user enable omatalk.service");
    let restart = fake.last_index("systemctl --user restart omatalk.service");
    assert!(
        last_download < reload && reload < enable && enable < restart,
        "{:?}",
        fake.commands()
    );
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
    assert!(read(fake.path("state/speech")).contains("speak Welcome to omatalk!"));
}

#[test]
fn setup_twice_changes_nothing_and_asks_nothing() {
    let fake = Fake::new();
    let config = fake.home(".config/omatalk/config.toml");
    write(&config, "voice = \"bf_emma\"\nspeed = 1.25\n");
    assert_eq!(fake.setup("y\n").code, 0);
    let before = mutating_commands(&fake);
    let config_before = bytes(&config);
    let bindings_before = bytes(fake.bindings());
    let model_before = bytes(fake.model(FP32));

    let second = fake.setup("");

    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(mutating_commands(&fake), before, "{:?}", fake.commands());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(fake.bindings()), bindings_before);
    assert_eq!(bytes(fake.model(FP32)), model_before);
    assert!(!second.stdout.contains("To bind F8"), "{}", second.stdout);
    assert_eq!(read(fake.bindings()), format!("\n{BIND_LINE}\n"));
}

#[test]
fn an_enabled_running_daemon_is_left_alone() {
    let fake = Fake::new();
    write(fake.path("state/enabled"), "");
    write(fake.path("state/ready"), "");

    let result = fake.setup("n\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.logged("systemctl --user daemon-reload"));
    assert!(!fake.logged("systemctl --user enable"));
    assert!(!fake.logged("systemctl --user restart"));
    assert!(!fake.logged("systemctl --user start"));
}

#[test]
fn corrupt_model_is_fetched_again() {
    let fake = Fake::new();
    assert_eq!(fake.setup("").code, 0);
    write(fake.model(FP32), "corrupt");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.model_requests(FP32), 2);
    assert_eq!(fake.model_requests(VOICES), 1);
    assert_eq!(read(fake.model(FP32)), "fake model");
}

#[test]
fn bad_model_download_fails_before_the_unit_is_touched() {
    let fake = Fake::new();
    write(fake.site("models").join(FP32), "bad download");

    let result = fake.setup("");

    assert_ne!(result.code, 0);
    assert!(!fake.logged("systemctl"));
    assert!(!fake.logged("omarchy"));
}

#[test]
fn setup_fails_if_daemon_never_becomes_ready() {
    let mut fake = Fake::new();
    fake.set("FAKE_DAEMON_DOWN", "1");

    let result = fake.setup("");

    assert_ne!(result.code, 0);
    assert!(
        result.stdout.contains("Daemon did not start"),
        "{}",
        result.stdout
    );
    assert!(!fake.logged("omarchy plugin"));
    assert!(!fake.plugin_dir().exists());
}

#[test]
fn failed_welcome_speak_warns_and_still_finishes_setup() {
    let mut fake = Fake::new();
    fake.set("FAKE_SPEAK_STATUS", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("Welcome speech failed"),
        "{}",
        result.stdout
    );
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn setup_without_the_package_says_how_to_install_it() {
    let mut fake = Fake::new();
    let missing = fake.path("usr/bin/missing");
    fake.set("OMATALK_BIN", missing);

    let result = fake.setup("");

    assert_eq!(result.code, 1);
    assert!(
        result.stdout.contains("omarchy pkg add omatalk-bin"),
        "{}",
        result.stdout
    );
    assert_eq!(fake.commands(), Vec::<String>::new());
}

#[test]
fn every_download_is_https_only_and_bounded() {
    let fake = Fake::new();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    let downloads = fake.downloads();
    let urls: Vec<&str> = downloads
        .iter()
        .map(|l| l.rsplit_once(' ').unwrap().1)
        .collect();
    assert_eq!(
        urls,
        [
            format!("{SITE_URL}/models/{FP32}"),
            format!("{SITE_URL}/models/{VOICES}"),
        ]
    );
    for line in &downloads {
        for flag in [
            "--proto =https",
            "--proto-redir =https",
            "--max-filesize ",
            "--max-time ",
            "--speed-time ",
        ] {
            assert!(line.contains(flag), "{flag:?} missing from {line:?}");
        }
    }
}
