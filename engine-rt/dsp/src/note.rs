//! Note math: frequency ↔ MIDI ↔ name, cents.

/// Concert pitch used unless overridden (A4 = 440 Hz = MIDI 69).
pub const A4_HZ: f32 = 440.0;
const A4_MIDI: f32 = 69.0;

const SHARP_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Fractional MIDI number of `freq_hz` (69.0 = A4).
#[inline]
pub fn freq_to_midi(freq_hz: f32, a4_hz: f32) -> f32 {
    A4_MIDI + 12.0 * (freq_hz / a4_hz).log2()
}

/// Frequency of a (possibly fractional) MIDI number.
#[inline]
pub fn midi_to_freq(midi: f32, a4_hz: f32) -> f32 {
    a4_hz * ((midi - A4_MIDI) / 12.0).exp2()
}

/// Nearest equal-tempered note and the deviation from it in cents (−50..=+50).
#[inline]
pub fn nearest_note(freq_hz: f32, a4_hz: f32) -> (i32, f32) {
    let m = freq_to_midi(freq_hz, a4_hz);
    let n = m.round();
    (n as i32, (m - n) * 100.0)
}

/// Interval between two frequencies in cents (positive when `f` is above `reference`).
#[inline]
pub fn cents_between(f: f32, reference: f32) -> f32 {
    1200.0 * (f / reference).log2()
}

/// Pitch class, C = 0 … B = 11.
#[inline]
pub fn pitch_class(midi: i32) -> usize {
    midi.rem_euclid(12) as usize
}

/// Scientific pitch notation ("E2", "C#4"); MIDI 60 = C4.
pub fn note_name(midi: i32) -> String {
    format!("{}{}", SHARP_NAMES[pitch_class(midi)], midi.div_euclid(12) - 1)
}

/// Pitch-class name without octave ("E", "C#").
pub fn pitch_class_name(pc: usize) -> &'static str {
    SHARP_NAMES[pc % 12]
}

/// Parses "E2", "c#4", "Bb3", "F♯3" into a MIDI number.
pub fn parse_note(name: &str) -> Option<i32> {
    let mut chars = name.trim().chars().peekable();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut accidental = 0;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | '♯' => accidental += 1,
            'b' | '♭' => accidental -= 1,
            _ => break,
        }
        chars.next();
    }
    let octave: i32 = chars.collect::<String>().parse().ok()?;
    Some((octave + 1) * 12 + base + accidental)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_strings_map_to_standard_midi_numbers() {
        let open = [(82.41, 40), (110.0, 45), (146.83, 50), (196.0, 55), (246.94, 59), (329.63, 64)];
        for (f, midi) in open {
            let (n, cents) = nearest_note(f, A4_HZ);
            assert_eq!(n, midi, "{f} Hz");
            assert!(cents.abs() < 1.0, "{f} Hz is {cents} cents off");
        }
    }

    #[test]
    fn names_round_trip() {
        for midi in 0..128 {
            assert_eq!(parse_note(&note_name(midi)), Some(midi));
        }
        assert_eq!(note_name(40), "E2");
        assert_eq!(parse_note("Bb3"), Some(58));
        assert_eq!(parse_note("F♯3"), Some(54));
        assert_eq!(parse_note("H2"), None);
    }

    #[test]
    fn frequency_round_trip_and_cents() {
        for midi in 28..100 {
            let f = midi_to_freq(midi as f32, A4_HZ);
            assert!((freq_to_midi(f, A4_HZ) - midi as f32).abs() < 1e-4);
        }
        assert!((cents_between(445.0, 440.0) - 19.56).abs() < 0.01);
        assert!((midi_to_freq(69.0, 432.0) - 432.0).abs() < 1e-3);
    }
}
