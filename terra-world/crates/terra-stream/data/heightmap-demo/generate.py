"""Regenerate the explicitly synthetic, georeferenced 16-bit elevation fixture."""
from pathlib import Path
import json, math, struct
from PIL import Image
folder = Path(__file__).parent
side = 257
values = []
for y in range(side):
    v = y / (side - 1)
    for x in range(side):
        u = x / (side - 1)
        west_ridge = 1050 * math.exp(-((u - .20) ** 2 / .028 + (v - .25) ** 2 / .10))
        east_ridge = 760 * math.exp(-((u - .82) ** 2 / .035 + (v - .37) ** 2 / .075))
        rolling = 28 * math.sin(u * 24) * math.sin(v * 20) + 12 * math.sin((u + v) * 71)
        height = max(0, min(1500, 120 + west_ridge + east_ridge + rolling))
        values.append(round(height / 1500 * 65535))
image = Image.frombytes('I;16', (side, side), struct.pack('<' + 'H' * len(values), *values))
image.save(folder / 'height.png')
level = 15
x = int((11.575 + 180) / 360 * 2 ** level)
y = int((90 - 48.137) / 180 * 2 ** level)
manifest = {
    'schema_version': 1, 'image': 'height.png',
    'bounds': {'south': 90 - (y + 5) / 2 ** level * 180,
               'west': (x - 4) / 2 ** level * 360 - 180,
               'north': 90 - (y - 4) / 2 ** level * 180,
               'east': (x + 5) / 2 ** level * 360 - 180},
    'min_height_m': 0, 'max_height_m': 1500,
    'revision': 'terra-authored-heightmap-demo-v1',
    'source': {'provider': 'Terra authored heightmap demonstration',
               'license': 'MIT OR Apache-2.0',
               'attribution': 'Synthetic elevation authored for the Terra heightmap importer; not the geography of Munich.',
               'url': 'urn:terra:heightmap:demo:v1',
               'retrieved_unix_seconds': 0, 'synthetic': True}}
(folder / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
