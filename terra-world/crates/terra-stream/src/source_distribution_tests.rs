use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "terra-source-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn authored() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/heightmap-connected/manifest.json")
}
fn options() -> SourceInspectionOptions {
    SourceInspectionOptions {
        terrain_resolution: 9,
        ..Default::default()
    }
}

#[test]
fn authored_connected_pack_exports_verifies_and_installs_with_exact_identity() {
    let temp = Directory::new();
    let original = inspect_prepared_source(authored(), options()).unwrap();
    assert_eq!(original.files.len(), 7);
    assert_eq!(original.bundles.len(), 2);
    assert_eq!(original.coverage.requested_tiles, 81);
    assert_eq!(original.coverage.fully_covered_tiles, 81);
    assert!(original.coverage.complete);
    assert!(
        original
            .bundles
            .iter()
            .all(|b| b.elevation.synthetic && b.features.as_ref().unwrap().synthetic)
    );
    let exported = export_source_package(authored(), temp.0.join("portable"), options()).unwrap();
    assert_eq!(exported.inspection, original);
    let verified = verify_source_package(
        temp.0.join("portable"),
        Some(&exported.package_identity),
        Some(&original.source_identity),
    )
    .unwrap();
    let installed = install_source_package(
        temp.0.join("portable"),
        temp.0.join("installed"),
        Some(&exported.package_identity),
        Some(&original.source_identity),
    )
    .unwrap();
    assert_eq!(verified.package_identity, installed.package_identity);
    assert_eq!(installed.inspection, original);
    assert_eq!(
        fs::read(temp.0.join("portable/source-package.json")).unwrap(),
        fs::read(temp.0.join("installed/source-package.json")).unwrap()
    );
    assert!(
        !temp.0.join("portable/generate.py").exists(),
        "unreferenced source-development files are not copied"
    );
    for file in &original.files {
        assert_eq!(
            fs::read(authored().parent().unwrap().join(&file.path)).unwrap(),
            fs::read(temp.0.join("installed").join(&file.path)).unwrap()
        );
    }
}

#[test]
fn legacy_raster_identity_is_preserved_and_new_file_hashes_still_bind_its_content() {
    let temp = Directory::new();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/heightmap-demo/manifest.json");
    let original = crate::HeightmapProvider::open(&path, 9).unwrap();
    let exported = export_source_package(path, temp.0.join("legacy"), options()).unwrap();
    assert_eq!(
        exported.inspection.source_identity,
        original.manifest.revision
    );
    assert_eq!(
        exported.inspection.source_identity,
        original.source_revision()
    );
    let mut bytes = fs::read(temp.0.join("legacy/height.png")).unwrap();
    bytes.push(0);
    fs::write(temp.0.join("legacy/height.png"), bytes).unwrap();
    assert!(
        verify_source_package(
            temp.0.join("legacy"),
            Some(&exported.package_identity),
            None
        )
        .unwrap_err()
        .to_string()
        .contains("hash/size")
    );
}

fn concave_fixture(root: &Path) -> PathBuf {
    let source = SourceRecord {
        provider: "original test elevation".into(),
        license: "MIT OR Apache-2.0".into(),
        attribution: "Authored constant test elevation; not surveyed geography".into(),
        url: "urn:terra:tests:source-coverage".into(),
        retrieved_unix_seconds: 0,
        synthetic: true,
    };
    let mut bundles = Vec::new();
    for (i, (x, y)) in [(6, 6), (7, 6), (6, 7)].into_iter().enumerate() {
        let image_name = format!("{i}.png");
        image::GrayImage::from_pixel(3, 3, image::Luma([128]))
            .save(root.join(&image_name))
            .unwrap();
        let manifest = HeightmapManifest {
            schema_version: 1,
            image: image_name.into(),
            bounds: TileKey::new(4, x, y).unwrap().bounds(),
            min_height_m: 0.,
            max_height_m: 100.,
            revision: format!("authored-test-{i}"),
            source: source.clone(),
            features: None,
        };
        let name = format!("{i}.json");
        fs::write(
            root.join(&name),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        bundles.push(PathBuf::from(name));
    }
    let set = HeightmapSetManifest {
        schema_version: 1,
        kind: "heightmap_set".into(),
        revision: "authored-concave-v1".into(),
        bundles,
        seam_tolerance_m: 0.,
    };
    let path = root.join("manifest.json");
    fs::write(&path, serde_json::to_vec_pretty(&set).unwrap()).unwrap();
    path
}

#[test]
fn concave_extent_and_partial_edge_tiles_are_explicit_missing_coverage() {
    let temp = Directory::new();
    let path = concave_fixture(&temp.0);
    let mut settings = options();
    settings.coverage_level = 4;
    let report = inspect_prepared_source(&path, settings).unwrap();
    assert_eq!(report.coverage.requested_tiles, 4);
    assert_eq!(report.coverage.fully_covered_tiles, 3);
    assert_eq!(
        report.coverage.missing_tiles,
        [TileKey::new(4, 7, 7).unwrap()]
    );
    assert!(!report.coverage.complete);
    assert!(
        report
            .coverage
            .nodata_policy
            .contains("NoData masks/sentinel detection are unsupported")
    );
    let mut bounds = TileKey::new(4, 6, 6).unwrap().bounds();
    bounds.west -= 0.001;
    settings.requested_bounds = Some(bounds);
    let report = inspect_prepared_source(&path, settings).unwrap();
    assert_eq!(report.coverage.requested_tiles, 2);
    assert_eq!(report.coverage.fully_covered_tiles, 1);
    assert_eq!(
        report.coverage.missing_tiles,
        [TileKey::new(4, 5, 6).unwrap()]
    );
}

#[test]
fn coverage_enumeration_is_bounded_before_allocating_a_global_tile_list() {
    let settings = SourceInspectionOptions {
        coverage_level: 20,
        requested_bounds: Some(GeoBounds {
            south: -90.,
            west: -180.,
            north: 90.,
            east: 180.,
        }),
        ..options()
    };
    let error = inspect_prepared_source(authored(), settings)
        .unwrap_err()
        .to_string();
    assert!(error.contains("65,536"), "{error}");
}

fn change_index(directory: &Path, edit: impl FnOnce(&mut SourceInspection)) {
    let path = directory.join(SOURCE_PACKAGE_INDEX);
    let mut index: SourceInspection = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut index);
    fs::write(path, serde_json::to_vec_pretty(&index).unwrap()).unwrap();
}

#[test]
fn package_index_cannot_invent_coverage_provenance_or_source_identity() {
    for change in ["coverage", "provenance", "identity"] {
        let temp = Directory::new();
        let package = temp.0.join("package");
        export_source_package(authored(), &package, options()).unwrap();
        change_index(&package, |index| match change {
            "coverage" => index.coverage.fully_covered_tiles += 1,
            "provenance" => index.bundles[0].elevation.license = "invented license".into(),
            _ => index.source_identity = "changed-save-authority".into(),
        });
        assert!(
            verify_source_package(&package, None, None)
                .unwrap_err()
                .to_string()
                .contains("index does not match")
        );
    }
}

#[test]
fn traversal_aliases_and_unindexed_members_are_not_admitted() {
    let temp = Directory::new();
    let package = temp.0.join("package");
    export_source_package(authored(), &package, options()).unwrap();
    change_index(&package, |index| index.files[0].path = "../outside".into());
    assert!(
        verify_source_package(&package, None, None)
            .unwrap_err()
            .to_string()
            .contains("contained relative")
    );

    let package = temp.0.join("unindexed");
    export_source_package(authored(), &package, options()).unwrap();
    fs::write(
        package.join("unexpected.txt"),
        b"not in the declared package",
    )
    .unwrap();
    assert!(
        verify_source_package(&package, None, None)
            .unwrap_err()
            .to_string()
            .contains("unindexed")
    );

    let source = temp.0.join("source");
    fs::create_dir(&source).unwrap();
    let manifest = concave_fixture(&source);
    let mut set: HeightmapSetManifest = read_manifest(&manifest).unwrap();
    fs::create_dir(source.join("Case")).unwrap();
    // A case-insensitive filesystem already sees Case here. The two distinct
    // filenames can still form an intentionally ambiguous manifest inventory.
    fs::create_dir_all(source.join("case")).unwrap();
    fs::copy(source.join("0.json"), source.join("Case/map.json")).unwrap();
    fs::copy(source.join("0.png"), source.join("Case/0.png")).unwrap();
    fs::copy(source.join("1.json"), source.join("case/map2.json")).unwrap();
    fs::copy(source.join("1.png"), source.join("case/1.png")).unwrap();
    set.bundles = vec!["Case/map.json".into(), "case/map2.json".into()];
    fs::write(&manifest, serde_json::to_vec(&set).unwrap()).unwrap();
    assert!(
        inspect_prepared_source(manifest, options())
            .unwrap_err()
            .to_string()
            .contains("case-insensitive")
    );
}

#[test]
fn wrong_expected_identity_and_existing_destination_leave_prior_data_intact() {
    let temp = Directory::new();
    let package = temp.0.join("package");
    let exported = export_source_package(authored(), &package, options()).unwrap();
    let original = fs::read(package.join(SOURCE_PACKAGE_INDEX)).unwrap();
    assert!(export_source_package(authored(), &package, options()).is_err());
    assert_eq!(
        fs::read(package.join(SOURCE_PACKAGE_INDEX)).unwrap(),
        original
    );
    let destination = temp.0.join("installed");
    assert!(install_source_package(&package, &destination, Some("blake3:wrong"), None).is_err());
    assert!(!destination.exists());
    assert!(
        install_source_package(
            &package,
            &destination,
            Some(&exported.package_identity),
            Some("other-world")
        )
        .is_err()
    );
    assert!(!destination.exists());
    assert!(
        verify_source_package(&temp.0, None, None).is_err(),
        "incomplete directory without a published index is not accepted"
    );
}

#[test]
fn portable_paths_reject_platform_characters_and_publication_name_aliases() {
    for name in [
        "height?.png",
        "height*.png",
        "height<.png",
        "height>.png",
        "height\".png",
        "height|.png",
        "héight.png",
        "CON.png",
        "a/COM1.png",
    ] {
        assert!(relative(Path::new(name)).is_err(), "{name}");
    }
    for name in [
        "SOURCE-PACKAGE.JSON",
        "Source-Package.Json/child.json",
        ".SOURCE-PACKAGE.PENDING",
        ".source-package.pending/child.json",
    ] {
        assert!(portable_member(Path::new(name)).is_err(), "{name}");
    }
    let temp = Directory::new();
    let manifest = concave_fixture(&temp.0);
    let aliased = temp.0.join("SOURCE-PACKAGE.JSON");
    fs::rename(manifest, &aliased).unwrap();
    assert!(
        inspect_prepared_source(aliased, options())
            .unwrap_err()
            .to_string()
            .contains("reserved publication")
    );
}

#[test]
fn nested_packages_keep_forward_slash_inventory_paths_on_every_host() {
    assert_eq!(
        joined_relative(Path::new("tiles/west"), Path::new("height.png"))
            .unwrap()
            .to_str(),
        Some("tiles/west/height.png")
    );
    assert_eq!(
        joined_relative(Path::new(""), Path::new("tiles/west/map.json"))
            .unwrap()
            .to_str(),
        Some("tiles/west/map.json")
    );
    for input in ["tiles\\west", "../escape", "/absolute", "C:/absolute"] {
        assert!(
            joined_relative(Path::new("maps"), Path::new(input)).is_err(),
            "{input}"
        );
    }
    let temp = Directory::new();
    let source = temp.0.join("nested-source");
    fs::create_dir(&source).unwrap();
    let manifest = concave_fixture(&source);
    let mut set: HeightmapSetManifest = read_manifest(&manifest).unwrap();
    for index in 0..3 {
        let relative = format!("tiles/part-{index}");
        fs::create_dir_all(source.join(&relative)).unwrap();
        for extension in ["json", "png"] {
            fs::rename(
                source.join(format!("{index}.{extension}")),
                source.join(format!("{relative}/{index}.{extension}")),
            )
            .unwrap();
        }
        set.bundles[index] = PathBuf::from(format!("{relative}/{index}.json"));
    }
    fs::write(&manifest, serde_json::to_vec_pretty(&set).unwrap()).unwrap();
    let exported = export_source_package(
        &manifest,
        temp.0.join("nested-package"),
        SourceInspectionOptions {
            coverage_level: 4,
            ..options()
        },
    )
    .unwrap();
    for file in &exported.inspection.files {
        assert!(!file.path.to_str().unwrap().contains('\\'));
    }
    for bundle in &exported.inspection.bundles {
        assert!(!bundle.manifest.to_str().unwrap().contains('\\'));
        assert!(bundle.manifest.to_str().unwrap().starts_with("tiles/part-"));
    }
    let installed = install_source_package(
        temp.0.join("nested-package"),
        temp.0.join("nested-installed"),
        Some(&exported.package_identity),
        Some(&exported.inspection.source_identity),
    )
    .unwrap();
    assert_eq!(installed.inspection, exported.inspection);
    assert_eq!(installed.package_identity, exported.package_identity);
}

#[test]
fn forged_aggregate_and_per_member_byte_bounds_fail_before_any_payload_reads() {
    for kind in ["total", "aggregate", "member"] {
        let temp = Directory::new();
        let package = temp.0.join("package");
        export_source_package(authored(), &package, options()).unwrap();
        change_index(&package, |index| {
            // These files deliberately do not exist: an IO error would prove
            // that the bounded declaration checks ran too late.
            index.files = vec![SourcePackageFile {
                path: "absent.png".into(),
                bytes: 32 * 1024 * 1024,
                blake3: format!("blake3:{}", "0".repeat(64)),
            }];
            match kind {
                "aggregate" => {
                    index.files = (0..9)
                        .map(|i| SourcePackageFile {
                            path: format!("absent-{i}.png").into(),
                            ..index.files[0].clone()
                        })
                        .collect()
                }
                "member" => index.files[0].bytes += 1,
                _ => (),
            }
            index.total_bytes = 1;
        });
        let error = verify_source_package(&package, None, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("aggregate") || error.contains("member byte bound"),
            "{kind}: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn source_members_package_members_and_output_ancestors_reject_symlinks() {
    use std::os::unix::fs::symlink;
    let temp = Directory::new();
    let source = temp.0.join("source");
    fs::create_dir(&source).unwrap();
    let manifest = concave_fixture(&source);
    fs::rename(source.join("0.png"), source.join("real.png")).unwrap();
    symlink(source.join("real.png"), source.join("0.png")).unwrap();
    assert!(
        inspect_prepared_source(manifest, options())
            .unwrap_err()
            .to_string()
            .contains("symbolic links")
    );
    let package = temp.0.join("package");
    export_source_package(authored(), &package, options()).unwrap();
    symlink(&source, package.join("unexpected-link")).unwrap();
    assert!(
        verify_source_package(&package, None, None)
            .unwrap_err()
            .to_string()
            .contains("link")
    );
    symlink(&temp.0, temp.0.join("linked-parent")).unwrap();
    assert!(
        export_source_package(authored(), temp.0.join("linked-parent/output"), options())
            .unwrap_err()
            .to_string()
            .contains("symbolic links")
    );
    assert!(!temp.0.join("output").exists());
}
