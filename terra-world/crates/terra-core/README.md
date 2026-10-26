# terra-core

Rendering-independent domain for Terra World. This crate contains no Bevy or
network dependency and can be reused by servers, tools and games.

## Coordinate contract

- Authoritative coordinates are `f64` WGS84 latitude/longitude in degrees and
  ellipsoidal altitude in metres. Longitude normalizes to `[-180,180)`.
- `LocalFrame` projects ECEF positions into **east/up/south** for Bevy. Earth
  curvature remains in the projection. Rebase rendering as the focus moves;
  never store authoritative world positions in `f32` transforms.
- Actor velocity is **east/north/up**. Weather wind direction is the direction
  wind comes **from**; `wind_enu_m_s` converts it into velocity.
- `TileKey` is an equirectangular quadtree covering both poles. It is not a
  Web Mercator tile key. Longitude neighbors wrap; polar rows clip. Neighborhood
  radius is bounded at 64 to keep a malformed request finite.
- Height fields run north-to-south, west-to-east. Four corners include shared
  tile edges. `validate` should be called at ingestion and after deserialization.
- Source elevations can use orthometric/geoid heights. A provider must retain
  datum provenance and either convert to ellipsoidal altitude or explicitly
  mark its approximation. Datum conversion is not hidden inside the renderer.

## Environmental scope

`WorldClock` defaults to 2026-06-21 12:00 UTC and advances in explicitly scaled
UTC seconds. Solar direction uses NOAA's fractional-year approximation without
refraction, terrain shadows or a full ephemeris. Meteorological seasons reverse
between hemispheres; tropical wet/dry calendars need sourced regional data.

Synthetic weather and biome helpers are deterministic **illustrative fallbacks**.
They are not observations or climate predictions. Biome classification is coarse
and cannot distinguish inland terrain below sea level from ocean without a real
water mask. Games should override it using loaded land cover and climate data.

Actors provide basic kinematic movement and declared ground/water/air capability.
Navigation, collision, locomotion, flight and hydrodynamics belong to game-specific
systems. The speed limits are sample-game policy values and are not engineering
performance data.

## References

- [NGA WGS84 reference ellipsoid](https://earth-info.nga.mil/GandG/wgs84/gravitymod/egm2008/index.html)
- [NOAA geometric solar equations](https://gml.noaa.gov/grad/solcalc/solareqns.PDF)
- [Cesium geospatial coordinate guide](https://github.com/CesiumGS/community/blob/main/GeospatialGuide/README.md)

`cargo test -p terra-core` exercises geodetic roundtrips at poles/antimeridian,
altitude precision, local axes, quadtree boundaries, leap years, midnight sun,
hemisphere seasons, data validation and deterministic synthetic weather.
