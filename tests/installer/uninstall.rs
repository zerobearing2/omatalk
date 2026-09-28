//! uninstall.sh: what it removes, and what it keeps unless asked.

use crate::common::write;
use crate::fake::*;

#[test]
fn uninstall_on_eof_keeps_models_config_and_bind_and_finishes() {
    let fake = Fake::new();
    let models = fake.model(FP32);
    write(&models, "fake model");
    let config = fake.home(".config/omatalk/config.toml");
    write(&config, "voice = \"af_heart\"\n");
    let bindings = fake.seed_bindings();
    let before = bytes(&bindings);

    let result = fake.uninstall("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(models.is_file());
    assert!(config.is_file());
    assert_eq!(bytes(&bindings), before);
    assert!(
        result.stdout.contains("Omatalk uninstalled."),
        "{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Remove the o.bind line"),
        "{}",
        result.stdout
    );
}

#[test]
fn uninstall_removes_a_rust_install_and_its_models() {
    let mut fake = Fake::new();
    fake.set("FAKE_STRAY_STATUS", "0");
    write(fake.launcher(), launcher_script("installed"));
    write(fake.installed_unit(), unit());
    write(fake.model(FP32), "fake model");

    let result = fake.uninstall("y\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.launcher().exists());
    assert!(!fake.installed_unit().exists());
    assert!(!fake.install_home().exists());
    let commands = fake.commands();
    assert!(
        commands
            .iter()
            .any(|l| l == "systemctl --user disable --now omatalk.service")
    );
    let patterns: Vec<&str> = commands
        .iter()
        .filter(|l| l.starts_with("pkill"))
        .filter_map(|l| l.splitn(3, ' ').nth(2))
        .collect();
    let daemon_argv = format!("{} daemon", fake.launcher().display());
    let matches = |pattern: &&str| {
        fancy_regex::Regex::new(pattern)
            .unwrap()
            .is_match(&daemon_argv)
            .unwrap()
    };
    assert!(patterns.iter().any(matches), "{patterns:?}");
}
