"""The learned note detector: around an attack (40 ms before, 120 ms after), which notes were
just attacked, MIDI 28 to 95. Trained on pieces the composer invents, played by the virtual
guitarist on notes recorded from real guitars (`opensmith-dataset`), and on real playing: the
real takes whose names start with `--learn-from` (players, e.g. P1,P3) are a share of every
batch, the others are the test.

Features come from `opensmith_dsp::notefeat` (the game's own code): 216 bins, three per semitone
from MIDI 26, a frame every 10 ms. The network is small enough to run in the game, in Rust
(`opensmith_dsp::notenet` reads the weights this script exports).

uv run --project pipeline-ingest python models/notenet/train.py <data folder> <weights out> [steps]
    [--channels 16] [--learn-from P1,P2,P3] [--measure weights.bin]
"""

import argparse

import json
import struct
import sys
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn

BINS, PER_SEMITONE, LOW_MIDI = 216, 3, 26
BEFORE, AFTER = 4, 12  # frames around the attack
FIRST, PITCHES = 28, 68  # MIDI 28 to 95
NEAR = 0.035  # a note started within this of the attack counts as attacked there (s)
REAL_SHARE = 0.3  # of each batch, from the real takes learned from
HARMONICS = (0.5, 1, 2, 3, 4, 5)
SHIFTS = [round(36 * np.log2(h)) for h in HARMONICS]


class Pieces:
    """Every piece of a folder: features (memory-mapped) and the notes attacked."""

    def __init__(self, folder: Path, keep=lambda stem: True):
        self.items = []
        for meta in sorted(folder.glob("*.json")):
            if not keep(meta.stem):
                continue
            doc = json.loads(meta.read_text())
            feats = np.memmap(meta.with_suffix(".f32"), dtype="<f4", mode="r").reshape(-1, BINS)
            notes = [(n["start"], int(round(n["pitch"])), n["kind"]) for n in doc["notes"]]
            attacks = sorted({round(s, 3) for s, _, _ in notes})
            self.items.append((meta.stem, feats, notes, attacks))

    def window(self, k: int, centre: float) -> np.ndarray:
        feats = self.items[k][1]
        c = int(round(centre / 0.01))
        out = np.zeros((BEFORE + AFTER, BINS), dtype=np.float32)
        lo, hi = c - BEFORE, c + AFTER
        a, b = max(lo, 0), min(hi, len(feats))
        if b > a:
            out[a - lo : b - lo] = feats[a:b]
        return out

    def target(self, k: int, centre: float) -> np.ndarray:
        y = np.zeros(PITCHES, dtype=np.float32)
        for s, p, kind in self.items[k][2]:
            if kind != "dead" and abs(s - centre) <= NEAR and FIRST <= p < FIRST + PITCHES:
                y[p - FIRST] = 1.0
        return y

    def batch(self, rng: np.random.Generator, n: int):
        """Windows at attacks (give or take 25 ms), near misses (50 to 120 ms from one: what is
        attacked a little later or earlier is not attacked here) and anywhere; each shifted by
        up to a third of a semitone (a guitar a little out of tune)."""
        xs, ys = [], []
        for _ in range(n):
            k = int(rng.integers(len(self.items)))
            attacks = self.items[k][3]
            r = rng.random()
            if attacks and r < 0.5:
                centre = attacks[int(rng.integers(len(attacks)))] + rng.uniform(-0.025, 0.025)
            elif attacks and r < 0.7:
                centre = attacks[int(rng.integers(len(attacks)))] + rng.choice([-1, 1]) * rng.uniform(0.05, 0.12)
            else:
                centre = rng.uniform(0.2, len(self.items[k][1]) * 0.01 - 0.2)
            x = self.window(k, centre)
            shift = int(rng.integers(-1, 2))
            if shift:
                x = np.roll(x, shift, axis=1)
            xs.append(x)
            ys.append(self.target(k, centre))
        return torch.from_numpy(np.stack(xs)), torch.from_numpy(np.stack(ys))


class NoteNet(nn.Module):
    def __init__(self, ch: int = 16):
        super().__init__()
        self.c1 = nn.Conv2d(len(HARMONICS), ch, (3, 5), padding=(1, 2))
        self.c2 = nn.Conv2d(ch, ch, (3, 5), padding=(1, 2))
        self.c3 = nn.Conv2d(ch, ch, (BEFORE + AFTER, 1))
        self.c4 = nn.Conv1d(ch, ch, 5, padding=2)
        self.c5 = nn.Conv1d(ch, 1, 1)

    def forward(self, x):  # x: (batch, frames, bins)
        stacks = []
        for s in SHIFTS:
            if s >= 0:
                stacks.append(torch.nn.functional.pad(x[:, :, s:], (0, s)))
            else:
                stacks.append(torch.nn.functional.pad(x[:, :, :s], (-s, 0)))
        h = torch.relu(self.c1(torch.stack(stacks, 1)))
        h = torch.relu(self.c2(h))
        h = torch.relu(self.c3(h)).squeeze(2)  # (batch, ch, bins)
        h = torch.relu(self.c4(h))
        h = self.c5(h).squeeze(1)  # (batch, bins)
        h = torch.nn.functional.max_pool1d(h.unsqueeze(1), PER_SEMITONE).squeeze(1)  # (batch, semitones)
        return h[:, FIRST - LOW_MIDI : FIRST - LOW_MIDI + PITCHES]


def evaluate(model, data: Pieces, device, limit: int = 4000):
    """Pitch-set F1 at the true attacks, threshold 0.5."""
    model.eval()
    tp = fp = fn = 0
    xs, ys = [], []
    for k in range(len(data.items)):
        for a in data.items[k][3]:
            xs.append(data.window(k, a))
            ys.append(data.target(k, a))
    if not xs:
        return float("nan"), 0
    idx = np.random.default_rng(0).permutation(len(xs))[:limit]
    with torch.no_grad():
        for i in range(0, len(idx), 512):
            sel = idx[i : i + 512]
            x = torch.from_numpy(np.stack([xs[j] for j in sel])).to(device)
            y = np.stack([ys[j] for j in sel])
            p = (torch.sigmoid(model(x)).cpu().numpy() >= 0.5).astype(np.float32)
            tp += (p * y).sum()
            fp += (p * (1 - y)).sum()
            fn += ((1 - p) * y).sum()
    model.train()
    return 2 * tp / max(2 * tp + fp + fn, 1), len(idx)


def export(model, path: Path):
    """Weights for `opensmith_dsp::notenet`: a header line of JSON (layer shapes), then every
    tensor as little-endian f32, in order."""
    tensors = [(name, t.detach().cpu().numpy().astype("<f4")) for name, t in model.state_dict().items()]
    header = json.dumps({"harmonics": SHIFTS, "before": BEFORE, "after": AFTER, "first": FIRST, "pitches": PITCHES, "tensors": [[n, list(t.shape)] for n, t in tensors]})
    with open(path, "wb") as f:
        f.write(struct.pack("<I", len(header)))
        f.write(header.encode())
        for _, t in tensors:
            f.write(t.tobytes())


def load(path: Path) -> NoteNet:
    """A network from weights `export` wrote."""
    raw = path.read_bytes()
    size = struct.unpack("<I", raw[:4])[0]
    header = json.loads(raw[4 : 4 + size])
    model = NoteNet(header["tensors"][0][1][0])
    at, state = 4 + size, {}
    for name, shape in header["tensors"]:
        count = int(np.prod(shape))
        state[name] = torch.from_numpy(np.frombuffer(raw, dtype="<f4", count=count, offset=at).reshape(shape).copy())
        at += 4 * count
    model.load_state_dict(state)
    return model


def check(model, data: Pieces, path: Path, device):
    """A few windows and what the network says of them, for the Rust side to compare: count,
    then each window's features and its probabilities, little-endian f32."""
    model.eval()
    rng = np.random.default_rng(7)
    picks = []
    for _ in range(4):
        k = int(rng.integers(len(data.items)))
        attacks = data.items[k][3] or [1.0]
        picks.append((k, attacks[int(rng.integers(len(attacks)))]))
    xs = np.stack([data.window(k, a) for k, a in picks])
    with torch.no_grad():
        ys = torch.sigmoid(model(torch.from_numpy(xs).to(device))).cpu().numpy()
    model.train()
    with open(path, "wb") as f:
        f.write(struct.pack("<I", len(xs)))
        for x, y in zip(xs, ys):
            f.write(x.astype("<f4").tobytes())
            f.write(y.astype("<f4").tobytes())


def main(folder: Path, out: Path, steps: int, channels: int = 16, learn_from: str = "", measure: Path | None = None):
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    players = tuple(p for p in learn_from.split(",") if p)
    train, val = Pieces(folder / "train"), Pieces(folder / "val")
    learned = Pieces(folder / "real", keep=lambda stem: bool(players) and stem.startswith(players))
    real = Pieces(folder / "real", keep=lambda stem: not (players and stem.startswith(players)))
    print(f"{len(train.items)} pieces and {len(learned.items)} real takes to learn from, {len(val.items)} pieces and {len(real.items)} real takes to check ({device})", flush=True)
    if measure is not None:
        model = load(measure).to(device)
        f_val, _ = evaluate(model, val, device)
        f_real, n_real = evaluate(model, real, device)
        print(f"{measure.name}: val F1 {100 * f_val:.1f} real F1 {100 * f_real:.1f} ({n_real} attacks)")
        return
    model = NoteNet(channels).to(device)
    print(sum(p.numel() for p in model.parameters()), "parameters", flush=True)
    opt = torch.optim.AdamW(model.parameters(), lr=2e-3, weight_decay=1e-4)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=3e-3, total_steps=steps)
    loss_fn = nn.BCEWithLogitsLoss(pos_weight=torch.tensor(4.0, device=device))
    rng = np.random.default_rng(1)
    mixed = round(256 * REAL_SHARE) if learned.items else 0
    best, t0 = -1.0, time.time()
    for step in range(1, steps + 1):
        x, y = train.batch(rng, 256 - mixed)
        if mixed:
            xr, yr = learned.batch(rng, mixed)
            x, y = torch.cat([x, xr]), torch.cat([y, yr])
        loss = loss_fn(model(x.to(device)), y.to(device))
        opt.zero_grad()
        loss.backward()
        opt.step()
        sched.step()
        if step % 500 == 0 or step == steps:
            f_val, _ = evaluate(model, val, device)
            f_real, n_real = evaluate(model, real, device)
            print(f"step {step} loss {loss.item():.4f} val F1 {100 * f_val:.1f} real F1 {100 * f_real:.1f} ({n_real} attacks) {time.time() - t0:.0f} s", flush=True)
            # Learning from real playing, the invented pieces no longer tell which step is best:
            # the last one (the learning rate down to nothing) is kept.
            if f_val > best or (mixed and step == steps):
                best = f_val
                export(model, out)
                check(model, val, out.with_suffix(".check"), device)
    print(f"best val F1 {100 * best:.1f}, weights in {out}")


if __name__ == "__main__":
    args = argparse.ArgumentParser(description="Trains the note detector and exports its weights.")
    args.add_argument("data", type=Path)
    args.add_argument("out", type=Path)
    args.add_argument("steps", type=int, nargs="?", default=20000)
    args.add_argument("--channels", type=int, default=16)
    args.add_argument("--learn-from", default="", help="real takes to learn from: name prefixes (players), comma-separated")
    args.add_argument("--measure", type=Path, help="only measure these weights")
    a = args.parse_args()
    main(a.data, a.out, a.steps, a.channels, a.learn_from, a.measure)
