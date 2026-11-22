use crate::common::{names, read};
use crate::fake::*;

fn changes(fake: &Fake) -> Vec<String> {
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
fn curl_install_migrates_to_the_package_and_keeps_user_files() {
    let fake = Fake::new();
    let (config, bindings) = fake.seed_curl_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);
    let model_before = bytes(fake.model(FP32));

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.launcher().exists());
    assert!(!fake.installed_unit().exists());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
    assert_eq!(bytes(fake.model(FP32)), model_before);
    assert_eq!(setup_calls(&fake), 1);
    let add = fake.last_index("omarchy pkg add omatalk-bin");
    let stop = fake.last_index("systemctl --user disable --now omatalk.service");
    let reload = fake.last_index("systemctl --user daemon-reload");
    assert!(add < stop && stop < reload, "{:?}", fake.commands());
    assert_eq!(fake.downloads(), Vec::<String>::new());
}

#[test]
fn python_leftovers_are_removed_and_models_stay() {
    let fake = Fake::new();
    let (config, bindings) = fake.seed_python_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(names(fake.install_home()), ["models"]);
    assert_eq!(names(fake.install_home().join("models")), [VOICES]);
    assert!(!fake.home(".local/bin/omatalkd").exists());
    assert!(!fake.launcher().exists());
    assert!(!fake.installed_unit().exists());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
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
    assert_eq!(changes(&fake), ["omarchy pkg add omatalk-bin"]);
    assert_eq!(setup_calls(&fake), 0);
}

#[test]
fn rerunning_on_a_migrated_home_only_adds_and_sets_up_again() {
    let fake = Fake::new();
    fake.seed_curl_install();
    assert_eq!(fake.install("").code, 0);
    let first = changes(&fake);

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        changes(&fake)[first.len()..],
        ["omarchy pkg add omatalk-bin"]
    );
    assert_eq!(setup_calls(&fake), 2);
}

#[test]
fn a_clean_home_just_installs_the_package_and_sets_up() {
    let fake = Fake::new();

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(changes(&fake), ["omarchy pkg add omatalk-bin"]);
    assert_eq!(setup_calls(&fake), 1);
    assert_eq!(names(fake.path("tmp")), Vec::<String>::new());
}

#[test]
fn a_daemon_that_will_not_stop_keeps_its_files() {
    let mut fake = Fake::new();
    fake.seed_curl_install();
    fake.set("FAKE_STOP_STATUS", "1");

    let result = fake.install("");

    assert_eq!(result.code, 1);
    assert_eq!(read(fake.launcher()), launcher_script("curl-era"));
    assert!(fake.installed_unit().is_file());
    assert_eq!(setup_calls(&fake), 0);
}

#[test]
fn a_missing_unit_is_tolerated() {
    let mut fake = Fake::new();
    fake.seed_curl_install();
    fake.set("FAKE_STOP_STATUS", "5");

    let result = fake.install("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.installed_unit().exists());
    assert_eq!(setup_calls(&fake), 1);
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
