"""Reproducible, original adjacent-map fixture; preserves the legacy demo.

Split its 257-column raster with one exactly shared sample column, and clip
attributed explicit land-cover polygons into the corresponding child bundle.
No data download or real-world reconstruction is involved.
"""
from copy import deepcopy
import json
from pathlib import Path
from PIL import Image

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parent / 'heightmap-demo'
manifest = json.loads((SOURCE / 'manifest-v2.json').read_text())
features = json.loads((SOURCE / 'features-v2.geojson').read_text())
raster = Image.open(SOURCE / 'height.png')
mid = (raster.width - 1) // 2
split = (manifest['bounds']['west'] + manifest['bounds']['east']) / 2


def clip_ring(ring, bound, keep_west):
    points = ring[:-1] if ring[0] == ring[-1] else ring
    out = []
    if not points:
        return out
    prev = points[-1]
    inside = lambda p: p[0] <= bound if keep_west else p[0] >= bound
    for point in points:
        if inside(prev) != inside(point):
            t = (bound - prev[0]) / (point[0] - prev[0])
            out.append([bound, prev[1] + t * (point[1] - prev[1])])
        if inside(point):
            out.append(point)
        prev = point
    compact = []
    for point in out:
        if not compact or point != compact[-1]:
            compact.append(point)
    if len(compact) >= 3:
        if compact[0] != compact[-1]:
            compact.append(compact[0])
        return compact
    return []


for side in ('west', 'east'):
    western = side == 'west'
    raster.crop((0, 0, mid + 1, raster.height) if western else
                (mid, 0, raster.width, raster.height)).save(HERE / f'{side}.png')
    child = deepcopy(manifest)
    child['image'] = f'{side}.png'
    child['revision'] = f'terra-connected-valley-{side}-v1'
    child['bounds']['east' if western else 'west'] = split
    child['features']['geojson'] = f'{side}.geojson'
    child['source']['attribution'] += ' Prepared as adjacent west/east bundles with a shared elevation column.'
    child_features = []
    for feature in features['features']:
        feature = deepcopy(feature)
        geometry = feature['geometry']
        if geometry['type'] == 'Point':
            longitude = geometry['coordinates'][0]
            if not (longitude <= split if western else longitude > split):
                continue
        elif geometry['type'] == 'Polygon':
            ring = clip_ring(geometry['coordinates'][0], split, western)
            if not ring:
                continue
            geometry['coordinates'] = [ring]
        else:
            raise ValueError('Fixture generator expects points and exterior polygons')
        child_features.append(feature)
    (HERE / f'{side}.json').write_text(json.dumps(child, indent=2) + '\n')
    (HERE / f'{side}.geojson').write_text(json.dumps({'type':'FeatureCollection', 'features':child_features}, indent=2) + '\n')

(HERE / 'manifest.json').write_text(json.dumps({
    'schema_version': 1, 'kind': 'heightmap_set',
    'revision': 'terra-connected-valley-v1',
    'bundles': ['west.json', 'east.json'], 'seam_tolerance_m': 0.01,
}, indent=2) + '\n')
