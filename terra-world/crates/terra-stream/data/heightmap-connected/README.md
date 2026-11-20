# Connected synthetic valley

Two adjacent elevation/semantic bundles demonstrate verified, continuous source
coverage. This is original authored Terra content, **not observed geography or
land use around Munich**. Each child retains its elevation and feature
attribution, uses a 129 × 257 16-bit grayscale raster, and shares the exact
boundary sample column with its neighbor.

From the repository root:

```sh
cargo run -p terra-demo -- --config configs/connected-heightmap.json
```

Regenerate with Python and Pillow:

```sh
python3 crates/terra-stream/data/heightmap-connected/generate.py
```

The generator reads the existing heightmap demo without modifying it. Its
polygons are explicitly clipped into the two child extents. The collection
manifest names both children and a 0.01 m seam tolerance. The source covers 81
level-15 tiles; its center seam cuts through a tile, exercising combined coverage
rather than depending on one source file per streamed tile.

See `docs/SOURCE_BUNDLES.md` for authoring limits and source identity semantics.
