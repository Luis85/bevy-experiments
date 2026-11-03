<#
Read-only verification of a native model-parity capture. The application emits
the ROI manifest when LASTLIGHT_PLAYER_MODEL_PARITY_REPORT is supplied. This
script never rewrites images or accepts a baseline/tolerance.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $Capture,
    [string] $Metadata,
    [string] $Source = (Join-Path $PSScriptRoot '../assets/art/pixel/menu-survivors-v1.png'),
    [string] $PythonCommand = 'python'
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Metadata)) {
    $Metadata = [System.IO.Path]::ChangeExtension($Capture, '.json')
}
foreach ($inputFile in @($Capture, $Metadata, $Source)) {
    if (-not (Test-Path -LiteralPath $inputFile -PathType Leaf)) {
        throw "Required parity input does not exist: $inputFile"
    }
}

$analysis = @'
import hashlib
import json
import math
import pathlib
import sys
from PIL import Image

if not __debug__:
    raise RuntimeError("Exact parity checks require Python assertions enabled")

def rgba_pixels(image):
    values = iter(image.tobytes())
    return list(zip(values, values, values, values))

capture_path, metadata_path, source_path = map(pathlib.Path, sys.argv[1:])
metadata = json.loads(metadata_path.read_text(encoding="utf-8-sig"))
expected_hash = "4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a"
assert hashlib.sha256(source_path.read_bytes()).hexdigest() == expected_hash, "Changed original source PNG"
assert metadata["schema_version"] == 1, "Unknown parity manifest schema"
assert metadata["source_sha256"] == expected_hash, "Manifest uses another source"
# Approved original-art crop: Rook's legacy right edge 652 extends to 654
# to retain the faint original pistol fringe. This is the first exact fixture,
# not a reblessing of an existing screenshot.
expected_frames = {"rook": [282, 42, 654, 848], "june": [1128, 60, 1465, 848]}
model_id = metadata["model_id"]
assert model_id in expected_frames, "Unknown survivor"
assert metadata["source_frame"] == expected_frames[model_id], "Unexpected source frame"
assert math.isfinite(metadata["pixels_per_point"]) and metadata["pixels_per_point"] > 0
audit = metadata["bind_audit"]
for field in ["missing_visible_pixels", "duplicate_visible_pixels", "differing_pixels", "differing_bytes", "hidden_fill_packets", "foreign_source_packets", "malformed_rectangles", "nonidentity_rectangles", "geometry_mismatches"]:
    assert audit[field] == 0, f"Source bind audit failed: {field}={audit[field]}"
# Older accepted manifests predate articulated packet matrices. New captures
# always include this check, while the original immutable evidence stays usable.
assert audit.get("nonidentity_packet_mappings", 0) == 0, "Nonidentity source/bone transforms in reference bind"
source = Image.open(source_path).convert("RGBA")
assert source.size == (1774, 887), "Changed original source dimensions"
source = source.crop(tuple(expected_frames[model_id]))
source_pixels = rgba_pixels(source)
visible_count = sum(pixel[3] > 0 for pixel in source_pixels)
native = Image.open(capture_path).convert("RGBA")
pairs = metadata["comparisons"]
assert len(pairs) == 2, "Need separate black and white native comparisons"
assert [pair["background"] for pair in pairs] == [[0, 0, 0, 255], [255, 255, 255, 255]], "Wrong parity backgrounds"
seen_rects = []
reports = []

def read_roi(rect):
    assert len(rect) == 4 and all(type(value) is int for value in rect), "ROI must use physical integer x/y/width/height"
    x, y, width, height = rect
    assert (width, height) == source.size, "Native ROI is not source resolution"
    assert x >= 0 and y >= 0 and x + width <= native.width and y + height <= native.height, "Clipped native source ROI"
    for previous in seen_rects:
        px, py, pw, ph = previous
        assert x + width <= px or px + pw <= x or y + height <= py or py + ph <= y, "Parity ROIs overlap"
    seen_rects.append(rect)
    return native.crop((x, y, x + width, y + height))

for pair in pairs:
    reference = read_roi(pair["reference"])
    assembled = read_roi(pair["assembled"])
    background = tuple(pair["background"])
    reference_pixels = rgba_pixels(reference)
    assert len(set(reference_pixels)) >= 128, "Reference ROI is empty or lacks the original artwork"
    assert sum(pixel != background for pixel in reference_pixels) > visible_count // 2, "Reference ROI misses most original pixels"
    assert all(actual == background for expected, actual in zip(source_pixels, reference_pixels) if expected[3] == 0), "Reference ROI/source texels are misaligned or contaminated"
    opaque = [(expected, actual) for expected, actual in zip(source_pixels, reference_pixels) if expected[3] == 255]
    assert len(opaque) > 0 and all(expected == actual for expected, actual in opaque), "GPU reference opaque pixels do not match the actual original source RGBA"
    expected_bytes, actual_bytes = reference.tobytes(), assembled.tobytes()
    different_bytes = sum(expected != actual for expected, actual in zip(expected_bytes, actual_bytes))
    different_pixels = sum(expected_bytes[index:index+4] != actual_bytes[index:index+4] for index in range(0, len(expected_bytes), 4))
    maximum_channel_delta = max(abs(expected - actual) for expected, actual in zip(expected_bytes, actual_bytes))
    report = {"background": pair["background"], "different_pixels": different_pixels, "different_bytes": different_bytes, "maximum_channel_delta": maximum_channel_delta, "verified_original_opaque_pixels": len(opaque)}
    reports.append(report)
    assert different_pixels == 0 and different_bytes == 0, json.dumps({"model_id": model_id, "failure": report})

print(json.dumps({"model_id": model_id, "capture": str(capture_path.resolve()), "source_sha256": expected_hash, "source_size": source.size, "visible_source_pixels": visible_count, "native_comparisons": reports}, indent=2))
'@

# Ignore inherited PYTHONOPTIMIZE/PYTHONWARNINGS so exact checks cannot vanish.
& $PythonCommand -E -c $analysis $Capture $Metadata $Source
if ($LASTEXITCODE -ne 0) {
    throw "Exact native player-model parity failed (exit $LASTEXITCODE)."
}
