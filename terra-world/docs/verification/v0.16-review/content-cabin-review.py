#!/usr/bin/env python3
"""Independent mesh evidence for the 0.16 human/Comet cabin review.

Reads the composed pack and its immutable GLBs directly; it does not call the
asset generator or change source assets. Measurements are authored geometry on
flat ground, not native screenshot, comfort, collision-authority or GPU evidence.
Only the supported unskinned node-transform sample is evaluated here.
"""
import argparse
import bisect
import hashlib
import json
import math
import struct
from pathlib import Path


def add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def mul(a, scale):
    return tuple(x * scale for x in a)


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(a):
    return mul(a, 1 / math.sqrt(dot(a, a)))


def matrix(translation, rotation, scale):
    x, y, z, w = unit(rotation)
    r = ((1 - 2 * (y*y + z*z), 2 * (x*y - z*w), 2 * (x*z + y*w)),
         (2 * (x*y + z*w), 1 - 2 * (x*x + z*z), 2 * (y*z - x*w)),
         (2 * (x*z - y*w), 2 * (y*z + x*w), 1 - 2 * (x*x + y*y)))
    return tuple(tuple(r[i][j] * scale[j] for j in range(3)) + (translation[i],) for i in range(3)) + ((0, 0, 0, 1),)


IDENTITY = matrix((0, 0, 0), (0, 0, 0, 1), (1, 1, 1))


def compose(a, b):
    return tuple(tuple(sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)) for i in range(4))


def point(m, p):
    return tuple(sum(m[i][k] * p[k] for k in range(3)) + m[i][3] for i in range(3))


def bounds(points):
    return [[min(p[i] for p in points) for i in range(3)], [max(p[i] for p in points) for i in range(3)]]


def overlap(a, b):
    return all(a[0][i] <= b[1][i] + 1e-8 and b[0][i] <= a[1][i] + 1e-8 for i in range(3))


def segment_hits_triangle(start, end, triangle):
    # Moller-Trumbore segment test; coplanar overlap is deliberately excluded.
    # This report's closed-shell crossings are transverse surfaces.
    a, b, c = triangle
    direction, ab, ac = sub(end, start), sub(b, a), sub(c, a)
    h = cross(direction, ac)
    determinant = dot(ab, h)
    if abs(determinant) < 1e-10:
        return False
    inv = 1 / determinant
    s = sub(start, a)
    u = inv * dot(s, h)
    if u < -1e-8 or u > 1 + 1e-8:
        return False
    q = cross(s, ab)
    v = inv * dot(direction, q)
    t = inv * dot(ac, q)
    return v >= -1e-8 and u + v <= 1 + 1e-8 and -1e-8 <= t <= 1 + 1e-8


def intersects(a, b):
    if not overlap(bounds(a), bounds(b)):
        return False
    return any(segment_hits_triangle(a[i], a[(i + 1) % 3], b) or
               segment_hits_triangle(b[i], b[(i + 1) % 3], a) for i in range(3))


class Glb:
    def __init__(self, path):
        self.path = path
        self.bytes = path.read_bytes()
        assert self.bytes[:4] == b"glTF"
        offset, chunks = 12, {}
        while offset < len(self.bytes):
            length, kind = struct.unpack_from("<I4s", self.bytes, offset)
            chunks[kind] = self.bytes[offset + 8:offset + 8 + length]
            offset += 8 + length
        self.root, self.binary = json.loads(chunks[b"JSON"]), chunks[b"BIN\0"]
        self.cache = {}

    def accessor(self, index):
        if index in self.cache:
            return self.cache[index]
        a = self.root["accessors"][index]
        v = self.root["bufferViews"][a["bufferView"]]
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        code = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}[a["componentType"]]
        fmt = "<" + code * width
        stride = v.get("byteStride", struct.calcsize(fmt))
        start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        rows = [struct.unpack_from(fmt, self.binary, start + i * stride) for i in range(a["count"])]
        self.cache[index] = rows
        return rows

    def clip(self, name):
        return next(a for a in self.root.get("animations", []) if a["name"] == name)

    def clip_times(self, name):
        return sorted({row[0] for s in self.clip(name)["samplers"] for row in self.accessor(s["input"])})

    def triangles(self, name=None, time=0, outer=IDENTITY):
        nodes = self.root["nodes"]
        poses = [{"translation": n.get("translation", [0, 0, 0]),
                  "rotation": n.get("rotation", [0, 0, 0, 1]),
                  "scale": n.get("scale", [1, 1, 1])} for n in nodes]
        if name:
            clip = self.clip(name)
            for channel in clip["channels"]:
                sampler = clip["samplers"][channel["sampler"]]
                times = [t[0] for t in self.accessor(sampler["input"])]
                values = self.accessor(sampler["output"])
                upper = min(max(1, bisect.bisect_right(times, time)), len(times) - 1)
                lower = upper - 1
                if time >= times[-1]:
                    value = values[-1]
                elif sampler.get("interpolation", "LINEAR") == "STEP":
                    value = values[lower]
                else:
                    weight = max(0, min(1, (time - times[lower]) / (times[upper] - times[lower])))
                    a, b = values[lower], values[upper]
                    if channel["target"]["path"] == "rotation":
                        a, b = unit(a), unit(b)
                        cosine = dot(a, b)
                        if cosine < 0:
                            b, cosine = mul(b, -1), -cosine
                        if cosine < .9995:
                            angle = math.acos(max(-1, min(1, cosine)))
                            value = add(mul(a, math.sin((1 - weight) * angle) / math.sin(angle)),
                                        mul(b, math.sin(weight * angle) / math.sin(angle)))
                        else:
                            value = unit(add(mul(a, 1 - weight), mul(b, weight)))
                    else:
                        value = add(mul(a, 1 - weight), mul(b, weight))
                poses[channel["target"]["node"]][channel["target"]["path"]] = value
        result = {}

        def visit(index, parent):
            node, pose = nodes[index], poses[index]
            transform = compose(parent, matrix(pose["translation"], pose["rotation"], pose["scale"]))
            if "mesh" in node:
                triangles = result.setdefault(node["name"], [])
                for primitive in self.root["meshes"][node["mesh"]]["primitives"]:
                    assert primitive.get("mode", 4) == 4
                    vertices = [point(transform, p) for p in self.accessor(primitive["attributes"]["POSITION"])]
                    indices = [r[0] for r in self.accessor(primitive["indices"])] if "indices" in primitive else list(range(len(vertices)))
                    triangles.extend(tuple(vertices[i] for i in indices[j:j + 3]) for j in range(0, len(indices), 3))
            for child in node.get("children", []):
                visit(child, transform)

        for index in self.root["scenes"][self.root.get("scene", 0)]["nodes"]:
            visit(index, outer)
        return result


def binding_transform(binding, profile=(1, 1, 1), seated=False):
    yaw = math.radians(binding.get("yaw_offset_degrees", 0))
    return matrix((0, 0 if seated else binding.get("ground_offset_m", 0), 0),
                  (0, math.sin(yaw / 2), 0, math.cos(yaw / 2)),
                  tuple(v * binding.get("scale", 1) for v in profile))


def flatten(triangles):
    return [p for triangle in triangles for p in triangle]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pack", type=Path, default=Path(__file__).resolve().parents[3] / "crates/terra-bevy/assets/character-demo.pack.json")
    args = parser.parse_args()
    manifest = json.loads(args.pack.read_text())
    human_binding = next(b for b in manifest["bindings"] if b["target"] == {"type": "kind", "value": "human"})
    cabin_binding = next(b for b in manifest["bindings"] if b["target"] == {"type": "vehicle_model", "value": "compact"})
    models = {m["id"]: m for m in manifest["models"]}
    human, cabin = [Glb(args.pack.parent / models[b["model"]]["file"]) for b in [human_binding, cabin_binding]]
    for glb, binding in [(human, human_binding), (cabin, cabin_binding)]:
        assert hashlib.sha256(glb.bytes).hexdigest() == models[binding["model"]]["sha256"]
    cabin_meshes = cabin.triangles(outer=binding_transform(cabin_binding))
    roof = bounds(flatten(cabin_meshes["roof-panel"]))
    floor = bounds(flatten(cabin_meshes["floor"]))
    wheel = set(flatten(cabin_meshes["cabin-steering-rim"]))
    shell_parts = {name: triangles for name, triangles in cabin_meshes.items()
                   if name in ["painted-shell", "driver-cabin-shell"] or name.startswith("footwell-")}
    shell = [triangle for triangles in shell_parts.values() for triangle in triangles]
    solid_checks = {name: triangles for name, triangles in cabin_meshes.items()
                    if name in ["roof-panel", "floor", "dashboard", "windscreen", "left-side-glazing", "right-side-glazing"]}
    anchor = point(binding_transform(cabin_binding), cabin_binding["driver_seat"]["translation_m"])
    clip = human_binding["animation"]["human"]["seated"]
    times = human.clip_times(clip)
    samples = sorted(set(times + [(a + b) / 2 for a, b in zip(times, times[1:])]))
    report = {"scope": "direct immutable GLB geometry; seated keyframes and interval midpoints; flat ground; no runtime collision/comfort/GPU claim",
              "human_sha256": hashlib.sha256(human.bytes).hexdigest(), "cabin_sha256": hashlib.sha256(cabin.bytes).hexdigest(),
              "sample_count": len(samples), "hip_anchor_m": anchor, "roof_bounds_m": roof, "floor_bounds_m": floor, "profiles": []}
    for name, scale in [("balanced", (1, 1, 1)), ("broad", (1.14, 1.02, 1.12)), ("slender", (.88, 1.06, .94))]:
        outer = compose(matrix(anchor, (0, 0, 0, 1), (1, 1, 1)), binding_transform(human_binding, scale, seated=True))
        sampled = [human.triangles(clip, time, outer) for time in samples]
        # Keep the full sampling contract but avoid repeating expensive spatial
        # queries for identical poses (this authored seated clip is static).
        unique_poses = {tuple(p for part in sorted(pose) for p in flatten(pose[part])): pose for pose in sampled}
        poses = list(unique_poses.values())
        vertices = [p for pose in poses for triangles in pose.values() for p in flatten(triangles)]
        head = [p for pose in poses for part in ["face", "hair_cap"] for p in flatten(pose[part])]
        boots = [p for pose in poses for part in ["left_boot", "right_boot"] for p in flatten(pose[part])]
        hands = {side: set(p for pose in poses for p in flatten(pose[side + "_hand"])) for side in ["left", "right"]}
        crossed = sorted({part for pose in poses for part, triangles in pose.items()
                          if any(intersects(a, b) for a in triangles for b in shell)})
        cabin_crossings = {name: sorted({part for pose in poses for part, triangles in pose.items()
                                        if overlap(bounds(flatten(triangles)), bounds(flatten(obstacle)))
                                        and any(intersects(a, b) for a in triangles for b in obstacle)})
                           for name, obstacle in solid_checks.items()}
        report["profiles"].append({"name": name, "body_scale": scale, "distinct_sampled_poses": len(poses), "bounds_m": bounds(vertices), "head_max_y_m": max(p[1] for p in head),
                                   "roof_clearance_m": roof[0][1] - max(p[1] for p in head), "boot_min_y_m": min(p[1] for p in boots),
                                   "floor_clearance_m": min(p[1] for p in boots) - floor[1][1], "transverse_shell_or_footwell_crossings": crossed,
                                   "transverse_cabin_crossings": cabin_crossings,
                                   "hand_to_rim_vertex_distance_m": {side: math.sqrt(min(dot(sub(p, q), sub(p, q)) for p in points for q in wheel)) for side, points in hands.items()},
                                   "hand_bounds_m": {side: bounds(points) for side, points in hands.items()}})
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
