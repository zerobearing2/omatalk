//! Voice names and the voices archive. Listing needs no ORT and no Daemon.

use std::fmt;
use std::path::Path;

/// Kokoro voices archive (npz of `<name>.npy`, each 510x1x256 f32).
pub const VOICES_FILE: &str = "voices-v1.0.bin";

/// A syntactically valid voice name (`[a-z]{2}_[a-z0-9]+`). Membership in the
/// archive is a separate question answered by `list` or the engine.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceName(String);

impl VoiceName {
    pub fn parse(raw: &str) -> Option<VoiceName> {
        let (lang, name) = raw.split_once('_')?;
        let ok = lang.len() == 2
            && lang.bytes().all(|b| b.is_ascii_lowercase())
            && !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        ok.then(|| VoiceName(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VoiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Sorted voice names from the archive's zip directory (entry names minus
/// `.npy`). Reads the central directory only, never the arrays. Entries that
/// are not voice names are skipped.
pub fn list(models: &Path) -> std::io::Result<Vec<VoiceName>> {
    let archive = npyz::npz::NpzArchive::open(models.join(VOICES_FILE))?;
    let mut names: Vec<VoiceName> = archive.array_names().filter_map(VoiceName::parse).collect();
    names.sort();
    Ok(names)
}

/// The one voice-validity check for the CLI (`config set voice`, `speak
/// --voice`). Error Display is the exact stderr line.
pub fn known(models: &Path, raw: &str) -> Result<VoiceName, VoiceError> {
    let unknown = || VoiceError::Unknown(raw.to_owned());
    let name = VoiceName::parse(raw).ok_or_else(unknown)?;
    let all = list(models).map_err(VoiceError::Archive)?;
    all.binary_search(&name)
        .map(|_| name)
        .map_err(|_| unknown())
}

#[derive(Debug)]
pub enum VoiceError {
    Unknown(String),
    Archive(std::io::Error),
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoiceError::Unknown(v) => write!(f, "{v}: not a known voice"),
            VoiceError::Archive(e) => write!(f, "{VOICES_FILE}: {e}"),
        }
    }
}
