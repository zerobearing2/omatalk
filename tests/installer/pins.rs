//! install.sh's fixed pins: what it downloads, and that nothing overrides it.

use crate::common::repo;
use crate::fake::*;

#[test]
fn installer_downloads_the_files_the_daemon_loads() {
    assert_eq!(pin("MODEL_FILE").as_deref(), Some(FP32));
    assert_eq!(pin("VOICES_FILE").as_deref(), Some(VOICES));
}

#[test]
fn installer_pins_are_not_read_from_the_environment() {
    for name in [
        "RELEASE_TAG",
        "TARBALL_SHA256",
        "RELEASE_BASE",
        "PLUGIN_REPO",
        "MODEL_BASE",
        "MODEL_SHA256",
        "VOICES_SHA256",
        "MODEL_FILE",
        "VOICES_FILE",
    ] {
        let value = pin(name).unwrap_or_else(|| panic!("install.sh has no {name}= pin"));
        assert!(!value.starts_with('$'), "{name} is not a fixed literal");
    }
}

#[test]
fn installer_ignores_environment_overrides_of_its_pins() {
    let mut fake = Fake::new();
    fake.publish(release("new"));
    for name in ["RELEASE_TAG", "RELEASE_BASE", "MODEL_BASE", "PLUGIN_REPO"] {
        fake.set(name, "https://evil.test/x");
    }
    fake.set("TARBALL_SHA256", "0".repeat(64));

    fake.bash(&repo("install.sh"), "");

    let downloads = fake.downloads();
    assert!(!downloads.is_empty());
    assert!(
        !downloads.iter().any(|l| l.contains("evil.test")),
        "{downloads:?}"
    );
    assert!(
        downloads[0].contains("https://github.com/zerobearing2/omatalk/releases/download/v"),
        "{downloads:?}"
    );
}
