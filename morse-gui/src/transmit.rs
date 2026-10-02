//! The parts of a transmission that need neither a window nor a sound
//! card: mapping the controls to timing and tone, walking the lamp along
//! the schedule, cancelling, and summarising what a translation left out.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use morse_core::{MAX_SAMPLE_RATE, MIN_SAMPLE_RATE, ScheduleStep, Timing, Tone, wpm_to_unit_ms};

/// Range of the tone slider, in Hz.
pub const MIN_TONE_HZ: f32 = 300.0;
pub const MAX_TONE_HZ: f32 = 1200.0;
/// Where the tone slider starts.
pub const DEFAULT_TONE_HZ: f32 = 600.0;
/// Where the volume slider starts, in percent of full scale.
pub const DEFAULT_VOLUME_PERCENT: f32 = 20.0;

/// Longest the transmit thread sleeps before it looks at the clock and
/// the cancel flag again.
pub const POLL: Duration = Duration::from_millis(10);

/// How many distinct left-out characters or codes the status line names.
const LISTED: usize = 8;
/// Longest left-out code shown in full, in characters.
const CODE_CHARS: usize = 12;

/// Timing from the speed controls: `char_wpm` for the characters and,
/// with Farnsworth timing on, `effective_wpm` overall.
pub fn timing_for(char_wpm: f64, effective_wpm: Option<f64>) -> Timing {
    match effective_wpm {
        Some(effective_wpm) => Timing::farnsworth_wpm(char_wpm, effective_wpm),
        None => Timing::uniform(wpm_to_unit_ms(char_wpm)),
    }
}

/// The tone to render for an output device running at `device_rate`.
///
/// Rendering at the device's own rate leaves nothing to resample; a rate
/// the renderer does not support falls back to the default one, which the
/// audio backend then converts. The slider values are clamped to their
/// ranges, so nothing a control can hold makes the rendering fail.
pub fn tone_for(frequency_hz: f32, volume_percent: f32, device_rate: u32) -> Tone {
    let default = Tone::default();
    Tone {
        frequency_hz: frequency_hz.clamp(MIN_TONE_HZ, MAX_TONE_HZ),
        sample_rate: if (MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&device_rate) {
            device_rate
        } else {
            default.sample_rate
        },
        volume: (volume_percent / 100.0).clamp(0.0, 1.0),
        ..default
    }
}

/// Walk the lamp along `schedule`: lit for each tone, dark for each
/// silence, and dark at the end. Returns `false` if `cancel` was raised
/// before the schedule ran out.
///
/// Every step ends at its cumulative time on the `elapsed` clock, not
/// after a sleep of its own length, so a late wake-up is not carried into
/// the steps that follow and the lamp stays in step with audio rendered
/// from the same schedule. `wait` is asked for at most [`POLL`] at a time.
pub fn run_lamp(
    schedule: &[ScheduleStep],
    lamp: &AtomicBool,
    cancel: &AtomicBool,
    elapsed: impl Fn() -> Duration,
    mut wait: impl FnMut(Duration),
) -> bool {
    let mut end = Duration::ZERO;
    let mut completed = true;
    for step in schedule {
        if cancel.load(Ordering::SeqCst) {
            completed = false;
            break;
        }
        end = end.saturating_add(Duration::from_millis(step.duration_ms));
        lamp.store(step.is_tone(), Ordering::SeqCst);
        if !wait_until(end, cancel, &elapsed, &mut wait) {
            completed = false;
            break;
        }
    }
    lamp.store(false, Ordering::SeqCst);
    completed
}

/// Wait until `elapsed` reaches `deadline`, in slices of at most [`POLL`].
/// Returns `false` as soon as `cancel` is raised.
pub fn wait_until(
    deadline: Duration,
    cancel: &AtomicBool,
    elapsed: impl Fn() -> Duration,
    mut wait: impl FnMut(Duration),
) -> bool {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return false;
        }
        let now = elapsed();
        if now >= deadline {
            return true;
        }
        wait((deadline - now).min(POLL));
    }
}

/// Marks a transmission as running for as long as it lives. Dropping it,
/// which a panic in the transmit thread does too, clears the flag and
/// puts the lamp out, so the Transmit button cannot stay disabled.
pub struct TransmitGuard {
    is_transmitting: Arc<AtomicBool>,
    is_lit: Arc<AtomicBool>,
}

impl TransmitGuard {
    /// Raise `is_transmitting` until the guard is dropped.
    pub fn engage(is_transmitting: Arc<AtomicBool>, is_lit: Arc<AtomicBool>) -> Self {
        is_transmitting.store(true, Ordering::SeqCst);
        Self {
            is_transmitting,
            is_lit,
        }
    }
}

impl Drop for TransmitGuard {
    fn drop(&mut self) {
        self.is_lit.store(false, Ordering::SeqCst);
        self.is_transmitting.store(false, Ordering::SeqCst);
    }
}

/// The characters an encode left out, for the status line: each distinct
/// one once, in order. Anything that is not a letter, a digit or visible
/// ASCII is named by its code point, since it may not show as a glyph.
pub fn left_out_chars(skipped: &[char]) -> String {
    list_distinct(skipped.iter().map(|&c| {
        if c.is_alphanumeric() || c.is_ascii_graphic() {
            c.to_string()
        } else {
            format!("U+{:04X}", c as u32)
        }
    }))
}

/// The codes a decode left out, for the status line: each distinct one
/// once, in order, long ones cut short.
pub fn left_out_codes(skipped: &[String]) -> String {
    list_distinct(skipped.iter().map(|code| {
        let mut shown: String = code
            .chars()
            .take(CODE_CHARS)
            .map(|c| if c.is_control() { '\u{fffd}' } else { c })
            .collect();
        if code.chars().nth(CODE_CHARS).is_some() {
            shown.push('…');
        }
        shown
    }))
}

/// The first [`LISTED`] distinct items, space-separated, followed by a
/// count of the distinct items not listed.
fn list_distinct(items: impl Iterator<Item = String>) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut distinct: Vec<String> = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            distinct.push(item);
        }
    }
    let more = distinct.len().saturating_sub(LISTED);
    distinct.truncate(LISTED);
    let mut list = distinct.join(" ");
    if more > 0 {
        list.push_str(&format!(" … (+{more})"));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    use morse_core::{StepKind, build_schedule, build_signal_plan, schedule_duration_ms};

    #[test]
    fn speed_controls_map_to_timing() {
        // 20 WPM is a 60 ms unit; without Farnsworth the gaps match it.
        assert_eq!(timing_for(20.0, None), Timing::uniform(60));
        assert_eq!(timing_for(5.0, None), Timing::uniform(240));
        assert_eq!(timing_for(40.0, None), Timing::uniform(30));
        // Farnsworth 20/5 stretches the gap unit by the ARRL formula.
        let farnsworth = timing_for(20.0, Some(5.0));
        assert_eq!((farnsworth.char_unit_ms, farnsworth.gap_unit_ms), (60, 534));
        // An effective speed at or above the character speed is standard.
        assert_eq!(timing_for(20.0, Some(20.0)), Timing::uniform(60));
        assert_eq!(timing_for(20.0, Some(30.0)), Timing::uniform(60));
    }

    #[test]
    fn tone_controls_map_to_a_renderable_tone() {
        let tone = tone_for(DEFAULT_TONE_HZ, DEFAULT_VOLUME_PERCENT, 48_000);
        assert_eq!(tone.frequency_hz, 600.0);
        assert!((tone.volume - 0.20).abs() < 1e-6, "{}", tone.volume);
        assert_eq!(tone.sample_rate, 48_000);
        assert_eq!(tone.ramp_ms, Tone::default().ramp_ms);
        assert_eq!(tone.validate(), Ok(()));

        // The whole of both sliders is renderable at every device rate,
        // including rates the renderer does not take.
        for hz in [MIN_TONE_HZ, 777.0, MAX_TONE_HZ] {
            for percent in [0.0, 1.0, 50.0, 100.0] {
                for rate in [0, 4_000, 8_000, 44_100, 96_000, 192_000, 384_000, u32::MAX] {
                    let tone = tone_for(hz, percent, rate);
                    assert_eq!(tone.validate(), Ok(()), "{hz} Hz, {percent} %, {rate} Hz");
                    assert_eq!(tone.frequency_hz, hz);
                    assert!((tone.volume - percent / 100.0).abs() < 1e-6);
                }
            }
        }
        assert_eq!(tone_for(600.0, 20.0, 44_100).sample_rate, 44_100);
        assert_eq!(tone_for(600.0, 20.0, 384_000).sample_rate, 44_100);
        assert_eq!(tone_for(600.0, 20.0, 0).sample_rate, 44_100);
        // Values no slider can hold are pulled back into range.
        assert_eq!(tone_for(50.0, 20.0, 44_100).frequency_hz, MIN_TONE_HZ);
        assert_eq!(tone_for(9_000.0, 20.0, 44_100).frequency_hz, MAX_TONE_HZ);
        assert_eq!(tone_for(600.0, 250.0, 44_100).volume, 1.0);
        assert_eq!(tone_for(600.0, -5.0, 44_100).volume, 0.0);
    }

    /// A clock that only moves when the lamp waits, plus a record of
    /// every change of the lamp and when it happened.
    struct Bench {
        now: Cell<Duration>,
        lamp: AtomicBool,
        cancel: AtomicBool,
        changes: RefCell<Vec<(u64, bool)>>,
        longest_wait: Cell<Duration>,
    }

    impl Bench {
        fn new() -> Self {
            Self {
                now: Cell::new(Duration::ZERO),
                lamp: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
                changes: RefCell::new(Vec::new()),
                longest_wait: Cell::new(Duration::ZERO),
            }
        }

        /// Run the lamp. Each wait takes `overrun` longer than asked, and
        /// `cancel_at` raises the cancel flag once the clock reaches it.
        fn run(
            &self,
            schedule: &[ScheduleStep],
            overrun: Duration,
            cancel_at: Option<Duration>,
        ) -> bool {
            let lit = Cell::new(false);
            run_lamp(
                schedule,
                &self.lamp,
                &self.cancel,
                || {
                    // Sampled on every look at the clock, which the lamp
                    // takes right after each change.
                    let now = self.now.get();
                    let lamp = self.lamp.load(Ordering::SeqCst);
                    if lamp != lit.get() {
                        lit.set(lamp);
                        self.changes
                            .borrow_mut()
                            .push((now.as_millis() as u64, lamp));
                    }
                    now
                },
                |wait| {
                    self.longest_wait.set(self.longest_wait.get().max(wait));
                    self.now.set(self.now.get() + wait + overrun);
                    if cancel_at.is_some_and(|at| self.now.get() >= at) {
                        self.cancel.store(true, Ordering::SeqCst);
                    }
                },
            )
        }
    }

    #[test]
    fn lamp_follows_the_core_schedule() {
        let schedule = build_schedule(&build_signal_plan("AE E"), Timing::uniform(60));
        let bench = Bench::new();
        assert!(bench.run(&schedule, Duration::ZERO, None));
        // dot, gap, dash, letter gap, dot, word gap, dot.
        assert_eq!(
            *bench.changes.borrow(),
            vec![
                (0, true),
                (60, false),
                (120, true),
                (300, false),
                (480, true),
                (540, false),
                (960, true),
            ]
        );
        assert!(!bench.lamp.load(Ordering::SeqCst), "dark once done");
        assert_eq!(bench.now.get(), Duration::from_millis(1020));
        assert_eq!(schedule_duration_ms(&schedule), 1020);
        assert!(bench.longest_wait.get() <= POLL);
    }

    #[test]
    fn late_wake_ups_do_not_accumulate() {
        // Every wait overruns by 3 ms. Sleeping each step's own length
        // would end PARIS PARIS hundreds of ms late; against the clock,
        // every edge stays within one overrun of its scheduled time.
        let schedule = build_schedule(&build_signal_plan("PARIS PARIS"), Timing::uniform(60));
        let overrun = Duration::from_millis(3);
        let bench = Bench::new();
        assert!(bench.run(&schedule, overrun, None));

        let mut scheduled = Vec::new();
        let mut at = 0;
        for step in &schedule {
            scheduled.push((at, step.is_tone()));
            at += step.duration_ms;
        }
        let changes = bench.changes.borrow();
        assert_eq!(changes.len(), scheduled.len());
        for (&(seen_at, lit), &(due_at, due_lit)) in changes.iter().zip(&scheduled) {
            assert_eq!(lit, due_lit, "at {due_at} ms");
            assert!(
                seen_at >= due_at && seen_at <= due_at + POLL.as_millis() as u64 + 3,
                "edge due at {due_at} ms seen at {seen_at} ms"
            );
        }
        let end = bench.now.get().as_millis() as u64;
        assert!((at..=at + 13).contains(&end), "ended at {end} of {at} ms");
    }

    #[test]
    fn cancelling_stops_the_lamp_mid_step_and_leaves_it_dark() {
        // A 7-unit Farnsworth word gap is seconds long: Stop must not
        // have to wait for it.
        let schedule = build_schedule(&build_signal_plan("T T"), Timing::uniform(1_000));
        let bench = Bench::new();
        let cancel_at = Duration::from_millis(1_500);
        assert!(!bench.run(&schedule, Duration::ZERO, Some(cancel_at)));
        assert_eq!(bench.now.get(), cancel_at, "stopped within one poll");
        assert!(!bench.lamp.load(Ordering::SeqCst));
        assert_eq!(*bench.changes.borrow(), vec![(0, true)]);

        // Raised before the start: the lamp never lights.
        let bench = Bench::new();
        bench.cancel.store(true, Ordering::SeqCst);
        assert!(!bench.run(&schedule, Duration::ZERO, None));
        assert_eq!(bench.now.get(), Duration::ZERO);
        assert_eq!(*bench.changes.borrow(), vec![]);
        assert!(!bench.lamp.load(Ordering::SeqCst));
    }

    #[test]
    fn an_empty_or_instant_schedule_completes_at_once() {
        let bench = Bench::new();
        assert!(bench.run(&[], Duration::ZERO, None));
        let instant = [ScheduleStep {
            kind: StepKind::Tone,
            duration_ms: 0,
        }];
        assert!(bench.run(&instant, Duration::ZERO, None));
        assert_eq!(bench.now.get(), Duration::ZERO);
        assert!(!bench.lamp.load(Ordering::SeqCst));
        // A step too long for a `Duration` sum saturates instead of
        // panicking; cancelling still ends it.
        let endless = [ScheduleStep {
            kind: StepKind::Tone,
            duration_ms: u64::MAX,
        }; 3];
        assert!(!bench.run(&endless, Duration::ZERO, Some(Duration::from_millis(50))));
    }

    #[test]
    fn wait_until_stops_at_the_deadline_or_on_cancel() {
        let now = Cell::new(Duration::ZERO);
        let cancel = AtomicBool::new(false);
        let deadline = Duration::from_millis(25);
        let waits = RefCell::new(Vec::new());
        let reached = wait_until(
            deadline,
            &cancel,
            || now.get(),
            |wait| {
                waits.borrow_mut().push(wait.as_millis());
                now.set(now.get() + wait);
            },
        );
        assert!(reached);
        assert_eq!(*waits.borrow(), vec![10, 10, 5]);
        // Already past the deadline: no wait at all.
        assert!(wait_until(
            deadline,
            &cancel,
            || now.get(),
            |_| panic!("waited")
        ));
        cancel.store(true, Ordering::SeqCst);
        let later = Duration::from_secs(60);
        assert!(!wait_until(
            later,
            &cancel,
            || now.get(),
            |_| panic!("waited")
        ));
    }

    #[test]
    fn guard_clears_the_flags_when_dropped() {
        let is_transmitting = Arc::new(AtomicBool::new(false));
        let is_lit = Arc::new(AtomicBool::new(false));
        let guard = TransmitGuard::engage(is_transmitting.clone(), is_lit.clone());
        assert!(is_transmitting.load(Ordering::SeqCst));
        is_lit.store(true, Ordering::SeqCst);
        drop(guard);
        assert!(!is_transmitting.load(Ordering::SeqCst));
        assert!(!is_lit.load(Ordering::SeqCst));
    }

    #[test]
    fn guard_clears_the_flags_when_the_transmit_thread_panics() {
        let is_transmitting = Arc::new(AtomicBool::new(false));
        let is_lit = Arc::new(AtomicBool::new(false));
        let guard = TransmitGuard::engage(is_transmitting.clone(), is_lit.clone());
        let lamp = is_lit.clone();
        let worker = std::thread::spawn(move || {
            let _guard = guard;
            lamp.store(true, Ordering::SeqCst);
            // Unwinds like a panic, without the report on stderr.
            std::panic::resume_unwind(Box::new("transmit thread failed"));
        });
        assert!(worker.join().is_err());
        assert!(!is_transmitting.load(Ordering::SeqCst));
        assert!(!is_lit.load(Ordering::SeqCst));

        // A thread that never ran drops the guard with its closure.
        let guard = TransmitGuard::engage(is_transmitting.clone(), is_lit.clone());
        let never_run = move || drop(guard);
        assert!(is_transmitting.load(Ordering::SeqCst));
        drop(never_run);
        assert!(!is_transmitting.load(Ordering::SeqCst));
    }

    #[test]
    fn left_out_characters_are_listed_once_each_in_order() {
        assert_eq!(left_out_chars(&[]), "");
        assert_eq!(left_out_chars(&['~']), "~");
        assert_eq!(left_out_chars(&['~', '#', '~', 'Я']), "~ # Я");
        // Marks, controls and symbols that may have no glyph are named.
        assert_eq!(
            left_out_chars(&['\u{0303}', '\u{1b}', '😀', '\u{200d}']),
            "U+0303 U+001B U+1F600 U+200D"
        );
        let many: Vec<char> = ('a'..='z').collect();
        assert_eq!(left_out_chars(&many), "a b c d e f g h … (+18)");
        let exactly: Vec<char> = ('a'..='h').collect();
        assert_eq!(left_out_chars(&exactly), "a b c d e f g h");
    }

    #[test]
    fn left_out_codes_are_listed_once_each_and_cut_short() {
        let codes = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
        assert_eq!(left_out_codes(&[]), "");
        assert_eq!(
            left_out_codes(&codes(&["..--..--", "hello", "..--..--"])),
            "..--..-- hello"
        );
        assert_eq!(
            left_out_codes(&codes(&[
                "------------",
                "-------------",
                "привет-привет-привет"
            ])),
            "------------ ------------… привет-приве…"
        );
        // Control characters are not passed on to the status line.
        assert_eq!(left_out_codes(&codes(&["\u{1b}[2J"])), "\u{fffd}[2J");
        let many: Vec<String> = (0..11).map(|n| format!("x{n}")).collect();
        assert_eq!(left_out_codes(&many), "x0 x1 x2 x3 x4 x5 x6 x7 … (+3)");
    }
}
