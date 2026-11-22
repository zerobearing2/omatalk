use crate::common::{names, read, write};
use crate::fake::*;

#[test]
fn setup_migrates_a_curl_era_install_and_keeps_user_files() {
    let fake = Fake::new();
    let (config, bindings) = fake.seed_curl_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);
    let model_before = bytes(fake.model(FP32));

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.launcher().exists());
    assert!(!fake.installed_unit().exists());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
    assert_eq!(bytes(fake.model(FP32)), model_before);
    let stop = fake.last_index("systemctl --user disable --now omatalk.service");
    let enable = fake.last_index("systemctl --user enable omatalk.service");
    assert!(stop < enable, "{:?}", fake.commands());
}

#[test]
fn python_leftovers_are_removed_and_models_stay() {
    let fake = Fake::new();
    let (config, bindings) = fake.seed_python_install();
    let config_before = bytes(&config);
    let bindings_before = bytes(&bindings);

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(names(fake.install_home()), ["models"]);
    assert_eq!(names(fake.install_home().join("models")), [FP32, VOICES]);
    assert!(!fake.home(".local/bin/omatalkd").exists());
    assert!(!fake.launcher().exists());
    assert!(!fake.installed_unit().exists());
    assert_eq!(bytes(&config), config_before);
    assert_eq!(bytes(&bindings), bindings_before);
}

#[test]
fn a_daemon_that_will_not_stop_keeps_its_files() {
    let mut fake = Fake::new();
    fake.seed_curl_install();
    fake.set("FAKE_STOP_STATUS", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 1);
    assert_eq!(read(fake.launcher()), launcher_script("curl-era"));
    assert!(fake.installed_unit().is_file());
    assert!(!fake.logged("systemctl --user enable"));
}

#[test]
fn a_missing_unit_is_tolerated() {
    let mut fake = Fake::new();
    fake.seed_curl_install();
    fake.set("FAKE_STOP_STATUS", "5");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.installed_unit().exists());
}

#[test]
fn a_failed_model_download_leaves_a_curl_era_install_running() {
    let fake = Fake::new();
    fake.seed_curl_install();
    std::fs::remove_file(fake.site("models").join(VOICES)).unwrap();

    let result = fake.setup("");

    assert_ne!(result.code, 0);
    assert_eq!(read(fake.launcher()), launcher_script("curl-era"));
    assert!(fake.installed_unit().is_file());
    assert!(!fake.logged("systemctl --user disable"));
    assert!(!fake.logged("systemctl --user stop"));
}

#[test]
fn a_curl_era_unit_with_an_absolute_launcher_path_is_migrated() {
    let fake = Fake::new();
    fake.seed_curl_install();
    let exec_start = format!("ExecStart={} daemon", fake.launcher().display());
    write(fake.installed_unit(), format!("[Service]\n{exec_start}\n"));

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!fake.installed_unit().exists());
    assert!(fake.logged("systemctl --user disable --now omatalk.service"));
}

#[test]
fn a_users_own_unit_override_is_kept_and_its_daemon_left_running() {
    let fake = Fake::new();
    let unit = "[Unit]\nDescription=mine\n[Service]\nExecStart=/usr/bin/omatalk daemon\nNice=5\n";
    write(fake.installed_unit(), unit);

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(fake.installed_unit()), unit);
    assert!(!fake.logged("systemctl --user disable"));
    assert!(!fake.logged("systemctl --user stop"));
    assert!(
        result.stdout.contains("not a curl-era unit"),
        "{}",
        result.stdout
    );
}
