//! Guitar fretboard model: tunings, (string, fret) ↔ MIDI, candidate positions, string colours.
//!
//! String 0 is the low E string, string 5 the high e.

pub const NUM_STRINGS: usize = 6;
pub const MAX_FRET: u8 = 24;

/// A fretted (or open, `fret == 0`) position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FretPos {
    pub string: u8,
    pub fret: u8,
}

impl FretPos {
    pub const fn new(string: u8, fret: u8) -> Self {
        Self { string, fret }
    }
}

/// Open-string pitches, low E first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuning {
    pub open_midi: [i32; NUM_STRINGS],
}

impl Tuning {
    pub const STANDARD: Tuning = Tuning { open_midi: [40, 45, 50, 55, 59, 64] };
    pub const DROP_D: Tuning = Tuning { open_midi: [38, 45, 50, 55, 59, 64] };
    pub const EB_STANDARD: Tuning = Tuning { open_midi: [39, 44, 49, 54, 58, 63] };

    /// Builds a tuning from each string's offset in semitones relative to E standard.
    pub fn from_offsets(offsets: [i32; NUM_STRINGS]) -> Self {
        let mut open_midi = Self::STANDARD.open_midi;
        for (m, o) in open_midi.iter_mut().zip(offsets) {
            *m += o;
        }
        Self { open_midi }
    }

    #[inline]
    pub fn midi_at(&self, pos: FretPos) -> i32 {
        self.open_midi[pos.string as usize] + pos.fret as i32
    }

    /// Every position (fret ≤ `max_fret`) that produces `midi`, lowest fret first. Allocation-free.
    pub fn positions(&self, midi: i32, max_fret: u8) -> Positions {
        let mut out = Positions { items: [FretPos::new(0, 0); NUM_STRINGS], len: 0 };
        // Higher strings give lower frets for the same pitch.
        for string in (0..NUM_STRINGS).rev() {
            let fret = midi - self.open_midi[string];
            if (0..=max_fret as i32).contains(&fret) {
                out.items[out.len] = FretPos::new(string as u8, fret as u8);
                out.len += 1;
            }
        }
        out
    }

    /// Chooses the position of `midi` nearest to the fretting hand (`hand_fret`, 0 = open
    /// position). Open strings cost nothing; ties go to the lower fret.
    pub fn resolve(&self, midi: i32, hand_fret: u8) -> Option<FretPos> {
        self.positions(midi, MAX_FRET)
            .as_slice()
            .iter()
            .copied()
            .min_by_key(|p| {
                let cost = if p.fret == 0 { 0 } else { (p.fret as i32 - hand_fret as i32).abs() };
                (cost, p.fret)
            })
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Fixed-capacity list of candidate positions.
#[derive(Clone, Copy, Debug)]
pub struct Positions {
    items: [FretPos; NUM_STRINGS],
    len: usize,
}

impl Positions {
    pub fn as_slice(&self) -> &[FretPos] {
        &self.items[..self.len]
    }
}

/// String colour on the highway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StringColor {
    pub name: &'static str,
    pub rgb: [u8; 3],
}

/// Highway palette, string 0 (low E) first: red, yellow, blue, orange, green, purple.
pub const STRING_COLORS: [StringColor; NUM_STRINGS] = [
    StringColor { name: "red", rgb: [0xE8, 0x2A, 0x2E] },
    StringColor { name: "yellow", rgb: [0xF4, 0xD0, 0x2C] },
    StringColor { name: "blue", rgb: [0x25, 0x8C, 0xEF] },
    StringColor { name: "orange", rgb: [0xF5, 0x8B, 0x1F] },
    StringColor { name: "green", rgb: [0x35, 0xC7, 0x4B] },
    StringColor { name: "purple", rgb: [0xA4, 0x4E, 0xE8] },
];

/// Display names of the strings, string 0 first.
pub const STRING_NAMES: [&str; NUM_STRINGS] = ["E", "A", "D", "G", "B", "e"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_strings_resolve_to_themselves() {
        let t = Tuning::STANDARD;
        for s in 0..NUM_STRINGS {
            let pos = t.resolve(t.open_midi[s], 0).unwrap();
            assert_eq!(pos, FretPos::new(s as u8, 0));
        }
    }

    #[test]
    fn e4_has_six_positions() {
        let p = Tuning::STANDARD.positions(64, 24);
        assert_eq!(
            p.as_slice(),
            &[
                FretPos::new(5, 0),
                FretPos::new(4, 5),
                FretPos::new(3, 9),
                FretPos::new(2, 14),
                FretPos::new(1, 19),
                FretPos::new(0, 24)
            ]
        );
        // E2 only exists on the open low E.
        assert_eq!(Tuning::STANDARD.positions(40, 24).as_slice(), &[FretPos::new(0, 0)]);
        assert!(Tuning::STANDARD.positions(39, 24).as_slice().is_empty());
    }

    #[test]
    fn hand_position_drives_resolution() {
        let t = Tuning::STANDARD;
        // C4 (60): B string fret 1, G fret 5, D fret 10, A fret 15, E fret 20.
        assert_eq!(t.resolve(60, 0), Some(FretPos::new(4, 1)));
        assert_eq!(t.resolve(60, 5), Some(FretPos::new(3, 5)));
        assert_eq!(t.resolve(60, 9), Some(FretPos::new(2, 10)));
        assert_eq!(Tuning::from_offsets([-2, 0, 0, 0, 0, 0]), Tuning::DROP_D);
    }
}
