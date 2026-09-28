//! The F8 binding in bindings.lua: asked for on install, offered for removal
//! on uninstall, and never taken from another command.

use crate::common::{read, write};
use crate::fake::*;

#[test]
fn install_does_not_bind_f8_on_eof_or_decline() {
    for answer in ["", "n\n"] {
        let fake = Fake::new();
        fake.publish(release("new"));

        let result = fake.install(answer);

        assert_eq!(result.code, 0, "{answer:?}: {}", result.stderr);
        assert!(!fake.bindings().exists(), "{answer:?}");
        assert!(result.stdout.contains("To bind F8"), "{answer:?}");
        assert!(
            fake.commands().iter().any(|l| l == PLUGIN_ADD_CMD),
            "{answer:?}"
        );
        assert!(!fake.logged("hyprctl"), "{answer:?}");
    }
}

#[test]
fn install_binds_f8_on_yes_or_enter() {
    for answer in ["y\n", "\n"] {
        let fake = Fake::new();
        fake.publish(release("new"));

        let result = fake.install(answer);

        assert_eq!(result.code, 0, "{answer:?}: {}", result.stderr);
        assert_eq!(
            read(fake.bindings()),
            format!("\n{BIND_LINE}\n"),
            "{answer:?}"
        );
        assert!(!result.stdout.contains("To bind F8"), "{answer:?}");
        assert!(
            fake.commands().iter().any(|l| l == "hyprctl reload"),
            "{answer:?}"
        );
        assert!(
            fake.commands().iter().any(|l| l == PLUGIN_ADD_CMD),
            "{answer:?}"
        );
    }
}

#[test]
fn install_reasks_after_decline() {
    let fake = Fake::new();
    fake.publish(release("new"));
    write(fake.bindings(), "-- mine\n");

    let first = fake.install("n\n");
    let second = fake.install("y\n");

    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(read(fake.bindings()), format!("-- mine\n\n{BIND_LINE}\n"));
}

#[test]
fn install_never_takes_f8_from_another_command() {
    let fake = Fake::new();
    fake.publish(release("new"));
    write(
        fake.bindings(),
        "o.bind(\"F8\", \"Other\", \"other thing\")\n",
    );
    let before = bytes(fake.bindings());

    let result = fake.install("y\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(bytes(fake.bindings()), before);
    assert!(
        result.stdout.contains("F8 is already bound"),
        "{}",
        result.stdout
    );
    assert!(result.stdout.contains("To bind F8"), "{}", result.stdout);
    assert!(!fake.logged("hyprctl"));
}

#[test]
fn install_rebinds_after_uninstall_leaves_a_comment() {
    let fake = Fake::new();
    fake.publish(release("new"));
    write(
        fake.bindings(),
        format!("-- F8 speaks selection (installed via ~/Work/omatalk/install.sh)\n{BIND_LINE}\n"),
    );

    let uninstalled = fake.uninstall("y\n");
    let reinstalled = fake.install("y\n");

    assert_eq!(uninstalled.code, 0, "{}", uninstalled.stderr);
    assert_eq!(reinstalled.code, 0, "{}", reinstalled.stderr);
    let bindings = read(fake.bindings());
    assert!(
        bindings.ends_with(&format!("\n{BIND_LINE}\n")),
        "{bindings:?}"
    );
    assert_eq!(bindings.matches(BIND_LINE).count(), 1, "{bindings:?}");
}

#[test]
fn uninstall_removes_omatalk_bind_on_yes() {
    let fake = Fake::new();
    let bindings = fake.seed_bindings();

    let result = fake.uninstall("y\n");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        read(&bindings),
        "-- Omatalk: F8 speaks selection\no.bind(\"F9\", \"Dictate\", \"voxtype\")\n"
    );
    assert!(fake.commands().iter().any(|l| l == "hyprctl reload"));
    assert!(
        result.stdout.contains("Removed the Omatalk binding"),
        "{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Remove the o.bind line"),
        "{}",
        result.stdout
    );
}

#[test]
fn uninstall_keeps_bind_on_no_or_eof() {
    for answer in ["", "n\n"] {
        let fake = Fake::new();
        let bindings = fake.seed_bindings();
        let before = bytes(&bindings);

        let result = fake.uninstall(answer);

        assert_eq!(result.code, 0, "{answer:?}: {}", result.stderr);
        assert_eq!(bytes(&bindings), before, "{answer:?}");
        assert!(
            result.stdout.contains("Remove the o.bind line"),
            "{answer:?}"
        );
    }
}

#[test]
fn uninstall_skips_bind_prompt_without_omatalk_bind() {
    let fake = Fake::new();
    write(
        fake.bindings(),
        "o.bind(\"F9\", \"Dictate\", \"voxtype\")\n",
    );

    let result = fake.uninstall("");

    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(!format!("{}{}", result.stdout, result.stderr).contains("Omatalk binding"));
    assert!(!result.stdout.contains("o.bind line"), "{}", result.stdout);
}
