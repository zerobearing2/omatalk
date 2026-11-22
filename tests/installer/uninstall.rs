//! uninstall.sh: what it removes, what it keeps unless asked, and the package
//! it drops last.

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
        result.stdout.contains("Omatalk files removed."),
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
    write(fake.installed_unit(), "[Service]\n");
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

#[test]
fn uninstall_ends_by_dropping_the_package_and_deletes_nothing_under_usr() {
    let fake = Fake::new();
    write(fake.model(FP32), "fake model");

    let result = fake.uninstall("y\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    let commands = fake.commands();
    assert_eq!(commands.last().unwrap(), "omarchy pkg drop omatalk-bin");
    assert!(
        commands
            .iter()
            .any(|l| l == "omarchy pkg present omatalk-bin"),
        "{commands:?}"
    );
    assert!(fake.path("usr/bin/omatalk").is_file());
}

#[test]
fn uninstall_without_the_package_has_nothing_to_drop() {
    let mut fake = Fake::new();
    write(fake.launcher(), launcher_script("curl-era"));
    fake.set("FAKE_PKG_ABSENT", "omatalk-bin");

    let result = fake.uninstall("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.launcher().exists());
    assert!(!fake.logged("omarchy pkg drop"));
    assert!(
        result.stdout.contains("nothing to drop"),
        "{}",
        result.stdout
    );
}

#[test]
fn uninstall_without_omarchy_names_the_pacman_command() {
    let mut fake = Fake::new();
    fake.without_omarchy();

    let result = fake.uninstall("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("sudo pacman -Rns omatalk-bin"),
        "{}",
        result.stdout
    );
}

/// `omatalk uninstall` runs the script as `bash -c <script>`, so its own argv
/// holds the whole text. A pattern given to `pkill -f` that matches that text
/// makes the script kill itself.
#[test]
fn uninstall_script_text_does_not_match_its_own_pkill_patterns() {
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/uninstall.sh"))
        .expect("uninstall.sh");
    let pkill = fancy_regex::Regex::new(r#"pkill -f "([^"]+)""#).unwrap();
    let patterns: Vec<String> = pkill
        .captures_iter(&script)
        .map(|c| c.unwrap().get(1).unwrap().as_str().to_owned())
        .collect();
    assert!(!patterns.is_empty());
    for pattern in patterns {
        let regex = fancy_regex::Regex::new(&pattern).unwrap();
        assert!(
            !regex.is_match(&script).unwrap(),
            "uninstall.sh text matches its own pkill pattern {pattern:?}"
        );
    }
}
