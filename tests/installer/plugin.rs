use crate::common::{read, write};
use crate::fake::*;

#[test]
fn fresh_setup_adds_plugin_repo_and_never_prompts_to_restart_shell() {
    let fake = Fake::new();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fake.commands().iter().any(|l| l == PLUGIN_ADD_CMD));
    assert!(!fake.logged("omarchy restart shell"));
    assert!(fake.plugin_dir().join(".git").is_dir());
}

#[test]
fn git_plugin_checkout_is_updated_in_place() {
    let fake = Fake::new();
    std::fs::create_dir_all(fake.plugin_dir().join(".git")).unwrap();
    write(fake.plugin_dir().join("keep.txt"), "store checkout\n");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(fake.plugin_dir().join("keep.txt")), "store checkout\n");
    assert!(fake.plugin_dir().join(".git").is_dir());
    assert!(
        fake.commands()
            .iter()
            .any(|l| l == "omarchy plugin update zerobearing.omatalk --yes")
    );
    assert!(!fake.logged("omarchy plugin add"));
    assert!(!fake.logged("omarchy plugin remove"));
    assert!(!fake.logged("omarchy plugin enable"));
    assert!(!fake.logged("omarchy restart shell"));
}

#[test]
fn plugin_update_failure_warns_and_still_finishes() {
    let mut fake = Fake::new();
    std::fs::create_dir_all(fake.plugin_dir().join(".git")).unwrap();
    fake.set("FAKE_PLUGIN_UPDATE_FAIL", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("Could not update the bar plugin"),
        "{}",
        result.stdout
    );
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn plugin_add_failure_still_sets_up_the_daemon() {
    let mut fake = Fake::new();
    fake.set("FAKE_PLUGIN_ADD_FAIL", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
    assert!(!fake.plugin_dir().exists());
    assert!(fake.logged("omarchy plugin add"));
    assert!(!fake.logged("omarchy plugin enable"));
    assert!(!fake.logged("omarchy restart shell"));
}

#[test]
fn legacy_copy_is_replaced_via_plugin_remove_and_add() {
    let fake = Fake::new();
    fake.seed_copy_plugin();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fake.plugin_dir().join(".git").is_dir());
    assert!(!fake.plugin_dir().join("old.txt").exists());
    assert!(fake.logged("omarchy plugin remove zerobearing.omatalk --yes"));
    assert!(fake.commands().iter().any(|l| l == PLUGIN_ADD_CMD));
    assert!(!fake.logged("omarchy restart shell"));
}

#[test]
fn legacy_copy_is_left_when_plugin_remove_fails() {
    let mut fake = Fake::new();
    fake.seed_copy_plugin();
    fake.set("FAKE_PLUGIN_REMOVE_FAIL", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(fake.plugin_dir().join("old.txt").is_file());
    assert!(!fake.plugin_dir().join(".git").exists());
    assert!(fake.logged("omarchy plugin remove zerobearing.omatalk --yes"));
    assert!(!fake.logged("omarchy plugin add"));
}
