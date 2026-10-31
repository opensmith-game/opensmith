# Note detector

A small network (about 11 000 weights) that the game asks, around an attack, which notes were
just played: chords strummed with other strings still ringing, notes the pitch tracker missed.
It runs in the engine as plain Rust (`engine-rt/dsp/src/notenet.rs`), a few milliseconds a call,
on the same features as here (`engine-rt/dsp/src/notefeat.rs`).

## Data, free of anyone's rights

- **Training**: pieces the composer invents (`engine-rt/session/src/composer.rs`: strummed
  chords, arpeggios, riffs, scale lines with hammer-ons, slides, bends, harmonics), played by the
  virtual guitarist on single notes recorded from real guitars in
  [Guitar-TECHS](https://guitar-techs.github.io) (CC BY 4.0); and the real playing of
  Guitar-TECHS (chords, scales, techniques, excerpts), its annotations cleaned of the MIDI
  pickup's ghosts and moved onto the real attacks, 30 % of every batch.
- **Test**: one player's real playing, the network trained on the two others: 77.9 % of the
  notes found (75.5 % trained on invented pieces only). The weights shipped learnt from all
  three.
- **Your own guitar**: the recording sessions of the Lessons screen give takes of the player's
  guitar and cable; `models/notenet/own_takes.py` moves each note onto its real attack and
  leaves out the ones not played, ready for the sample bank and for training.

## Rebuilding it

1. Download Guitar-TECHS from [Zenodo](https://zenodo.org/records/14963133), unzip it and turn
   its items into takes with `models/notenet/guitartechs.py` (direct input as the cable carries
   it, chart from the per-string MIDI).
2. Make the data (about 8 GB for 25 hours of playing):

   ```bash
   cargo run --release -p opensmith-session --bin opensmith-dataset -- --bank <takes> --out <data>/train --pieces 1500 --seconds 60 --seed 1
   ```

   then `--out <data>/val --pieces 100 --seed 999`, and `--takes <takes> --out <data>/real`.
3. Train (about an hour on an M1 Max), which writes the weights and a check file the Rust test
   compares against:

   ```bash
   uv run --project pipeline-ingest python models/notenet/train.py <data> engine-rt/dsp/assets/notenet.bin 20000 --learn-from P1,P2,P3
   ```

   `--learn-from P1,P3` keeps player 2 for the test; `--measure <weights>` only measures.

4. Measure the game's judging with it: `opensmith-simulate --takes <takes>` (real playing) and
   `opensmith-simulate --real-notes <takes> --player good <charts>`.
