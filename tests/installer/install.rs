use crate::common::{names, read};
use crate::fake::*;

fn pkg_and_systemctl_commands(fake: &Fake) -> Vec<String> {
    fake.commands()
        .into_iter()
        .filter(|l| l.starts_with("omarchy pkg") || l.starts_with("systemctl"))
        .collect()
}

fn setup_calls(fake: &Fake) -> usize {
    let calls = fake.path("state/setup-calls");
    if calls.exists() {
        read(calls).lines().count()
    } else {
        0
    }
}

#[test]
fn the_shim_installs_the_package_then_runs_setup() {
    let fake = Fake::new();
    fake.seed_curl_install();

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        pkg_and_systemctl_commands(&fake),
        ["omarchy pkg add omatalk-bin"]
    );
    assert_eq!(setup_calls(&fake), 1);
}

#[test]
fn failed_pkg_add_changes_nothing() {
    let mut fake = Fake::new();
    let (config, bindings) = fake.seed_curl_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);
    fake.set("FAKE_PKG_ADD_FAIL", "1");

    let result = fake.install("");

    assert_ne!(result.code, 0);
    assert!(
        result.stdout.contains("nothing was changed"),
        "{}",
        result.stdout
    );
    assert_eq!(read(fake.launcher()), launcher_script("curl-era"));
    assert!(fake.installed_unit().is_file());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
    assert!(fake.model(FP32).is_file());
    assert_eq!(
        pkg_and_systemctl_commands(&fake),
        ["omarchy pkg add omatalk-bin"]
    );
    assert_eq!(setup_calls(&fake), 0);
}

#[test]
fn a_clean_home_just_installs_the_package_and_sets_up() {
    let fake = Fake::new();

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        pkg_and_systemctl_commands(&fake),
        ["omarchy pkg add omatalk-bin"]
    );
    assert_eq!(setup_calls(&fake), 1);
    assert_eq!(names(fake.path("tmp")), Vec::<String>::new());
}

#[test]
fn installer_requires_omarchy() {
    let mut fake = Fake::new();
    fake.seed_curl_install();
    fake.without_omarchy();

    let result = fake.install("");

    assert_ne!(result.code, 0);
    assert!(
        result.stdout.contains("requires Omarchy"),
        "{}",
        result.stdout
    );
    assert_eq!(fake.commands(), Vec::<String>::new());
    assert!(fake.launcher().is_file());
    assert_eq!(setup_calls(&fake), 0);
}
