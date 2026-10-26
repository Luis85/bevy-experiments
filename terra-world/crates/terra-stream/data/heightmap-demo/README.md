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
