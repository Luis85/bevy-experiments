"""Original Lastlight scene layers/source cues, synthesized with the stdlib.

No recordings, samples or external assets are inputs. Existing nine clips are
read only for provenance and preservation checks; this script writes nine new
files. Fixed seeds and cycle-complete oscillators make regeneration repeatable.
"""
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import wave

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "assets" / "audio"
RATE = 22_050
TAU = math.tau
ORIGINAL = ("shot", "reload", "melee", "damage", "boss", "ambience", "incoming", "ready", "structure")
LOOPS = {"ambient_wind": (16.0, 301), "ambient_urban": (20.0, 302), "ambient_nature": (24.0, 303), "ambient_rain": (18.0, 304), "ambient_cold": (22.0, 305)}
CUES = {"npc_shot": (0.26, 401), "turret_shot": (0.20, 402), "trap": (0.65, 403), "armor": (0.15, 404)}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


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


def write_clip(name, values):
    assert values and all(math.isfinite(value) and abs(value) < 0.95 for value in values), name
    payload = b"".join(struct.pack("<h", round(value * 32767)) for value in values)
    path = OUTPUT / f"{name}.wav"
    with wave.open(str(path), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(RATE)
        audio.writeframes(payload)


def inspect(path):
    with wave.open(str(path), "rb") as audio:
        assert (audio.getnchannels(), audio.getsampwidth(), audio.getframerate(), audio.getcomptype()) == (1, 2, RATE, "NONE"), path.name
        frames = audio.getnframes()
        values = struct.unpack(f"<{frames}h", audio.readframes(frames))
    peak = max(abs(value) for value in values)
    assert frames > 0 and 0 < peak < 32767, path.name
    return {"file": path.name, "seconds": frames / RATE, "frames": frames, "peak": peak, "rms": math.sqrt(sum(value * value for value in values) / frames) / 32767.0, "boundary_step": abs(values[-1] - values[0]) / 32767.0, "sha256": digest(path)}


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    originals = {name: digest(OUTPUT / f"{name}.wav") for name in ORIGINAL}
    for name, (seconds, seed) in LOOPS.items():
        write_clip(name, render_loop(name, seconds, seed))
    for name, (seconds, seed) in CUES.items():
        write_clip(name, render_cue(name, seconds, seed))
    assert all(digest(OUTPUT / f"{name}.wav") == sha for name, sha in originals.items()), "Existing audio changed"
    records = []
    for path in sorted(OUTPUT.glob("*.wav")):
        record = inspect(path)
        if path.stem in LOOPS:
            with wave.open(str(path), "rb") as audio:
                values = struct.unpack(f"<{audio.getnframes()}h", audio.readframes(audio.getnframes()))
            differences = sorted(abs(b - a) / 32767.0 for a, b in zip(values, values[1:]))
            limit = max(0.004, differences[int(len(differences) * 0.99)])
            assert record["boundary_step"] <= limit, (path.name, record["boundary_step"], limit)
            record["seam_limit"] = limit
        record["generator"] = "scripts/generate_atmosphere_audio.py" if path.stem in LOOPS or path.stem in CUES else "scripts/generate_audio.py"
        record["seed"] = (LOOPS.get(path.stem) or CUES.get(path.stem) or (0.0, 12))[1]
        records.append(record)
    provenance = {"authored": "2026-11-02", "license": "MIT (repository original assets)", "inputs": "Python standard-library synthesis; no recordings or third-party samples", "sample_rate": RATE, "channels": 1, "bits": 16, "generator_sha256": digest(Path(__file__)), "loop_seam_validation": "Boundary one-sample change does not exceed the larger of 0.004 full scale and the clip's 99th-percentile interior one-sample change; cycle-complete oscillators and cyclic IIR memory avoid a special seam impulse.", "preserved_originals": originals, "files": records}
    (OUTPUT / "PROVENANCE.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(records, indent=2))
    print("PASS: original nine WAV files are byte-identical; all clips are mono 16-bit 22,050 Hz PCM, unclipped, and new loop seams pass.")


if __name__ == "__main__":
    main()
