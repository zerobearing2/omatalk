use crate::common::{read, write};
use crate::fake::*;

fn is_link_to(link: &std::path::Path, target: &std::path::Path) -> bool {
    std::fs::read_link(link).is_ok_and(|t| t == target)
}

#[test]
fn fresh_setup_links_the_packaged_plugin_rescans_and_enables_it() {
    let fake = Fake::new();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    assert!(fake.plugin_dir().join("manifest.json").is_file());
    assert!(fake.plugin_enabled());
    let rescan = fake.last_index(RESCAN_CMD);
    let enable = fake.last_index(ENABLE_CMD);
    assert!(rescan < enable, "{:?}", fake.commands());
    for gone in [
        "plugin add",
        "plugin update",
        "plugin remove",
        "restart shell",
    ] {
        assert!(!fake.logged(&format!("omarchy {gone}")), "{gone}");
    }
}

#[test]
fn setup_twice_leaves_the_plugin_link_alone() {
    let fake = Fake::new();
    assert_eq!(fake.setup("").code, 0);
    let before = fake.commands();

    let second = fake.setup("");

    assert_eq!(second.code, 0, "{}", second.stderr);
    assert!(!second.stdout.contains("Replacing"), "{}", second.stdout);
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    let added: Vec<String> = fake.commands()[before.len()..]
        .iter()
        .filter(|l| l.contains("plugin") || l.contains("Plugins"))
        .cloned()
        .collect();
    assert_eq!(added, Vec::<String>::new());
}

#[test]
fn a_curl_era_checkout_is_replaced_by_the_link() {
    let fake = Fake::new();
    fake.seed_checkout_plugin();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    assert!(!fake.plugin_source().join("old.txt").exists());
    assert!(!fake.plugin_dir().join("old.txt").exists());
    assert!(
        result.stdout.contains(&format!(
            "Replacing the git checkout at {}",
            fake.plugin_dir().display()
        )),
        "{}",
        result.stdout
    );
    let enable = fake.last_index(ENABLE_CMD);
    let restart = fake.last_index("omarchy restart shell");
    assert!(enable < restart, "{:?}", fake.commands());
    assert!(!fake.logged(RESCAN_CMD));
}

#[test]
fn a_plugin_that_is_already_enabled_is_not_enabled_again() {
    let fake = Fake::new();
    fake.seed_checkout_plugin();
    write(fake.path("state/plugin-enabled"), "");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    assert!(fake.logged("omarchy restart shell"));
    assert!(!fake.logged("omarchy plugin enable"));
}

#[test]
fn a_link_to_another_directory_is_repointed_and_its_target_kept() {
    let fake = Fake::new();
    let clone = fake.path("clone");
    write(clone.join("manifest.json"), "{}\n");
    std::fs::create_dir_all(fake.plugin_dir().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&clone, fake.plugin_dir()).unwrap();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    assert_eq!(read(clone.join("manifest.json")), "{}\n");
    assert!(
        result.stdout.contains(&format!(
            "Replacing the link {} -> {}",
            fake.plugin_dir().display(),
            clone.display()
        )),
        "{}",
        result.stdout
    );
    assert!(fake.logged("omarchy restart shell"));
}

#[test]
fn a_failed_rescan_warns_and_still_enables_and_finishes() {
    let mut fake = Fake::new();
    fake.set("FAKE_RESCAN_STATUS", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("Could not rescan plugins"),
        "{}",
        result.stdout
    );
    assert!(fake.plugin_enabled());
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn a_failed_enable_warns_and_still_finishes() {
    let mut fake = Fake::new();
    fake.set("FAKE_PLUGIN_ENABLE_FAIL", "1");

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("Could not enable the bar plugin"),
        "{}",
        result.stdout
    );
    assert!(is_link_to(&fake.plugin_dir(), &fake.plugin_source()));
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn a_missing_packaged_plugin_warns_and_keeps_what_is_there() {
    let fake = Fake::new();
    fake.seed_checkout_plugin();
    std::fs::remove_dir_all(fake.plugin_source()).unwrap();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("is missing"), "{}", result.stdout);
    assert!(fake.plugin_dir().join("old.txt").is_file());
    assert!(!fake.logged(RESCAN_CMD));
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}

#[test]
fn setup_without_omarchy_warns_and_finishes() {
    let mut fake = Fake::new();
    fake.without_omarchy();

    let result = fake.setup("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.contains("omarchy not found"),
        "{}",
        result.stdout
    );
    assert!(!fake.plugin_dir().exists());
    assert!(result.stdout.contains("Done."), "{}", result.stdout);
}
