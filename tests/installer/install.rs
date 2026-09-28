//! install.sh: the binary, unit, models, and upgrades from earlier installs.

use crate::common::{names, read, write};
use crate::fake::*;

const PKG_DEPS: &str = "curl pipewire wl-clipboard onnxruntime-cpu espeak-ng libnotify";

#[test]
fn fresh_install_writes_the_release_and_fetches_each_model_once() {
    let fake = Fake::new();
    fake.publish(release("one"));

    let result = fake.install("n\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(fake.launcher()), launcher_script("one"));
    assert_eq!(bytes(fake.installed_unit()), unit());
    assert!(!fake.home(".config/omatalk/config.toml").exists());
    assert_eq!(fake.model_requests(FP32), 1);
    assert_eq!(fake.model_requests(VOICES), 1);
    assert_eq!(names(fake.install_home()), ["models"]);
    assert_eq!(names(fake.home(".local/bin")), ["omatalk"]);
}

#[test]
fn reinstall_replaces_the_binary_and_keeps_user_files() {
    let fake = Fake::new();
    fake.publish(release("one"));
    assert_eq!(fake.install("n\n").code, 0);
    let config = fake.home(".config/omatalk/config.toml");
    write(&config, "voice = \"bf_emma\"\nspeed = 1.25\n");
    write(fake.bindings(), format!("{BIND_LINE}\n"));
    let config_before = bytes(&config);
    fake.publish(release("two"));

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(fake.launcher()), launcher_script("two"));
    assert_eq!(bytes(&config), config_before);
    assert_eq!(read(fake.bindings()), format!("{BIND_LINE}\n"));
    assert!(!result.stdout.contains("To bind F8"), "{}", result.stdout);
    assert_eq!(fake.model_requests(FP32), 1);
    assert_eq!(fake.model_requests(VOICES), 1);
    assert_eq!(names(fake.install_home()), ["models"]);
    assert_eq!(names(fake.home(".local/bin")), ["omatalk"]);
    let adds: Vec<String> = fake
        .commands()
        .into_iter()
        .filter(|l| l.contains("omarchy plugin add"))
        .collect();
    assert_eq!(adds, [PLUGIN_ADD_CMD]);
    assert!(!fake.logged("omarchy plugin remove"));
}

#[test]
fn corrupt_model_is_fetched_again() {
    let fake = Fake::new();
    fake.publish(release("one"));
    assert_eq!(fake.install("").code, 0);
    write(fake.model(FP32), "corrupt");

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fake.model_requests(FP32), 2);
    assert_eq!(fake.model_requests(VOICES), 1);
    assert_eq!(read(fake.model(FP32)), "fake model");
}

#[test]
fn bad_model_download_fails_and_keeps_the_installed_binary() {
    let fake = Fake::new();
    fake.publish(release("one"));
    assert_eq!(fake.install("").code, 0);
    fake.publish(release("two"));
    write(fake.site("models").join(FP32), "bad download");
    std::fs::remove_file(fake.model(FP32)).unwrap();

    let result = fake.install("");

    assert_ne!(result.code, 0);
    assert_eq!(read(fake.launcher()), launcher_script("one"));
    assert_eq!(names(fake.home(".local/bin")), ["omatalk"]);
}

#[test]
fn downloads_finish_before_the_daemon_stops() {
    let fake = Fake::new();
    fake.publish(release("new"));

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    let stop = fake.last_index("systemctl --user stop omatalk.service");
    let reload = fake.last_index("systemctl --user daemon-reload");
    let start = fake.last_index("systemctl --user enable --now omatalk.service");
    let last_download = fake.last_index("curl ");
    assert!(
        last_download < stop && stop < reload && reload < start,
        "{:?}",
        fake.commands()
    );
}

#[test]
fn python_install_is_migrated_in_place() {
    let fake = Fake::new();
    fake.publish(release("new"));
    let (config, bindings) = fake.seed_python_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);
    let running = fake.path("running-launcher");
    std::fs::hard_link(fake.launcher(), &running).unwrap();

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(names(fake.install_home()), ["models"]);
    assert_eq!(names(fake.install_home().join("models")), [FP32, VOICES]);
    assert!(!fake.home(".local/bin/omatalkd").exists());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
    assert_eq!(bytes(fake.installed_unit()), unit());
    assert!(!fake.launcher().is_symlink());
    assert_eq!(read(fake.launcher()), launcher_script("new"));
    // Renamed over, not rewritten in place: the old inode is untouched.
    assert_eq!(read(&running), "#!/venv/bin/python\n");
    assert_eq!(fake.model_requests(FP32), 1);
    assert_eq!(fake.model_requests(VOICES), 0);
}

#[test]
fn symlinked_launcher_becomes_a_regular_file() {
    let fake = Fake::new();
    fake.publish(release("new"));
    let target = fake.path("checkout-build");
    write(&target, "dev build\n");
    std::fs::create_dir_all(fake.launcher().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, fake.launcher()).unwrap();

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.launcher().is_symlink());
    assert_eq!(read(fake.launcher()), launcher_script("new"));
    assert_eq!(read(&target), "dev build\n");
}

#[test]
fn system_dependencies_are_the_native_runtime() {
    let present = format!("omarchy pkg present {PKG_DEPS}");
    let add = format!("omarchy pkg add {PKG_DEPS}");
    for (status, want) in [("0", vec![present.as_str()]), ("1", vec![&present, &add])] {
        let mut fake = Fake::new();
        fake.publish(release("new"));
        fake.set("FAKE_PKG_PRESENT_STATUS", status);

        let result = fake.install("");

        assert_eq!(result.code, 0, "{status}: {}", result.stderr);
        let pkg: Vec<String> = fake
            .commands()
            .into_iter()
            .filter(|l| l.starts_with("omarchy pkg"))
            .collect();
        assert_eq!(pkg, want, "present status {status}");
    }
}

#[test]
fn installer_requires_omarchy() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    fake.without_omarchy();

    let result = fake.install("");

    assert_ne!(result.code, 0);
    assert!(
        result.stdout.contains("requires Omarchy"),
        "{}",
        result.stdout
    );
    assert_eq!(fake.commands(), Vec::<String>::new());
    assert!(!fake.launcher().exists());
}

#[test]
fn installer_tolerates_missing_unit() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    fake.set("FAKE_STOP_STATUS", "5");

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
}

#[test]
fn installer_rejects_tarball_checksum_mismatch() {
    let fake = Fake::new();
    fake.publish(release("new"));
    fake.seed_python_install();

    let result = fake.install_with_sha("", &"0".repeat(64));

    assert_ne!(result.code, 0);
    assert_eq!(read(fake.launcher()), "#!/venv/bin/python\n");
    assert!(!fake.logged("systemctl"));
    assert_eq!(names(fake.install_home()), ["models", "src", "venv"]);
}

#[test]
fn installer_does_not_replace_files_if_daemon_will_not_stop() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    fake.seed_python_install();
    fake.set("FAKE_STOP_STATUS", "1");

    let result = fake.install("");

    assert_eq!(result.code, 1);
    assert_eq!(read(fake.launcher()), "#!/venv/bin/python\n");
    assert!(fake.install_home().join("venv/bin/omatalk").is_file());
    assert!(fake.model(FP16).is_file());
    assert!(read(fake.installed_unit()).contains("venv"));
}

#[test]
fn installer_fails_if_daemon_never_becomes_ready() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    fake.set("FAKE_DAEMON_DOWN", "1");

    let result = fake.install("");

    assert_ne!(result.code, 0);
    assert!(
        result.stdout.contains("Daemon did not start"),
        "{}",
        result.stdout
    );
}

#[test]
fn failed_welcome_speak_warns_and_still_finishes_the_install() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    fake.set("FAKE_SPEAK_STATUS", "1");

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("Welcome speech failed"),
        "{}",
        result.stdout
    );
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn every_download_is_https_only_and_bounded() {
    let fake = Fake::new();
    fake.publish(release("new"));

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    let downloads = fake.downloads();
    let urls: Vec<&str> = downloads
        .iter()
        .map(|l| l.rsplit_once(' ').unwrap().1)
        .collect();
    assert_eq!(
        urls,
        [
            format!("{SITE_URL}/{TARBALL}"),
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
