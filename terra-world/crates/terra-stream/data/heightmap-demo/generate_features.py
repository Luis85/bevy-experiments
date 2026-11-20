"""Rebuild the original, synthetic semantic companion without changing v1 data.

No downloads or inferred real-world geography. Coordinates are WGS84 solely to
exercise the import contract. This script never rewrites height.png/manifest.json.
"""
from pathlib import Path
import json
import math

folder = Path(__file__).parent
manifest = json.loads((folder / "manifest.json").read_text())
bounds = manifest["bounds"]

def position(u, v):
    return [round(bounds["west"] + u * (bounds["east"] - bounds["west"]), 10),
            round(bounds["north"] - v * (bounds["north"] - bounds["south"]), 10)]

features = []
def polygon(name, uv, properties):
    ring = [position(u, v) for u, v in uv]
    features.append({"type": "Feature", "id": name,
                     "properties": {"name": name.replace("-", " ").title(), **properties},
                     "geometry": {"type": "Polygon", "coordinates": [ring + [ring[0]]]}})

# Broken woodland contours leave clear valley corridors and preserve the
# authored settlement's immediate streets. Each exterior is simple and bounded.
for name, u, v, rx, ry, leaf in [
    ("west-ridge-wood", .23, .27, .13, .17, "needleleaved"),
    ("western-foothills", .29, .60, .11, .14, "mixed"),
    ("east-ridge-wood", .80, .32, .12, .19, "needleleaved"),
    ("eastern-foothills", .75, .64, .12, .10, "mixed"),
    ("south-valley-wood", .50, .85, .16, .08, "broadleaved"),
    ("village-copse", .552, .592, .018, .022, "broadleaved"),
    ("north-copse", .555, .492, .023, .022, "mixed"),
]:
    points = []
    for i in range(12):
        angle = i * math.tau / 12
        taper = 1 - (i % 3) * .09
        points.append((u + math.cos(angle) * rx * taper, v + math.sin(angle) * ry * taper))
    polygon(name, points, {"landuse": "forest", "leaf_type": leaf, "height": 12 if leaf == "needleleaved" else 9})

for name, u, v, width, height, landuse, crop in [
    ("south-wheat-field", .45, .60, .035, .038, "farmland", "wheat"),
    ("south-barley-field", .49, .65, .047, .04, "farmland", "barley"),
    ("east-orchard", .59, .55, .035, .045, "orchard", "apple"),
    ("west-pasture", .40, .48, .033, .05, "meadow", "grass"),
    ("valley-meadow", .45, .72, .11, .05, "meadow", "grass"),
]:
    polygon(name, [(u,v),(u+width,v),(u+width,v+height),(u,v+height)], {"landuse": landuse, "crop": crop})

# Independent specimen points exercise ground placement and broadleaf variants.
for i, (u, v) in enumerate([(.493,.527),(.497,.531),(.526,.569),(.529,.573),(.479,.565),(.475,.569)]):
    features.append({"type":"Feature","id":f"valley-tree-{i}",
                     "properties":{"natural":"tree","leaf_type":"broadleaved","height":8+i%3},
                     "geometry":{"type":"Point","coordinates":position(u,v)}})

(folder / "features-v2.geojson").write_text(json.dumps({"type":"FeatureCollection","features":features}, indent=2) + "\n")
manifest["revision"] = "terra-authored-heightmap-demo-v2"
manifest["features"] = {
    "geojson": "features-v2.geojson",
    "source": {
        "provider": "Terra authored valley land cover",
        "license": "MIT OR Apache-2.0",
        "attribution": "Original synthetic forests, fields, orchard and trees; not observed land use around Munich.",
        "url": "urn:terra:heightmap:valley-features:v2",
        "retrieved_unix_seconds": 0,
        "synthetic": True,
    },
}
(folder / "manifest-v2.json").write_text(json.dumps(manifest, indent=2) + "\n")
