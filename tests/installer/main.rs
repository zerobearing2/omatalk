//! install.sh and uninstall.sh against a fake release site and the stubbed
//! system commands in tests/fakes/system, with HOME in a temp dir.

#[path = "../common/mod.rs"]
mod common;

mod bindings;
mod fake;
mod install;
mod pins;
mod plugin;
mod uninstall;
