# Authored heightmap example

This 257 × 257, 16-bit grayscale PNG is a **synthetic elevation dataset**, not a
satellite image and not a reconstruction of Munich. Its georeferencing covers a
9 × 9 block of level-15 geographic tiles around the default demo coordinate.
The source attribution and synthetic flag are required in the manifest.

The provider maps pixel value 0 to 0 metres and 65535 to 1500 metres. Image rows
run north to south. Bilinear sampling shares exact elevation samples at chunk
edges. Data is source-defined altitude; the importer does not apply a geoid
correction or infer land use, water, buildings, or roads from grayscale values.

Use `HeightmapProvider::open("crates/terra-stream/data/heightmap-demo/manifest.json", 65)`
and install it with the Bevy custom-provider hook, or select the native
`--heightmap` option. A tile outside the manifest's complete geographic coverage
is unavailable; it is never filled with fabricated heights. Dateline-crossing
rasters should be split into two explicitly georeferenced inputs.

`generate.py` documents and regenerates the exact authored fixture using Python
and Pillow. It is not needed by the Rust runtime.

## Semantic valley bundle

New launcher worlds use `manifest-v2.json`, which adds `features-v2.geojson`
without changing the original manifest or raster. It contains original synthetic
woodland, fields, an orchard and specimen trees. `generate_features.py` rebuilds
these files independently of the v1 elevation generator. Separate attribution
marks all of this land cover as synthetic. See the [bundle guide](../../../../docs/SOURCE_BUNDLES.md)
for geometry limits and content-based source identity.
