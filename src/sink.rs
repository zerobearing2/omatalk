//! The default sink at press, and the lead of silence it needs before speech.
//!
//! A suspended Bluetooth or HDMI device resumes its link within ~50 ms of a
//! stream opening, then unmutes 375 to 550 ms later. No software signal marks
//! that second step, so speech that arrives in between is lost. Analog sinks
//! suspend too but play from the first sample.

use std::time::{Duration, Instant};

use crate::exec::{self, Argv};

/// Both pactl calls together take ~10 ms. First PCM is ~200 ms after press,
/// so a probe that hits this bound still does not delay speech.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(150);

/// Whether the default sink `name` is Bluetooth or HDMI. DisplayPort sinks
/// are named `hdmi` in PipeWire too.
fn slow_to_wake(name: &str) -> bool {
    name.starts_with("bluez_output.") || name.contains("hdmi")
}

/// Whether `sinks` (`pactl list sinks short`: one tab-separated line per
/// sink, the name second and the state last) lists `name` as SUSPENDED.
fn suspended(name: &str, sinks: &str) -> bool {
    sinks
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .any(|fields| fields.get(1) == Some(&name) && fields.last() == Some(&"SUSPENDED"))
}

/// Silence to play before speech, from the config snapshot at press.
#[derive(Clone, Debug)]
pub struct WakeLead {
    /// pactl-compatible (`sink_probe`).
    pub probe: Argv,
    /// Zero disables the probe.
    pub lead: Duration,
}

impl WakeLead {
    /// `lead` when `probe` reports a suspended Bluetooth or HDMI default
    /// sink, else zero. A missing, failing, or slow probe is zero.
    pub fn measure(&self) -> Duration {
        if self.lead.is_zero() {
            return self.lead;
        }
        let deadline = Instant::now() + PROBE_TIMEOUT;
        let run = |args: &[&str]| {
            let mut cmd = self.probe.command();
            cmd.args(args);
            exec::stdout(cmd, deadline)
        };
        // Name first: an analog default never costs the second pactl.
        let wakes_slowly = run(&["get-default-sink"]).is_some_and(|name| {
            let name = name.trim();
            slow_to_wake(name)
                && run(&["list", "sinks", "short"]).is_some_and(|sinks| suspended(name, &sinks))
        });
        if wakes_slowly {
            self.lead
        } else {
            Duration::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINKS: &str = "\
63\talsa_output.usb-Generic_Blue_Microphones-00.analog-stereo\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED
66\talsa_output.pci-0000_c1_00.6.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING
98\tbluez_output.F4_2B_7D_4B_D0_63.1\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED
99\tbluez_output.00_11_22_33_44_55.1\tPipeWire\ts16le 2ch 48000Hz\tIDLE
19708\talsa_output.pci-0000_c1_00.1.hdmi-stereo-extra2\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED
";

    #[test]
    fn only_a_suspended_bluetooth_or_hdmi_default_sink_wakes_slowly() {
        for (name, want) in [
            ("bluez_output.F4_2B_7D_4B_D0_63.1", true),
            ("bluez_output.00_11_22_33_44_55.1", false),
            ("alsa_output.pci-0000_c1_00.1.hdmi-stereo-extra2", true),
            (
                "alsa_output.usb-Generic_Blue_Microphones-00.analog-stereo",
                false,
            ),
            ("alsa_output.pci-0000_c1_00.6.analog-stereo", false),
            ("bluez_output.gone", false),
            ("", false),
        ] {
            assert_eq!(
                slow_to_wake(name) && suspended(name, SINKS),
                want,
                "{name:?}"
            );
        }
    }
}
