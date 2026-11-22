use crate::common::{read, write};
use crate::fake::*;

#[test]
fn setup_does_not_bind_f8_on_eof_or_decline() {
    for answer in ["", "n\n"] {
        let fake = Fake::new();

        let result = fake.setup(answer);

        assert_eq!(result.code, 0, "{answer:?}: {}", result.stderr);
        assert!(!fake.bindings().exists(), "{answer:?}");
        assert!(result.stdout.contains("To bind F8"), "{answer:?}");
        assert!(fake.logged(RESCAN_CMD), "{answer:?}");
        assert!(!fake.logged("hyprctl"), "{answer:?}");
    }
}

#[test]
fn setup_binds_f8_on_yes_or_enter() {
    for answer in ["y\n", "\n"] {
        let fake = Fake::new();

        let result = fake.setup(answer);

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
        assert!(fake.logged(RESCAN_CMD), "{answer:?}");
    }
}

#[test]
fn setup_reasks_after_decline() {
    let fake = Fake::new();
    write(fake.bindings(), "-- mine\n");

    let first = fake.setup("n\n");
    let second = fake.setup("y\n");

    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(read(fake.bindings()), format!("-- mine\n\n{BIND_LINE}\n"));
}

#[test]
fn setup_never_takes_f8_from_another_command() {
    let fake = Fake::new();
    write(
        fake.bindings(),
        "o.bind(\"F8\", \"Other\", \"other thing\")\n",
    );
    let before = bytes(fake.bindings());

    let result = fake.setup("y\n");

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
fn setup_rebinds_when_only_the_comment_is_left() {
    let fake = Fake::new();
    write(
        fake.bindings(),
        "-- F8 speaks selection (installed via ~/Work/omatalk/install.sh)\n",
    );

    let resetup = fake.setup("y\n");

    assert_eq!(resetup.code, 0, "{}", resetup.stderr);
    let bindings = read(fake.bindings());
    assert!(
        bindings.ends_with(&format!("\n{BIND_LINE}\n")),
        "{bindings:?}"
    );
    assert_eq!(bindings.matches(BIND_LINE).count(), 1, "{bindings:?}");
}
