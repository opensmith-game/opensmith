//! The learned note detector: from the features (`notefeat`) of 40 ms before to 120 ms after an
//! attack, how likely each note from MIDI 28 to 95 was just attacked. A small convolutional
//! network over harmonics (`models/notenet/train.py` trains it and exports its weights), run
//! here as plain loops: no runtime to ship, a few milliseconds a call. Not for the audio
//! callback: the session calls it when it judges a chord or a note.

use crate::notefeat::{BINS, PER_SEMITONE};

#[derive(Debug, thiserror::Error)]
pub enum NoteNetError {
    #[error("note detector weights unreadable: {0}")]
    Format(String),
}

#[derive(serde::Deserialize)]
struct Header {
    harmonics: Vec<i32>,
    before: usize,
    after: usize,
    first: i32,
    pitches: usize,
    tensors: Vec<(String, Vec<usize>)>,
}

struct Conv {
    w: Vec<f32>,
    b: Vec<f32>,
    out: usize,
    inp: usize,
    kt: usize,
    kb: usize,
}

pub struct NoteNet {
    shifts: Vec<i32>,
    /// Frames before and after the attack the network looks at.
    pub before: usize,
    pub after: usize,
    /// Lowest MIDI note it answers for, and how many.
    pub first: i32,
    pub pitches: usize,
    layers: Vec<Conv>,
}

impl NoteNet {
    /// Reads weights as `train.py` exports them.
    pub fn from_bytes(bytes: &[u8]) -> Result<NoteNet, NoteNetError> {
        let bad = |m: &str| NoteNetError::Format(m.to_string());
        let len = u32::from_le_bytes(bytes.get(..4).ok_or_else(|| bad("too short"))?.try_into().unwrap()) as usize;
        let header: Header = serde_json::from_slice(bytes.get(4..4 + len).ok_or_else(|| bad("header"))?).map_err(|e| bad(&e.to_string()))?;
        let mut at = 4 + len;
        let mut next = |count: usize| -> Result<Vec<f32>, NoteNetError> {
            let raw = bytes.get(at..at + 4 * count).ok_or_else(|| bad("weights cut short"))?;
            at += 4 * count;
            Ok(raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
        };
        let mut layers = Vec::new();
        let mut it = header.tensors.iter();
        while let (Some((_, ws)), Some((_, bs))) = (it.next(), it.next()) {
            let w = next(ws.iter().product())?;
            let b = next(bs.iter().product())?;
            let (out, inp) = (ws[0], ws[1]);
            let (kt, kb) = if ws.len() == 4 { (ws[2], ws[3]) } else { (1, ws[2]) };
            layers.push(Conv { w, b, out, inp, kt, kb });
        }
        if layers.len() != 5 {
            return Err(bad("expected five layers"));
        }
        Ok(NoteNet { shifts: header.harmonics, before: header.before, after: header.after, first: header.first, pitches: header.pitches, layers })
    }

    /// Frames it needs: `before + after`.
    pub fn frames(&self) -> usize {
        self.before + self.after
    }

    /// Probability that each note `first ..first + pitches` was attacked, from `window`
    /// (`frames()` × `BINS` features, the attack at frame `before`).
    pub fn attacked(&self, window: &[f32]) -> Vec<f32> {
        let t = self.frames();
        assert_eq!(window.len(), t * BINS);
        // Harmonic stack: channel h holds, at the bin of a pitch, the bin of its h-th harmonic.
        let mut x = vec![0.0f32; self.shifts.len() * t * BINS];
        for (h, &s) in self.shifts.iter().enumerate() {
            for f in 0..t {
                for b in 0..BINS {
                    let src = b as i32 + s;
                    if (0..BINS as i32).contains(&src) {
                        x[(h * t + f) * BINS + b] = window[f * BINS + src as usize];
                    }
                }
            }
        }
        let x = conv2d(&self.layers[0], &x, t, true);
        let x = conv2d(&self.layers[1], &x, t, true);
        // Over all the frames at once: one row per channel.
        let c3 = &self.layers[2];
        let mut y = vec![0.0f32; c3.out * BINS];
        for o in 0..c3.out {
            let row = &mut y[o * BINS..(o + 1) * BINS];
            row.fill(c3.b[o]);
            for i in 0..c3.inp {
                for f in 0..c3.kt {
                    let w = c3.w[(o * c3.inp + i) * c3.kt + f];
                    let src = &x[(i * t + f) * BINS..(i * t + f + 1) * BINS];
                    for (r, s) in row.iter_mut().zip(src) {
                        *r += w * s;
                    }
                }
            }
            row.iter_mut().for_each(|v| *v = v.max(0.0));
        }
        let y = conv2d(&self.layers[3], &y, 1, true);
        let y = conv2d(&self.layers[4], &y, 1, false);
        // Best of each semitone's bins, then the notes answered for.
        let skip = (self.first - crate::notefeat::LOW_MIDI as i32) as usize;
        (0..self.pitches)
            .map(|p| {
                let k = (skip + p) * PER_SEMITONE;
                let v = y[k..k + PER_SEMITONE].iter().fold(f32::MIN, |a, &b| a.max(b));
                1.0 / (1.0 + (-v).exp())
            })
            .collect()
    }
}

/// A convolution over (frames × `BINS`) planes, `same` padding, optionally rectified.
fn conv2d(c: &Conv, x: &[f32], frames: usize, relu: bool) -> Vec<f32> {
    let (pt, pb) = (c.kt / 2, c.kb / 2);
    let mut y = vec![0.0f32; c.out * frames * BINS];
    for o in 0..c.out {
        for f in 0..frames {
            let row = &mut y[(o * frames + f) * BINS..(o * frames + f + 1) * BINS];
            row.fill(c.b[o]);
            for i in 0..c.inp {
                for dt in 0..c.kt {
                    let src_f = f as isize + dt as isize - pt as isize;
                    if src_f < 0 || src_f >= frames as isize {
                        continue;
                    }
                    let src = &x[(i * frames + src_f as usize) * BINS..(i * frames + src_f as usize + 1) * BINS];
                    for db in 0..c.kb {
                        let w = c.w[((o * c.inp + i) * c.kt + dt) * c.kb + db];
                        let shift = db as isize - pb as isize;
                        let (lo, hi) = ((-shift).max(0) as usize, (BINS as isize - shift.max(0)) as usize);
                        for b in lo..hi {
                            row[b] += w * src[(b as isize + shift) as usize];
                        }
                    }
                }
            }
            if relu {
                row.iter_mut().for_each(|v| *v = v.max(0.0));
            }
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The network gives what PyTorch gave for the same windows (`assets/notenet.check`,
    /// written by `train.py` with the weights).
    #[test]
    fn the_rust_network_answers_like_the_trained_one() {
        let net = NoteNet::from_bytes(include_bytes!("../assets/notenet.bin")).unwrap();
        let check = include_bytes!("../assets/notenet.check");
        let floats: Vec<f32> = check[4..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        let n = u32::from_le_bytes(check[..4].try_into().unwrap()) as usize;
        let (win, out) = (net.frames() * BINS, net.pitches);
        for k in 0..n {
            let at = k * (win + out);
            let got = net.attacked(&floats[at..at + win]);
            let want = &floats[at + win..at + win + out];
            let worst = got.iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-4, "window {k}: off by {worst}");
        }
    }
}
