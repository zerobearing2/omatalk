//! `omatalk render`: a whole script to a WAV file, in this process. No
//! Daemon, no Stream, no Interrupt. All audio stays in memory until the end,
//! so a failed render never leaves a file and never touches an existing one.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Paths;
use crate::speech::{
    self, Engine, LoadError, SAMPLE_RATE, SpeechError, StopToken, Utterance, s16le,
};

const HEADER_LEN: usize = 44;
/// RIFF counts everything after its own 8 bytes, which is the 36 header
/// bytes after them plus the data.
const RIFF_OVERHEAD: u32 = HEADER_LEN as u32 - 8;

#[derive(Debug)]
pub enum RenderError {
    NotWav(String),
    Load(LoadError),
    Speech(SpeechError),
    TooLong,
    Write(PathBuf, std::io::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::NotWav(out) => write!(
                f,
                "{out}: render writes .wav only; convert it with ffmpeg -i out.wav out.mp3"
            ),
            RenderError::Load(e) => f.write_str(&e.0),
            RenderError::Speech(e) => e.fmt(f),
            RenderError::TooLong => f.write_str("the script is too long for one WAV file"),
            RenderError::Write(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl From<SpeechError> for RenderError {
    fn from(e: SpeechError) -> RenderError {
        RenderError::Speech(e)
    }
}

/// The target path, checked before anything slow happens.
pub fn check_out(raw: &str) -> Result<PathBuf, RenderError> {
    if raw.to_ascii_lowercase().ends_with(".wav") {
        Ok(PathBuf::from(raw))
    } else {
        Err(RenderError::NotWav(raw.to_owned()))
    }
}

/// Renders `u` to `out`; returns the audio's length. `progress` gets the
/// batch count and the seconds so far after each batch.
pub fn render(
    paths: &Paths,
    u: &Utterance,
    out: &Path,
    progress: impl FnMut(usize, Duration),
) -> Result<Duration, RenderError> {
    let mut engine =
        speech::load_engine(&paths.models, paths.fake_engine).map_err(RenderError::Load)?;
    let pcm = synthesize(engine.as_mut(), u, progress)?;
    write_wav(out, &pcm)?;
    Ok(duration(&pcm))
}

fn synthesize(
    engine: &mut dyn Engine,
    u: &Utterance,
    mut progress: impl FnMut(usize, Duration),
) -> Result<Vec<u8>, RenderError> {
    let stop = StopToken::default();
    let mut pcm = Vec::new();
    for (i, audio) in engine.speak(u, &stop).enumerate() {
        pcm.extend(s16le(&audio?));
        progress(i + 1, duration(&pcm));
    }
    Ok(pcm)
}

fn duration(pcm: &[u8]) -> Duration {
    Duration::from_secs_f64(pcm.len() as f64 / 2.0 / f64::from(SAMPLE_RATE))
}

/// The 44-byte header for 16-bit mono PCM at `SAMPLE_RATE`; `None` when the
/// data does not fit RIFF's u32 sizes.
fn header(data_len: usize) -> Option<[u8; HEADER_LEN]> {
    let data_len = u32::try_from(data_len).ok()?;
    let riff_len = data_len.checked_add(RIFF_OVERHEAD)?;
    let mut h = [0u8; HEADER_LEN];
    let fields: [(usize, &[u8]); 12] = [
        (0, b"RIFF"),
        (4, &riff_len.to_le_bytes()),
        (8, b"WAVE"),
        (12, b"fmt "),
        (16, &16u32.to_le_bytes()),
        (20, &1u16.to_le_bytes()),
        (22, &1u16.to_le_bytes()),
        (24, &SAMPLE_RATE.to_le_bytes()),
        (28, &(SAMPLE_RATE * 2).to_le_bytes()),
        (32, &2u16.to_le_bytes()),
        (34, &16u16.to_le_bytes()),
        (36, b"data"),
    ];
    for (at, bytes) in fields {
        h[at..at + bytes.len()].copy_from_slice(bytes);
    }
    h[40..].copy_from_slice(&data_len.to_le_bytes());
    Some(h)
}

/// Writes beside `out`, then renames over it: readers see the old file or
/// the whole new one.
fn write_wav(out: &Path, pcm: &[u8]) -> Result<(), RenderError> {
    let header = header(pcm.len()).ok_or(RenderError::TooLong)?;
    let tmp = temp_beside(out);
    let written = File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(&header)?;
            file.write_all(pcm)
        })
        .and_then(|()| fs::rename(&tmp, out));
    written.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        RenderError::Write(out.to_owned(), e)
    })
}

fn temp_beside(out: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(out.file_name().unwrap_or_default());
    name.push(format!(".{}.tmp", std::process::id()));
    out.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_for_four_bytes_of_pcm() {
        let want: &[u8] = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\xC0\x5D\0\0\x80\xBB\0\0\x02\0\x10\0data\x04\0\0\0";
        assert_eq!(header(4).unwrap().as_slice(), want);
    }

    #[test]
    fn header_refuses_what_riff_cannot_size() {
        assert!(header(u32::MAX as usize - RIFF_OVERHEAD as usize).is_some());
        assert!(header(u32::MAX as usize - RIFF_OVERHEAD as usize + 1).is_none());
        assert!(header(u32::MAX as usize + 1).is_none());
    }

    #[test]
    fn only_wav_paths_pass_whatever_the_case() {
        for ok in ["a.wav", "dir/A.WAV", "x.Wav", ".wav"] {
            assert_eq!(check_out(ok).unwrap(), PathBuf::from(ok));
        }
        for bad in ["a.mp3", "a.wav.mp3", "a", "", "wav", "a.wave"] {
            assert!(
                matches!(check_out(bad), Err(RenderError::NotWav(_))),
                "{bad}"
            );
        }
        let line = check_out("a.mp3").unwrap_err().to_string();
        assert!(line.contains("ffmpeg") && !line.contains('\n'), "{line}");
    }

    #[test]
    fn the_temp_file_sits_beside_the_target() {
        let tmp = temp_beside(Path::new("some/dir/out.wav"));
        assert_eq!(
            tmp,
            Path::new("some/dir").join(format!(".out.wav.{}.tmp", std::process::id()))
        );
        assert_eq!(
            temp_beside(Path::new("out.wav")),
            PathBuf::from(format!(".out.wav.{}.tmp", std::process::id()))
        );
    }
}
