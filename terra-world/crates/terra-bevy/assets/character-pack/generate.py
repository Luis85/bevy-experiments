#!/usr/bin/env python3
"""Original Terra Fieldhand articulated character. MIT OR Apache-2.0.

Metres, +Y up, -Z forward. Stand at Y=0; the seated clip puts the hip at
model origin. Python standard library only, reusing the original Comet GLB
writer. --check compares every generated output without writing any files.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("terra_original_glb", ROOT.parent / "transport-pack/generate.py")
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def ellipsoid(model, name, center, radius, material, parent, sides=10, rings=6):
    vertices = []
    for row in range(rings + 1):
        a = math.pi * row / rings
        for col in range(sides):
            b = math.tau * col / sides
            vertices.append(tuple(center[i] + radius[i] * value for i, value in
                                  enumerate((math.sin(a) * math.cos(b), math.cos(a), math.sin(a) * math.sin(b)))))
    faces = []
    for row in range(rings):
        for col in range(sides):
            nxt = (col + 1) % sides
            faces.append((row * sides + col, row * sides + nxt,
                          (row + 1) * sides + nxt, (row + 1) * sides + col))
    return model.faces(name, vertices, faces, material, parent)


def clip(model, name, duration, tracks):
    times = [duration * i / 24 for i in range(25)]
    time = model.accessor(times, 1, True)
    channels, samplers = [], []
    for node, prop, function in tracks:
        output = model.accessor([function(i / 24) for i in range(25)], 4 if prop == "rotation" else 3)
        channels.append({"sampler": len(samplers), "target": {"node": node, "path": prop}})
        samplers.append({"input": time, "output": output, "interpolation": "LINEAR"})
    model.gltf["animations"].append({"name": name, "channels": channels, "samplers": samplers})


def shoulder_pose(sign):
    # Y turns the hands toward the steering rim; X reaches forward. Keep the
    # standing anatomy and all authored body-style proportions unchanged.
    ax, ay = .92 / 2, sign * .55 / 2
    return [math.sin(ax) * math.cos(ay), math.cos(ax) * math.sin(ay),
            -math.sin(ax) * math.sin(ay), math.cos(ax) * math.cos(ay)]


def build():
    m = base.Model()
    m.gltf["asset"]["generator"] = "Terra original Fieldhand character authoring pipeline"
    m.gltf["scenes"][0]["name"] = "Fieldhand articulated character"
    m.gltf["nodes"][0]["name"] = "fieldhand"
    skin = m.material("character_skin", [1, 1, 1, 1], .86)
    hair = m.material("character_hair", [1, 1, 1, 1], .92)
    cloth = m.material("character_outfit", [1, 1, 1, 1], .94)
    pants = m.material("slate_trousers", [.20, .25, .27, 1], .95)
    shoes = m.material("dark_boots", [.13, .13, .12, 1], .9)
    trim = m.material("jacket_hardware", [.14, .18, .18, 1], .7)
    eyes = m.material("dark_eyes", [.10, .08, .065, 1], .7)
    hips = m.node("hips", position=(0, .85, 0))
    ellipsoid(m, "trouser_hips", (0, .02, 0), (.21, .14, .135), pants, hips)
    chest = m.node("chest", hips, (0, .25, 0))
    ellipsoid(m, "jacket_body", (0, .08, 0), (.235, .32, .155), cloth, chest, 12)
    m.box("jacket_zipper", (0, .10, -.155), (.022, .42, .018), trim, chest)
    for sign, side in [(-1, "left"), (1, "right")]:
        m.box(side + "_jacket_pocket", (sign * .12, -.04, -.142), (.13, .12, .025), cloth, chest)
    m.beam("neck", (0, .34, 0), (0, .45, 0), .065, skin, chest)
    head = m.node("head", chest, (0, .53, 0))
    ellipsoid(m, "face", (0, 0, 0), (.135, .175, .137), skin, head, 12)
    ellipsoid(m, "hair_cap", (0, .115, .015), (.143, .085, .14), hair, head, 12)
    ellipsoid(m, "nose", (0, -.015, -.137), (.025, .033, .03), skin, head, 8)
    for sign, side in [(-1, "left"), (1, "right")]:
        ellipsoid(m, side + "_eye", (sign * .048, .03, -.129), (.015, .012, .012), eyes, head, 6, 4)
        ellipsoid(m, side + "_ear", (sign * .133, 0, 0), (.028, .044, .025), skin, head, 6, 4)
    legs, knees, ankles, arms, elbows = [], [], [], [], []
    for sign, side in [(-1, "left"), (1, "right")]:
        leg = m.node(side + "_hip", hips, (sign * .115, 0, 0))
        legs.append(leg)
        ellipsoid(m, side + "_thigh", (0, -.19, 0), (.09, .24, .095), pants, leg)
        knee = m.node(side + "_knee", leg, (0, -.4, 0))
        knees.append(knee)
        ellipsoid(m, side + "_shin", (0, -.17, 0), (.075, .22, .08), pants, knee)
        ankle = m.node(side + "_ankle", knee, (0, -.375, 0))
        ankles.append(ankle)
        m.box(side + "_boot", (0, 0, -.055), (.165, .15, .29), shoes, ankle)
        arm = m.node(side + "_shoulder", chest, (sign * .235, .26, 0))
        arms.append(arm)
        ellipsoid(m, side + "_sleeve", (sign * .015, -.13, 0), (.075, .175, .075), cloth, arm)
        elbow = m.node(side + "_elbow", arm, (sign * .025, -.255, 0))
        elbows.append(elbow)
        ellipsoid(m, side + "_forearm", (0, -.105, 0), (.052, .135, .055), skin, elbow)
        ellipsoid(m, side + "_hand", (0, -.245, -.005), (.055, .075, .061), skin, elbow)
    rotation = lambda angle: base.quat_x(angle)
    clip(m, "idle", 3., [(chest, "rotation", lambda t: rotation(.015 * math.sin(math.tau * t))),
                           (head, "rotation", lambda t: rotation(-.01 * math.sin(math.tau * t)))])
    for name, duration, swing, bob in [("walk", 1., .46, .018), ("run", .62, .82, .035)]:
        tracks = [(hips, "translation", lambda t, bob=bob: [0, .85 + bob * (1 - math.cos(2 * math.tau * t)), 0]),
                  (chest, "rotation", lambda t, name=name: rotation(.12 if name == "run" else .025))]
        for i in range(2):
            phase = i * math.pi
            tracks.extend([
                (legs[i], "rotation", lambda t, p=phase, a=swing: rotation(a * math.sin(math.tau * t + p))),
                (knees[i], "rotation", lambda t, p=phase, a=swing: rotation(-max(0., math.sin(math.tau * t + p)) * a)),
                (arms[i], "rotation", lambda t, p=phase, a=swing: rotation(-a * .72 * math.sin(math.tau * t + p))),
                (elbows[i], "rotation", lambda t, name=name: rotation(.85 if name == "run" else .14)),
            ])
        clip(m, name, duration, tracks)
    tracks = [(hips, "translation", lambda t: [0, 0, 0]), (chest, "rotation", lambda t: rotation(-.07))]
    for i in range(2):
        tracks.extend([(legs[i], "rotation", lambda t: rotation(1.48)),
                       (knees[i], "rotation", lambda t: rotation(-.30)),
                       (ankles[i], "rotation", lambda t: rotation(-1.18)),
                       (arms[i], "rotation", lambda t, sign=(-1 if i == 0 else 1): shoulder_pose(sign)),
                       (elbows[i], "rotation", lambda t: rotation(2.2))])
    clip(m, "seated", 2., tracks)
    return m.bytes()


def driver_cabin():
    """A separate original variant; never rewrite the retained Comet source.

    Its empty high seat/floor prototype was not a fit-qualified human cabin.
    Lower the front cushion/floor and use a full-height seat back; all exterior,
    wheel, steering, material and animation payload bytes remain from Comet.
    """
    original = base.build()[0][base.ROOT / "comet.glb"]
    length = struct.unpack_from("<I", original, 12)[0]
    document = json.loads(original[20:20 + length])
    binary = original[28 + length:]
    document["asset"]["generator"] = "Terra original Comet driver-cabin variant"
    document["scenes"][0]["name"] = "Comet driver cabin"
    for node in document["nodes"]:
        name = node["name"]
        if name == "floor":
            node["translation"] = [0, -.38, 0]
            node["scale"] = [1, 1, 3. / 2.45]
            node["translation"][2] = -.1 * 3. / 2.45
        elif name == "painted-shell":
            # The original prototype was a closed shell even beneath the
            # cabin. Keep its retained mesh bytes, but replace its scene use
            # with an open cabin and a connected lowered floor pan.
            node.pop("mesh")
        elif name in ("left-front-seat-base", "right-front-seat-base"):
            node["translation"] = [0, -.34, 0]
        elif name in ("left-front-seat-back", "right-front-seat-back"):
            scale = .65 / .45
            node["scale"] = [1, scale, 1]
            node["translation"] = [0, .715 - .92 * scale, 0]
        elif name in ("left-front-headrest", "right-front-headrest"):
            node["translation"] = [0, -.095, 0]
    model = base.Model()
    model.gltf, model.binary = document, bytearray(binary)
    rings = [(-2.07, .72, .48, .70), (-1.82, .87, .38, .84),
             (-1.50, .874, .38, .863), (-.97, .88, .38, .90),
             (.99, .88, .38, .90), (1.50, .854, .406, .880),
             (1.77, .84, .42, .87), (2.04, .73, .47, .78)]
    vertices = []
    for z, width, low, high in rings:
        vertices.extend([(-width + .08, low, z), (width - .08, low, z), (width, low + .09, z),
                         (width, high - .06, z), (width - .07, high, z), (-width + .07, high, z),
                         (-width, high - .06, z), (-width, low + .09, z)])
    faces = [tuple(reversed(range(8))), tuple((len(rings) - 1) * 8 + i for i in range(8))]
    for j in range(len(rings) - 1):
        for i in range(8):
            if i == 4 and rings[j][0] >= -.97 and rings[j + 1][0] <= .99:
                continue  # open cabin top, between windscreen and rear seats
            if i == 0 and rings[j][0] >= -1.5 and rings[j + 1][0] <= 1.5:
                continue  # connect cabin/footwell through to the lowered pan
            faces.append((j * 8 + i, j * 8 + (i + 1) % 8, (j + 1) * 8 + (i + 1) % 8, (j + 1) * 8 + i))
    paint = next(i for i, m in enumerate(document["materials"]) if m["name"] == "original-petrol-teal-livery")
    trim = next(i for i, m in enumerate(document["materials"]) if m["name"] == "charcoal-trim")
    model.faces("driver-cabin-shell", vertices, faces, paint)
    for sign, side in [(-1, "left"), (1, "right")]:
        model.box("footwell-" + side, (sign * .775, .295, 0), (.035, .23, 3.), trim)
        model.box("footwell-end-" + side, (0, .295, sign * 1.5), (1.55, .23, .035), trim)
    return model.bytes()


def outputs():
    glb = build()
    cabin = driver_cabin()
    model = {"id": "fieldhand", "file": "fieldhand.glb", "sha256": hashlib.sha256(glb).hexdigest(), "scene": 0}
    binding = {"target": {"type": "kind", "value": "human"}, "model": "fieldhand", "scale": 1.,
               "ground_offset_m": 0., "yaw_offset_degrees": 0.,
               "animation": {"idle": "idle", "walk": "walk", "run": "run", "feed": "idle", "drink": "idle",
                             "walk_speed_m_s": 2., "run_speed_m_s": 5., "blend_seconds": .18,
                             "human": {"seated": "seated", "skin_material": "character_skin", "hair_material": "character_hair", "outfit_material": "character_outfit"}}}
    pack = {"schema_version": 1, "name": "Terra original Fieldhand character", "author": "Terra World contributors",
            "license": "MIT OR Apache-2.0", "models": [model], "bindings": [binding]}
    composed = json.loads((ROOT.parent / "transport-demo.pack.json").read_text())
    composed["name"] = "Terra original character transport and wildlife"
    composed["models"].append(dict(model, file="character-pack/fieldhand.glb"))
    for entry in composed["models"]:
        if entry["id"] == "comet":
            entry["file"] = "character-pack/comet-cabin.glb"
            entry["sha256"] = hashlib.sha256(cabin).hexdigest()
    composed["bindings"].append(copy.deepcopy(binding))
    for b in composed["bindings"]:
        if b["target"] == {"type": "vehicle_model", "value": "compact"}:
            b["driver_seat"] = {"translation_m": [-.42, .46, -.24]}
    recipe = copy.deepcopy(composed)
    recipe["name"] = "Terra authored character transport and wildlife"
    recipe["models"] = [{"id": m["id"], "source": m["file"], "scene": m["scene"],
                        "provenance": "Original Terra World procedural geometry and animation; MIT OR Apache-2.0. No downloaded art."}
                       for m in composed["models"]]
    encoded = lambda value: (json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n").encode()
    return {ROOT / "fieldhand.glb": glb, ROOT / "comet-cabin.glb": cabin, ROOT / "pack.json": encoded(pack),
            ROOT.parent / "character-demo.pack.json": encoded(composed),
            ROOT.parent / "character-authoring.recipe.json": encoded(recipe)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    files = []
    for path, content in outputs().items():
        if args.check:
            if not path.is_file() or path.read_bytes() != content:
                raise SystemExit(f"Generated content differs: {path}")
        else:
            path.write_bytes(content)
        files.append({"file": str(path.relative_to(ROOT.parent)), "bytes": len(content),
                      "sha256": hashlib.sha256(content).hexdigest()})
    print(json.dumps({"status": "matched" if args.check else "generated", "files": files}, indent=2))


if __name__ == "__main__":
    main()
