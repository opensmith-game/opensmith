/// The last few seconds of raw guitar input, addressed by input stream index.
pub struct TapHistory {
    buf: Vec<f32>,
    mask: u64,
    /// Index of the next sample to arrive.
    end: u64,
}

impl TapHistory {
    /// `capacity` is rounded up to a power of two.
    pub fn new(capacity: usize) -> TapHistory {
        let n = capacity.next_power_of_two();
        TapHistory { buf: vec![0.0; n], mask: n as u64 - 1, end: 0 }
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    /// Stores `samples` starting at stream index `first`; a gap is filled with silence.
    pub fn push(&mut self, first: u64, samples: &[f32]) {
        let cap = self.buf.len() as u64;
        if first > self.end {
            for i in self.end.max(first.saturating_sub(cap))..first {
                self.buf[(i & self.mask) as usize] = 0.0;
            }
        }
        for (k, &v) in samples.iter().enumerate() {
            let i = first + k as u64;
            if i >= self.end || self.end - i <= cap {
                self.buf[(i & self.mask) as usize] = v;
            }
        }
        self.end = self.end.max(first + samples.len() as u64);
    }

    /// Copies samples `start..start + out.len()`; false if they are not all held.
    pub fn read(&self, start: u64, out: &mut [f32]) -> bool {
        let stop = start + out.len() as u64;
        if stop > self.end || self.end - start > self.buf.len() as u64 {
            return false;
        }
        for (k, o) in out.iter_mut().enumerate() {
            *o = self.buf[((start + k as u64) & self.mask) as usize];
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_most_recent_samples_and_fills_gaps() {
        let mut h = TapHistory::new(8);
        h.push(0, &[1.0, 2.0, 3.0]);
        h.push(5, &[6.0, 7.0]);
        let mut out = [0.0; 4];
        assert!(h.read(2, &mut out));
        assert_eq!(out, [3.0, 0.0, 0.0, 6.0]);
        assert!(!h.read(5, &mut [0.0; 3]), "sample 7 has not arrived");
        h.push(7, &[8.0; 9]);
        assert!(!h.read(0, &mut [0.0; 2]), "overwritten");
        assert!(h.read(8, &mut out));
        assert_eq!(out, [8.0; 4]);
    }
}
