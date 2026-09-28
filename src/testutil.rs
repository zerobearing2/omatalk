//! Helpers shared by the unit tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("omatalk-test-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes an executable script into the directory and returns its path.
    /// A child shell writes it: a write fd held by this process would leak
    /// into a sibling test's fork and fail exec of the script with ETXTBSY.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let path = self.0.join(name);
        let mut sh = Command::new("sh")
            .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .expect("spawn sh");
        sh.stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .expect("write script");
        assert!(sh.wait().expect("wait sh").success(), "write script");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Polls `check` until it returns `Some` or `timeout` passes.
pub fn wait_for<T>(what: &str, timeout: Duration, mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Lines of a log file that start with `prefix` (a missing file has none).
pub fn log_lines(path: &Path, prefix: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(str::to_owned)
        .collect()
}

/// The shell fakes in tests/fakes.
pub const FAKES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes");

/// The env vars the fakes read, each naming a file in `dir`.
pub fn fake_env(dir: &Path) -> [(&'static str, PathBuf); 6] {
    [
        ("OMATALK_TEST_LOG", dir.join("play.log")),
        ("OMATALK_TEST_NOTIFY_LOG", dir.join("notify.log")),
        ("OMATALK_TEST_TICKS_FILE", dir.join("ticks.txt")),
        ("OMATALK_TEST_CAPTURE_FILE", dir.join("capture.txt")),
        ("OMATALK_TEST_CLIPBOARD_FILE", dir.join("clipboard.txt")),
        ("OMATALK_TEST_PCM", dir.join("pcm.raw")),
    ]
}

/// Config keys that route every external command to a fake. `argv` turns a
/// fake's path into its TOML value.
pub fn fake_commands(argv: impl Fn(&str) -> String) -> [(&'static str, String); 5] {
    [
        ("capture_primary", argv(&format!("{FAKES}/capture-primary"))),
        (
            "capture_clipboard",
            argv(&format!("{FAKES}/capture-clipboard")),
        ),
        ("player", argv(&format!("{FAKES}/player"))),
        ("notify", argv(&format!("{FAKES}/notify"))),
        ("sink_probe", "[\"false\"]".to_owned()),
    ]
}
