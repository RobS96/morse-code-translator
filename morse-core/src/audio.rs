//! Audio rendering: a keying schedule to PCM samples, and PCM samples to
//! a WAV stream.
//!
//! Like the rest of the crate this plays nothing itself. It produces the
//! samples; handing them to an audio device or a file is the caller's job.

use std::f64::consts::PI;
use std::fmt;
use std::io::{self, Write};

use crate::{ScheduleStep, Signal, Timing, build_schedule};

/// Lowest tone frequency [`Tone`] accepts, in Hz.
pub const MIN_FREQUENCY_HZ: f32 = 20.0;
/// Highest tone frequency [`Tone`] accepts, in Hz. The frequency must
/// also stay below half the sample rate.
pub const MAX_FREQUENCY_HZ: f32 = 20_000.0;

/// Lowest sample rate [`Tone`] accepts, in Hz.
pub const MIN_SAMPLE_RATE: u32 = 8_000;
/// Highest sample rate [`Tone`] accepts, in Hz.
pub const MAX_SAMPLE_RATE: u32 = 192_000;

/// Most samples one rendering may hold: an hour at 48 kHz. A longer
/// transmission is a [`RenderError::TooLong`] rather than an attempt to
/// allocate without bound (a single dash at
/// [`MAX_UNIT_MS`](crate::MAX_UNIT_MS) already lasts three minutes).
pub const MAX_RENDER_SAMPLES: u64 = 60 * 60 * 48_000;

/// The tone a transmission is rendered with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    /// Pitch in Hz, from [`MIN_FREQUENCY_HZ`] to [`MAX_FREQUENCY_HZ`] and
    /// below half of `sample_rate`.
    pub frequency_hz: f32,
    /// Samples per second, from [`MIN_SAMPLE_RATE`] to [`MAX_SAMPLE_RATE`].
    pub sample_rate: u32,
    /// Peak amplitude, from 0.0 (silent) to 1.0 (full scale).
    pub volume: f32,
    /// Length in ms of the raised-cosine attack and release that keep the
    /// keying from clicking; 0 keys the tone hard on and off. Shortened
    /// where needed so that it never exceeds half the shortest tone.
    pub ramp_ms: f32,
}

impl Default for Tone {
    /// A 600 Hz tone at a fifth of full scale, 44.1 kHz, with 5 ms ramps.
    fn default() -> Self {
        Self {
            frequency_hz: 600.0,
            sample_rate: 44_100,
            volume: 0.2,
            ramp_ms: 5.0,
        }
    }
}

impl Tone {
    /// Check every field against its documented range. Rendering does
    /// this itself; call it to reject bad values before doing other work.
    pub fn validate(&self) -> Result<(), RenderError> {
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&self.sample_rate) {
            return Err(RenderError::SampleRate(self.sample_rate));
        }
        // A NaN is in no range, so each of these rejects it too.
        if !(MIN_FREQUENCY_HZ..=MAX_FREQUENCY_HZ).contains(&self.frequency_hz)
            || f64::from(self.frequency_hz) * 2.0 >= f64::from(self.sample_rate)
        {
            return Err(RenderError::Frequency(self.frequency_hz));
        }
        if !(0.0..=1.0).contains(&self.volume) {
            return Err(RenderError::Volume(self.volume));
        }
        if !(0.0..f32::INFINITY).contains(&self.ramp_ms) {
            return Err(RenderError::Ramp(self.ramp_ms));
        }
        Ok(())
    }
}

/// Why a transmission could not be rendered. The parameter variants carry
/// the rejected value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderError {
    /// The frequency is not a number from [`MIN_FREQUENCY_HZ`] to
    /// [`MAX_FREQUENCY_HZ`], or is not below half the sample rate.
    Frequency(f32),
    /// The sample rate is outside [`MIN_SAMPLE_RATE`] to
    /// [`MAX_SAMPLE_RATE`].
    SampleRate(u32),
    /// The volume is not a number from 0 to 1.
    Volume(f32),
    /// The ramp length is not zero or a positive number.
    Ramp(f32),
    /// The transmission needs more than [`MAX_RENDER_SAMPLES`] samples,
    /// or more memory than is available.
    TooLong,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Frequency(hz) => write!(
                f,
                "tone frequency must be between {MIN_FREQUENCY_HZ} and {MAX_FREQUENCY_HZ} Hz \
                 and below half the sample rate, got {hz}"
            ),
            RenderError::SampleRate(rate) => write!(
                f,
                "sample rate must be between {MIN_SAMPLE_RATE} and {MAX_SAMPLE_RATE} Hz, \
                 got {rate}"
            ),
            RenderError::Volume(volume) => {
                write!(f, "volume must be between 0 and 1, got {volume}")
            }
            RenderError::Ramp(ms) => {
                write!(f, "ramp length must be 0 ms or more, got {ms}")
            }
            RenderError::TooLong => write!(
                f,
                "transmission is too long to render (the limit is {MAX_RENDER_SAMPLES} samples)"
            ),
        }
    }
}

impl std::error::Error for RenderError {}

/// Render a signal plan as mono PCM samples in `-volume..=volume`: the
/// plan laid out by [`build_schedule`], then [`render_schedule`].
pub fn render_samples(
    plan: &[Signal],
    timing: Timing,
    tone: &Tone,
) -> Result<Vec<f32>, RenderError> {
    render_schedule(&build_schedule(plan, timing), tone)
}

/// Render a keying schedule as mono PCM samples in `-volume..=volume`.
///
/// Every step starts and ends at the sample nearest its cumulative time,
/// so rounding never accumulates: step lengths differ from their nominal
/// length by less than a sample and the total is the rounded total
/// duration. Each tone is a sine wave starting at phase zero, shaped by a
/// raised-cosine attack and release of `tone.ramp_ms` whose first and last
/// samples are zero. Silence is exactly `0.0`.
pub fn render_schedule(schedule: &[ScheduleStep], tone: &Tone) -> Result<Vec<f32>, RenderError> {
    tone.validate()?;
    let rate = tone.sample_rate;

    // Where each step ends, in samples, and the shortest tone among them.
    let mut ends: Vec<usize> = Vec::with_capacity(schedule.len());
    let mut shortest_tone = usize::MAX;
    let mut elapsed_ms: u64 = 0;
    let mut start = 0;
    for step in schedule {
        elapsed_ms = elapsed_ms.saturating_add(step.duration_ms);
        let end = sample_index(elapsed_ms, rate).ok_or(RenderError::TooLong)?;
        if step.is_tone() && end > start {
            shortest_tone = shortest_tone.min(end - start);
        }
        ends.push(end);
        start = end;
    }
    let total = start;

    // Saturating: a huge `ramp_ms` just means "as long as the tones allow".
    let ramp = (f64::from(tone.ramp_ms) * f64::from(rate) / 1000.0).round() as usize;
    let ramp = ramp.min(shortest_tone / 2);

    let mut samples: Vec<f32> = Vec::new();
    samples
        .try_reserve_exact(total)
        .map_err(|_| RenderError::TooLong)?;
    samples.resize(total, 0.0);

    let volume = f64::from(tone.volume);
    let radians_per_sample = 2.0 * PI * f64::from(tone.frequency_hz) / f64::from(rate);
    let mut start = 0;
    for (step, &end) in schedule.iter().zip(&ends) {
        if step.is_tone() {
            let keyed = &mut samples[start..end];
            let len = keyed.len();
            for (i, sample) in keyed.iter_mut().enumerate() {
                // Distance to the nearer end of the tone.
                let edge = i.min(len - 1 - i);
                let gain = if edge < ramp {
                    0.5 * (1.0 - (PI * edge as f64 / ramp as f64).cos())
                } else {
                    1.0
                };
                *sample = (volume * gain * (radians_per_sample * i as f64).sin()) as f32;
            }
        }
        start = end;
    }
    Ok(samples)
}

/// The sample that `ms` into a rendering falls on, to the nearest sample;
/// `None` past [`MAX_RENDER_SAMPLES`].
fn sample_index(ms: u64, sample_rate: u32) -> Option<usize> {
    let index = (u128::from(ms) * u128::from(sample_rate) + 500) / 1000;
    if index > u128::from(MAX_RENDER_SAMPLES) {
        return None;
    }
    usize::try_from(index).ok()
}

/// Write `samples` as a WAV stream: a 44-byte RIFF header, then 16-bit
/// little-endian PCM, mono, at `sample_rate`.
///
/// Samples are scaled from `-1.0..=1.0`; anything outside is clipped to
/// full scale and a NaN is written as silence. `out` is not flushed. More
/// samples than a WAV header can count (about 2^31), or a sample rate of
/// zero or of 2^31 Hz and more, is an [`io::ErrorKind::InvalidInput`]
/// error with nothing written.
pub fn write_wav(out: &mut impl Write, samples: &[f32], sample_rate: u32) -> io::Result<()> {
    out.write_all(&wav_header(samples.len() as u64, sample_rate)?)?;
    // Converted a block at a time, so that a writer with no buffer of its
    // own is not handed two bytes per call.
    let mut block = [0u8; 2 * WAV_BLOCK_SAMPLES];
    for chunk in samples.chunks(WAV_BLOCK_SAMPLES) {
        for (i, &sample) in chunk.iter().enumerate() {
            block[2 * i..2 * i + 2].copy_from_slice(&pcm16(sample).to_le_bytes());
        }
        out.write_all(&block[..2 * chunk.len()])?;
    }
    Ok(())
}

/// Bytes per sample of the 16-bit mono PCM that [`write_wav`] writes.
const WAV_BYTES_PER_SAMPLE: u32 = 2;
/// Samples [`write_wav`] converts and writes per call to the writer.
const WAV_BLOCK_SAMPLES: usize = 4096;

/// A sample as 16-bit PCM. The scale is symmetric (-32767 to 32767), and
/// the cast turns a NaN into 0.
fn pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

/// The canonical 44-byte header of a 16-bit mono PCM WAV file holding
/// `sample_count` samples.
fn wav_header(sample_count: u64, sample_rate: u32) -> io::Result<[u8; 44]> {
    let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidInput, what.to_string());
    if sample_rate == 0 {
        return Err(invalid("WAV sample rate must not be zero"));
    }
    let byte_rate = sample_rate
        .checked_mul(WAV_BYTES_PER_SAMPLE)
        .ok_or_else(|| invalid("WAV sample rate is too high"))?;
    // The RIFF chunk size counts the 36 header bytes after it as well.
    let sizes = sample_count
        .checked_mul(u64::from(WAV_BYTES_PER_SAMPLE))
        .and_then(|data| u32::try_from(data).ok())
        .and_then(|data| Some((data.checked_add(36)?, data)));
    let Some((riff_size, data_size)) = sizes else {
        return Err(invalid("too many samples for a WAV file"));
    };

    let mut header = [0u8; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&riff_size.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    // Format 1 is integer PCM; one channel.
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&1u16.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&(WAV_BYTES_PER_SAMPLE as u16).to_le_bytes());
    header[34..36].copy_from_slice(&16u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StepKind, build_signal_plan, schedule_duration_ms};

    /// The sample a time falls on, rounded half up: the reference the
    /// renderer's boundaries are checked against.
    fn sample_at(ms: u64, rate: u32) -> usize {
        ((ms as u128 * rate as u128 + 500) / 1000) as usize
    }

    /// `(is_tone, start, end)` of every step, in samples.
    fn spans(schedule: &[ScheduleStep], rate: u32) -> Vec<(bool, usize, usize)> {
        let mut elapsed_ms = 0;
        schedule
            .iter()
            .map(|step| {
                let start = sample_at(elapsed_ms, rate);
                elapsed_ms += step.duration_ms;
                (step.is_tone(), start, sample_at(elapsed_ms, rate))
            })
            .collect()
    }

    fn tone(frequency_hz: f32, sample_rate: u32, volume: f32, ramp_ms: f32) -> Tone {
        Tone {
            frequency_hz,
            sample_rate,
            volume,
            ramp_ms,
        }
    }

    #[test]
    fn total_length_is_the_rounded_total_duration() {
        for (text, timing) in [
            ("PARIS", Timing::uniform(60)),
            ("PARIS PARIS", Timing::farnsworth_wpm(20.0, 5.0)),
            ("SOS <SK>", Timing::uniform(7)),
            ("CQ CQ DE", Timing::uniform(1)),
            ("한글", Timing::farnsworth_wpm(13.0, 7.0)),
            ("E", Timing::uniform(1)),
        ] {
            for rate in [8_000, 11_025, 22_050, 44_100, 48_000] {
                let plan = build_signal_plan(text);
                let schedule = build_schedule(&plan, timing);
                let samples =
                    render_samples(&plan, timing, &tone(600.0, rate, 0.5, 5.0)).expect("renders");
                assert_eq!(
                    samples.len(),
                    sample_at(schedule_duration_ms(&schedule), rate),
                    "{text:?} at {rate} Hz"
                );
            }
        }
    }

    #[test]
    fn step_lengths_come_from_cumulative_time_so_rounding_never_accumulates() {
        // 1 ms units at 11025 Hz are 11.025 samples each: truncating or
        // rounding each step on its own would drift by a sample every 40
        // steps. 400 dots must still end within a sample of where they
        // should, and every step must sit on its own rounded boundaries.
        let rate = 11_025;
        let timing = Timing::uniform(1);
        let plan = vec![Signal::Dot; 400];
        let schedule = build_schedule(&plan, timing);
        // No ramp, so that every sample of a tone but its first is audible.
        let samples = render_samples(&plan, timing, &tone(1_000.0, rate, 1.0, 0.0)).unwrap();
        assert_eq!(samples.len(), sample_at(799, rate));
        let mut lengths = std::collections::BTreeSet::new();
        for (is_tone, start, end) in spans(&schedule, rate) {
            lengths.insert(end - start);
            let audible = samples[start..end].iter().filter(|s| **s != 0.0).count();
            if is_tone {
                assert!(audible >= end - start - 2, "tone at {start}..{end}");
            } else {
                assert_eq!(audible, 0, "silence at {start}..{end}");
            }
        }
        assert_eq!(lengths.into_iter().collect::<Vec<_>>(), vec![11, 12]);
    }

    #[test]
    fn every_tone_ramps_up_from_zero_and_back_down_to_zero() {
        let rate = 44_100;
        let timing = Timing::uniform(60);
        let plan = build_signal_plan("PARIS");
        let schedule = build_schedule(&plan, timing);
        let samples = render_samples(&plan, timing, &tone(600.0, rate, 0.8, 5.0)).unwrap();
        // 5 ms at 44.1 kHz, rounded.
        let ramp = 221;
        let wave = |i: usize| (2.0 * PI * 600.0 * i as f64 / rate as f64).sin();
        let gain = |edge: usize| 0.5 * (1.0 - (PI * edge as f64 / ramp as f64).cos());
        let mut tones = 0;
        for (is_tone, start, end) in spans(&schedule, rate) {
            if !is_tone {
                continue;
            }
            tones += 1;
            let tone = &samples[start..end];
            assert_eq!(tone[0], 0.0, "first sample of the tone at {start}");
            assert_eq!(
                tone[tone.len() - 1],
                0.0,
                "last sample of the tone at {start}"
            );
            for i in 0..tone.len() {
                let edge = i.min(tone.len() - 1 - i);
                let expected = 0.8 * wave(i) * if edge < ramp { gain(edge) } else { 1.0 };
                assert!(
                    (tone[i] as f64 - expected).abs() < 1e-6,
                    "sample {i} of the tone at {start}: {} vs {expected}",
                    tone[i]
                );
            }
            // The envelope rises steadily: no step at the start of a tone.
            assert!(tone[1].abs() < 0.001 && tone[tone.len() - 2].abs() < 0.001);
        }
        assert_eq!(tones, 14);
    }

    #[test]
    fn ramp_is_clamped_to_half_the_shortest_tone() {
        // Dots of 80 samples and dashes of 240: a 1 s ramp becomes 40
        // samples, for the dashes too.
        let rate = 8_000;
        let timing = Timing::uniform(10);
        let plan = build_signal_plan("A");
        let samples = render_samples(&plan, timing, &tone(1_000.0, rate, 1.0, 1_000.0)).unwrap();
        let schedule = build_schedule(&plan, timing);
        let spans = spans(&schedule, rate);
        let (_, dot_start, dot_end) = spans[0];
        let (_, dash_start, dash_end) = spans[2];
        assert_eq!((dot_end - dot_start, dash_end - dash_start), (80, 240));
        let wave = |i: usize| (2.0 * PI * 1_000.0 * i as f64 / rate as f64).sin();
        let dash = &samples[dash_start..dash_end];
        // Full amplitude from sample 40 to sample 199 of the dash.
        for (i, &sample) in dash.iter().enumerate().take(200).skip(40) {
            assert!((sample as f64 - wave(i)).abs() < 1e-6, "dash sample {i}");
        }
        // 1 kHz at 8 kHz peaks on samples 2, 10, 18, ...: sample 34 is a
        // peak, and still below full amplitude inside the ramp.
        assert!((wave(34) - 1.0).abs() < 1e-9);
        assert!(dash[34] < 0.99 && dash[34] > 0.5, "{}", dash[34]);
        // The dot never reaches full amplitude: its attack runs to the
        // midpoint and its release starts there.
        let dot = &samples[dot_start..dot_end];
        assert!(dot[34] < 0.99 && dot[34] > 0.5, "{}", dot[34]);
        assert!(dot[42] < 0.99 && dot[42] > 0.5, "{}", dot[42]);
        assert_eq!((dot[0], dot[79]), (0.0, 0.0));
        // Tones too short to ramp at all are still rendered without panic.
        for unit in [0, 1] {
            let samples = render_samples(
                &build_signal_plan("SOS"),
                Timing::uniform(unit),
                &tone(600.0, MIN_SAMPLE_RATE, 1.0, 5.0),
            )
            .unwrap();
            assert!(samples.iter().all(|s| s.is_finite()));
        }
    }

    #[test]
    fn silence_is_exactly_zero_and_amplitude_never_exceeds_the_volume() {
        for volume in [0.0, 0.2, 0.33, 1.0] {
            for (rate, ramp_ms) in [(44_100, 5.0), (48_000, 0.0), (8_000, 2.5)] {
                let timing = Timing::farnsworth_wpm(25.0, 10.0);
                let plan = build_signal_plan("CQ DE <KN>");
                let schedule = build_schedule(&plan, timing);
                let samples =
                    render_samples(&plan, timing, &tone(880.0, rate, volume, ramp_ms)).unwrap();
                assert!(
                    samples.iter().all(|s| s.abs() <= volume),
                    "volume {volume} at {rate} Hz"
                );
                let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
                assert!(peak >= volume * 0.99, "peak {peak} at volume {volume}");
                for (is_tone, start, end) in spans(&schedule, rate) {
                    if !is_tone {
                        assert!(samples[start..end].iter().all(|s| *s == 0.0));
                    }
                }
            }
        }
    }

    #[test]
    fn bad_tone_parameters_are_errors_not_panics_or_nan_samples() {
        let plan = build_signal_plan("SOS");
        let timing = Timing::uniform(10);
        let good = Tone::default();
        assert_eq!(good.validate(), Ok(()));
        assert_eq!(
            (
                good.frequency_hz,
                good.sample_rate,
                good.volume,
                good.ramp_ms
            ),
            (600.0, 44_100, 0.2, 5.0)
        );

        for hz in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -600.0,
            0.0,
            19.9,
            20_000.1,
        ] {
            let bad = Tone {
                frequency_hz: hz,
                ..good
            };
            assert!(
                matches!(bad.validate(), Err(RenderError::Frequency(_))),
                "{hz} Hz"
            );
            assert!(render_samples(&plan, timing, &bad).is_err(), "{hz} Hz");
        }
        // In range, but not below half the sample rate.
        assert_eq!(
            tone(4_000.0, 8_000, 0.2, 5.0).validate(),
            Err(RenderError::Frequency(4_000.0))
        );
        assert_eq!(tone(3_999.0, 8_000, 0.2, 5.0).validate(), Ok(()));

        for volume in [f32::NAN, f32::INFINITY, -0.1, 1.01, -1.0] {
            let bad = Tone { volume, ..good };
            assert!(
                matches!(bad.validate(), Err(RenderError::Volume(_))),
                "volume {volume}"
            );
            assert!(render_samples(&plan, timing, &bad).is_err());
        }
        for sample_rate in [0, 1, 7_999, 192_001, u32::MAX] {
            let bad = Tone {
                sample_rate,
                ..good
            };
            assert_eq!(bad.validate(), Err(RenderError::SampleRate(sample_rate)));
            assert!(render_samples(&plan, timing, &bad).is_err());
        }
        for ramp_ms in [f32::NAN, f32::INFINITY, -1.0] {
            let bad = Tone { ramp_ms, ..good };
            assert!(
                matches!(bad.validate(), Err(RenderError::Ramp(_))),
                "ramp {ramp_ms}"
            );
            assert!(render_samples(&plan, timing, &bad).is_err());
        }
        // The ends of every range are accepted, and render finite samples.
        for ok in [
            tone(MIN_FREQUENCY_HZ, MIN_SAMPLE_RATE, 0.0, 0.0),
            tone(MAX_FREQUENCY_HZ, MAX_SAMPLE_RATE, 1.0, 1e9),
        ] {
            let samples = render_samples(&plan, timing, &ok).expect("in range");
            assert!(!samples.is_empty() && samples.iter().all(|s| s.is_finite()));
        }
        // Every error explains itself in one line.
        for err in [
            RenderError::Frequency(f32::NAN),
            RenderError::SampleRate(0),
            RenderError::Volume(2.0),
            RenderError::Ramp(-1.0),
            RenderError::TooLong,
        ] {
            let message = err.to_string();
            assert!(!message.is_empty() && !message.contains('\n'), "{message}");
        }
    }

    #[test]
    fn a_transmission_too_long_to_hold_is_an_error() {
        let tone = Tone::default();
        // `.err()`, so that a failure here does not print the samples.
        let error = |plan: &[Signal], timing| render_samples(plan, timing, &tone).err();
        // A unit length built by hand, past every clamp.
        assert_eq!(
            error(&build_signal_plan("E E"), Timing::uniform(u64::MAX)),
            Some(RenderError::TooLong)
        );
        // Two hours at the slowest speed the clamps allow.
        let slow = Timing::uniform(crate::MAX_UNIT_MS);
        assert_eq!(error(&[Signal::Dash; 30], slow), Some(RenderError::TooLong));
        // The limit is on samples: a millisecond past the hour at 48 kHz
        // is over it, and so is half an hour at 192 kHz.
        let silence = |duration_ms| {
            [ScheduleStep {
                kind: StepKind::LetterGap,
                duration_ms,
            }]
        };
        let at = |sample_rate| Tone {
            sample_rate,
            ..tone
        };
        let hour_ms = MAX_RENDER_SAMPLES / 48;
        assert_eq!(
            render_schedule(&silence(hour_ms + 1), &at(48_000)).err(),
            Some(RenderError::TooLong)
        );
        assert_eq!(
            render_schedule(&silence(hour_ms / 2), &at(192_000)).err(),
            Some(RenderError::TooLong)
        );
        assert_eq!(sample_index(hour_ms, 48_000), Some(172_800_000));
        assert_eq!(sample_index(hour_ms + 1, 48_000), None);
        assert_eq!(sample_index(u64::MAX, MAX_SAMPLE_RATE), None);
        let second = render_schedule(&silence(1_000), &at(48_000)).unwrap();
        assert_eq!(second.len(), 48_000);
    }

    #[test]
    fn an_empty_plan_renders_no_samples() {
        assert_eq!(
            render_samples(&[], Timing::uniform(60), &Tone::default()),
            Ok(vec![])
        );
        assert_eq!(render_schedule(&[], &Tone::default()), Ok(vec![]));
    }

    /// The fields of a WAV header, read back the way a player would.
    #[derive(Debug, PartialEq)]
    struct WavHeader {
        riff_size: u32,
        format: u16,
        channels: u16,
        sample_rate: u32,
        byte_rate: u32,
        block_align: u16,
        bits_per_sample: u16,
        data_size: u32,
    }

    /// Parse the canonical 44-byte header and return it with the PCM
    /// samples that follow.
    fn parse_wav(bytes: &[u8]) -> (WavHeader, Vec<i16>) {
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let u32_at =
            |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32_at(16), 16, "PCM fmt chunk size");
        assert_eq!(&bytes[36..40], b"data");
        let header = WavHeader {
            riff_size: u32_at(4),
            format: u16_at(20),
            channels: u16_at(22),
            sample_rate: u32_at(24),
            byte_rate: u32_at(28),
            block_align: u16_at(32),
            bits_per_sample: u16_at(34),
            data_size: u32_at(40),
        };
        let samples = bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        (header, samples)
    }

    #[test]
    fn wav_header_describes_16_bit_mono_pcm() {
        let mut bytes = Vec::new();
        write_wav(&mut bytes, &[0.0, 0.5, -0.5, 0.25], 44_100).unwrap();
        assert_eq!(bytes.len(), 44 + 8);
        let (header, samples) = parse_wav(&bytes);
        assert_eq!(
            header,
            WavHeader {
                riff_size: 36 + 8,
                format: 1,
                channels: 1,
                sample_rate: 44_100,
                byte_rate: 88_200,
                block_align: 2,
                bits_per_sample: 16,
                data_size: 8,
            }
        );
        assert_eq!(samples, vec![0, 16_384, -16_384, 8_192]);
        // Byte for byte, for the fields a parser of our own could misread.
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes[4..8], 44u32.to_le_bytes());
        assert_eq!(bytes[24..28], [0x44, 0xAC, 0x00, 0x00]);
        assert_eq!(bytes[44..48], [0x00, 0x00, 0x00, 0x40]);
    }

    #[test]
    fn wav_of_no_samples_is_a_bare_header() {
        let mut bytes = Vec::new();
        write_wav(&mut bytes, &[], 8_000).unwrap();
        assert_eq!(bytes.len(), 44);
        let (header, samples) = parse_wav(&bytes);
        assert_eq!((header.riff_size, header.data_size), (36, 0));
        assert_eq!(header.byte_rate, 16_000);
        assert_eq!(samples, vec![]);
    }

    #[test]
    fn wav_clips_at_full_scale() {
        let mut bytes = Vec::new();
        let samples = [
            1.0,
            -1.0,
            1.5,
            -1.5,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            1e-9,
        ];
        write_wav(&mut bytes, &samples, 44_100).unwrap();
        let (_, pcm) = parse_wav(&bytes);
        assert_eq!(
            pcm,
            vec![32_767, -32_767, 32_767, -32_767, 32_767, -32_767, 0, 0]
        );
    }

    #[test]
    fn wav_round_trips_a_rendered_transmission() {
        let tone = Tone::default();
        let samples =
            render_samples(&build_signal_plan("SOS"), Timing::uniform(60), &tone).unwrap();
        let mut bytes = Vec::new();
        write_wav(&mut bytes, &samples, tone.sample_rate).unwrap();
        let (header, pcm) = parse_wav(&bytes);
        assert_eq!(header.sample_rate, tone.sample_rate);
        assert_eq!(header.data_size as usize, 2 * samples.len());
        assert_eq!(header.riff_size, 36 + header.data_size);
        assert_eq!(bytes.len(), 44 + 2 * samples.len());
        assert_eq!(pcm.len(), samples.len());
        for (i, (&pcm, &sample)) in pcm.iter().zip(&samples).enumerate() {
            assert!(
                (pcm as f32 / 32_767.0 - sample).abs() <= 0.5 / 32_767.0,
                "sample {i}: {pcm} vs {sample}"
            );
        }
        // 27 units of 60 ms at 44.1 kHz.
        assert_eq!(pcm.len(), 27 * 60 * 441 / 10);
    }

    #[test]
    fn wav_refuses_what_its_header_cannot_describe() {
        assert_eq!(
            wav_header(4, 0).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            wav_header(4, u32::MAX).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        // The largest data chunk a 32-bit RIFF size can count.
        let most = (u32::MAX as u64 - 36) / 2;
        assert!(wav_header(most, 44_100).is_ok());
        assert_eq!(
            wav_header(most + 1, 44_100).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        // Nothing is written when the header cannot be.
        let mut bytes = Vec::new();
        assert!(write_wav(&mut bytes, &[0.0], 0).is_err());
        assert_eq!(bytes, Vec::<u8>::new());
    }

    #[test]
    fn wav_reports_a_failed_write() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::StorageFull.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let err = write_wav(&mut Full, &[0.0; 16], 44_100).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::StorageFull);
    }
}
