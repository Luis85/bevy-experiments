"""Original Lastlight ambience beds and legacy source cues, synthesized with the stdlib.

No recordings, samples or external assets are inputs. Fixed seeds and
cycle-complete oscillators make regeneration repeatable.

Owned files: the five ambient_* loops in assets/audio (shipped beds) and the
four legacy cues in archive/audio (archived, not shipped, by production-pass
decision 10). The nine clips of scripts/generate_audio.py (ambience in
assets/audio, the other eight in archive/audio) are read only, for provenance
and the preservation check. This script is the one writer of the two Python
manifests: assets/audio/PROVENANCE.json (the six beds) and
archive/audio/PROVENANCE.json (the twelve archived clips).
assets/audio/SURVIVAL_PROVENANCE.json belongs to the Rust exporter and is never
read or written here.

--check (the default) renders into a temporary directory outside the
workspace, compares every owned clip and both manifests with the committed
bytes, writes nothing and exits 1 on any mismatch. --write regenerates the
owned clips and both manifests in place. Run it as
python -I -B scripts/generate_atmosphere_audio.py [--check | --write].
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import sys
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets" / "audio"
ARCHIVE = ROOT / "archive" / "audio"
RATE = 22_050
TAU = math.tau
# scripts/generate_audio.py's clips, read only, and the folder each lives in.
ORIGINAL = {"shot": ARCHIVE, "reload": ARCHIVE, "melee": ARCHIVE, "damage": ARCHIVE, "boss": ARCHIVE, "ambience": ASSETS, "incoming": ARCHIVE, "ready": ARCHIVE, "structure": ARCHIVE}
# Owned: the loops ship in assets/audio, the cues are archived.
LOOPS = {"ambient_wind": (16.0, 301), "ambient_urban": (20.0, 302), "ambient_nature": (24.0, 303), "ambient_rain": (18.0, 304), "ambient_cold": (22.0, 305)}
CUES = {"npc_shot": (0.26, 401), "turret_shot": (0.20, 402), "trap": (0.65, 403), "armor": (0.15, 404)}
OWNED = {**{name: ASSETS for name in LOOPS}, **{name: ARCHIVE for name in CUES}}
ARCHIVED = "2026-11-06"
ARCHIVE_NOTE = "Not shipped; retained by production-pass decision 10; restore with git mv"
ARCHIVE_REASON = "Not loaded at runtime; archived by production-pass decision 10 (user archive list)"
SEAM_VALIDATION = "Boundary one-sample change does not exceed the larger of 0.004 full scale and the clip's 99th-percentile interior one-sample change; cycle-complete oscillators and cyclic IIR memory avoid a special seam impulse."


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_digest(path):
    """The LF-normalised SHA-256 of a text source, the same on every checkout."""
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


def chirp(t, onset, duration, pitch, rise, gain):
    delta = t - onset
    if not 0.0 <= delta < duration:
        return 0.0
    envelope = math.sin(math.pi * delta / duration) ** 2
    phase = TAU * (pitch * delta + 0.5 * rise * delta * delta / duration)
    return gain * envelope * (math.sin(phase) + 0.18 * math.sin(phase * 2.0))


def impulse(t, onset, pitch, decay, gain, noise):
    delta = t - onset
    if delta < 0.0:
        return 0.0
    return (math.sin(TAU * pitch * delta) * 0.72 + noise * 0.28) * math.exp(-delta * decay) * gain


def render_loop(name, seconds, seed):
    frames = round(seconds * RATE)
    # Warm the two IIR filters through one complete deterministic noise period.
    # Their memory at the beginning/end then belongs to the same cyclic signal.
    low = middle = 0.0
    rng = random.Random(seed)
    for _ in range(frames):
        noise = rng.uniform(-1.0, 1.0)
        low = low * 0.985 + noise * 0.015
        middle = middle * 0.88 + noise * 0.12
    rng = random.Random(seed)
    values = []
    for index in range(frames):
        t = index / RATE
        noise = rng.uniform(-1.0, 1.0)
        low = low * 0.985 + noise * 0.015
        middle = middle * 0.88 + noise * 0.12
        band = middle - low
        phase = TAU * t / seconds
        if name == "ambient_wind":
            gust = 0.62 + 0.22 * math.sin(phase) + 0.10 * math.sin(phase * 3.0 + 0.8)
            value = (low * 0.74 + band * 0.20) * gust
        elif name == "ambient_urban":
            value = low * 0.30 + band * 0.035
            value += math.sin(TAU * 48.0 * t) * 0.018 + math.sin(TAU * 72.0 * t) * 0.007
            value += chirp(t, 4.3, 0.42, 410.0, -70.0, 0.018)
            value += chirp(t, 13.8, 0.34, 535.0, -95.0, 0.013)
        elif name == "ambient_nature":
            value = low * 0.19 + band * 0.025
            for onset, duration, pitch, rise, gain in [(1.9, 0.17, 1880.0, 530.0, 0.15), (2.2, 0.12, 2300.0, -450.0, 0.11), (8.6, 0.21, 1690.0, 690.0, 0.16), (14.1, 0.14, 2470.0, -620.0, 0.12), (20.7, 0.19, 2110.0, 570.0, 0.14)]:
                value += chirp(t, onset, duration, pitch, rise, gain)
        elif name == "ambient_rain":
            drizzle = 0.81 + 0.10 * math.sin(phase * 2.0) + 0.07 * math.sin(phase * 5.0)
            value = (band * 0.47 + (noise - middle) * 0.075 + low * 0.18) * drizzle
            # Quiet periodic roof/water detail, rather than random danger cues.
            value += chirp(t, 3.4, 0.045, 1150.0, -250.0, 0.035)
            value += chirp(t, 11.2, 0.035, 1430.0, -310.0, 0.025)
        else:
            gust = 0.60 + 0.24 * math.sin(phase * 2.0 + 0.5)
            value = (low * 0.60 + band * 0.14) * gust
            value += math.sin(TAU * 320.0 * t + 0.35 * math.sin(phase)) * 0.006 * (0.5 + 0.5 * math.sin(phase))
        # Source headroom makes the default 15% ambience bus audible at rest,
        # while normalized scene weights keep it behind the combat-effects bus.
        values.append(value * (2.0 if name == "ambient_rain" else 3.0))
    return values


def render_cue(name, seconds, seed):
    rng = random.Random(seed)
    values = []
    low = 0.0
    for index in range(round(seconds * RATE)):
        t = index / RATE
        noise = rng.uniform(-1.0, 1.0)
        low = low * 0.94 + noise * 0.06
        if name == "npc_shot":
            value = noise * math.exp(-t * 95.0) * 0.39
            value += math.sin(TAU * (170.0 * t - 180.0 * t * t)) * math.exp(-t * 27.0) * 0.29
            value += impulse(t, 0.075, 1730.0, 100.0, 0.075, noise)
        elif name == "turret_shot":
            value = impulse(t, 0.004, 128.0, 37.0, 0.40, noise)
            value += impulse(t, 0.042, 830.0, 110.0, 0.14, noise)
            value += (noise - low) * math.exp(-t * 65.0) * 0.15
        elif name == "trap":
            value = impulse(t, 0.014, 1380.0, 115.0, 0.12, noise)
            delta = t - 0.045
            if delta > 0.0:
                value += (math.sin(TAU * (63.0 * delta - 17.0 * delta * delta)) * 0.31 + noise * 0.34) * math.exp(-delta * 17.0)
            for onset, pitch in [(0.14, 2370.0), (0.22, 1780.0), (0.31, 3120.0)]:
                value += impulse(t, onset, pitch, 85.0, 0.085, noise)
        else:
            value = (math.sin(TAU * 2180.0 * t) * 0.15 + math.sin(TAU * 3410.0 * t) * 0.075 + (noise - low) * 0.055) * math.exp(-t * 43.0)
        edge = min(1.0, t / 0.002, (seconds - t) / 0.015)
        values.append(value * max(0.0, edge))
    return values


def write_clip(path, values):
    assert values and all(math.isfinite(value) and abs(value) < 0.95 for value in values), path.name
    payload = b"".join(struct.pack("<h", round(value * 32767)) for value in values)
    with wave.open(str(path), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(RATE)
        audio.writeframes(payload)


def render_owned(destination):
    """Render every owned clip; destination(name) is the folder to write it in."""
    for name, (seconds, seed) in LOOPS.items():
        write_clip(destination(name) / f"{name}.wav", render_loop(name, seconds, seed))
    for name, (seconds, seed) in CUES.items():
        write_clip(destination(name) / f"{name}.wav", render_cue(name, seconds, seed))


def inspect(path):
    with wave.open(str(path), "rb") as audio:
        assert (audio.getnchannels(), audio.getsampwidth(), audio.getframerate(), audio.getcomptype()) == (1, 2, RATE, "NONE"), path.name
        frames = audio.getnframes()
        values = struct.unpack(f"<{frames}h", audio.readframes(frames))
    peak = max(abs(value) for value in values)
    assert frames > 0 and 0 < peak < 32767, path.name
    return {"file": path.name, "seconds": frames / RATE, "frames": frames, "peak": peak, "rms": math.sqrt(sum(value * value for value in values) / frames) / 32767.0, "boundary_step": abs(values[-1] - values[0]) / 32767.0, "sha256": digest(path)}


def record(name, path):
    entry = inspect(path)
    if name in LOOPS:
        with wave.open(str(path), "rb") as audio:
            values = struct.unpack(f"<{audio.getnframes()}h", audio.readframes(audio.getnframes()))
        differences = sorted(abs(b - a) / 32767.0 for a, b in zip(values, values[1:]))
        limit = max(0.004, differences[int(len(differences) * 0.99)])
        assert entry["boundary_step"] <= limit, (path.name, entry["boundary_step"], limit)
        entry["seam_limit"] = limit
    entry["generator"] = "scripts/generate_atmosphere_audio.py" if name in OWNED else "scripts/generate_audio.py"
    entry["seed"] = (LOOPS.get(name) or CUES.get(name) or (0.0, 12))[1]
    if (OWNED.get(name) or ORIGINAL.get(name)) == ARCHIVE:
        entry["bytes"] = path.stat().st_size
        entry["archived_from"] = f"assets/audio/{name}.wav"
        entry["archived"] = ARCHIVED
        entry["reason"] = ARCHIVE_REASON
    return entry


def manifests(location, originals):
    """Both Python manifests as {path: text}; location(name) is the clip to inspect."""
    header = {"authored": "2026-11-02", "license": "MIT (repository original assets)", "inputs": "Python standard-library synthesis; no recordings or third-party samples", "sample_rate": RATE, "channels": 1, "bits": 16, "generator_sha256": source_digest(Path(__file__))}
    texts = {}
    for folder in (ASSETS, ARCHIVE):
        names = sorted(name for name in (*ORIGINAL, *OWNED) if (OWNED.get(name) or ORIGINAL.get(name)) == folder)
        provenance = dict(header)
        if folder == ASSETS:
            provenance["loop_seam_validation"] = SEAM_VALIDATION
        else:
            provenance["note"] = ARCHIVE_NOTE
        provenance["preserved_originals"] = {name: sha for name, sha in originals.items() if ORIGINAL[name] == folder}
        provenance["files"] = [record(name, location(name)) for name in names]
        texts[folder / "PROVENANCE.json"] = json.dumps(provenance, indent=2) + "\n"
    return texts


def committed(path):
    return path.read_bytes().replace(b"\r\n", b"\n") if path.is_file() else None


def check():
    failures = []
    missing = [f"{folder / name}.wav" for name, folder in ORIGINAL.items() if not (folder / f"{name}.wav").is_file()]
    if missing:
        print("FAIL: missing generate_audio.py clips: " + ", ".join(missing))
        return 1
    originals = {name: digest(folder / f"{name}.wav") for name, folder in ORIGINAL.items()}
    with tempfile.TemporaryDirectory(prefix="lastlight-atmosphere-") as temp:
        temp = Path(temp)
        render_owned(lambda name: temp)
        for name, folder in OWNED.items():
            path = folder / f"{name}.wav"
            if not path.is_file():
                failures.append(f"{path.relative_to(ROOT)}: missing")
            elif digest(path) != digest(temp / f"{name}.wav"):
                failures.append(f"{path.relative_to(ROOT)}: rendered sha256 differs")
        texts = manifests(lambda name: temp / f"{name}.wav" if name in OWNED else ORIGINAL[name] / f"{name}.wav", originals)
        for path, text in texts.items():
            if committed(path) != text.encode("utf-8"):
                failures.append(f"{path.relative_to(ROOT)}: differs from the rendered manifest")
    for failure in failures:
        print("FAIL: " + failure)
    if failures:
        print("Run with --write to regenerate, then review the diff.")
        return 1
    print(f"PASS: {len(OWNED)} owned clips and both Python manifests are byte-identical to a fresh render; nothing written.")
    return 0


def write():
    for folder in (ASSETS, ARCHIVE):
        folder.mkdir(parents=True, exist_ok=True)
    originals = {name: digest(folder / f"{name}.wav") for name, folder in ORIGINAL.items()}
    render_owned(lambda name: OWNED[name])
    assert all(digest(ORIGINAL[name] / f"{name}.wav") == sha for name, sha in originals.items()), "Existing audio changed"
    texts = manifests(lambda name: (OWNED.get(name) or ORIGINAL[name]) / f"{name}.wav", originals)
    for path, text in texts.items():
        path.write_bytes(text.encode("utf-8"))
    print("PASS: generate_audio.py's nine clips are byte-identical; all clips are mono 16-bit 22,050 Hz PCM, unclipped, and loop seams pass.")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="render to a temp dir and compare (default)")
    mode.add_argument("--write", action="store_true", help="regenerate the owned clips and both manifests")
    arguments = parser.parse_args(argv)
    return write() if arguments.write else check()


if __name__ == "__main__":
    sys.exit(main())
