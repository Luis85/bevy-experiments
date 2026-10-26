//! The save codecs (spec §5.8, §12.7, ADR-0006, ADR-0011): the serde-reflection format of
//! every `Hashed` component and resource codec of the sim with every catalog feature
//! (`fl_catalog::features()`, built on the base mod), from `fl_sim::codec_formats`, pinned by
//! an insta snapshot. Each codec block is headed by its `<name>@<version>`, with the older
//! versions it reads.
//!
//! The guard reads the committed snapshot: a codec whose format changed while its version stayed
//! the same, or whose version rose without a migration from the committed version, fails before
//! the snapshot diff, which then needs a bless.

use fl_sim::{MatchSetup, RunConfig, build_sim_app, codec_formats, load_content};

#[path = "../../fl_protocol/tests/support/format_guard.rs"]
mod format_guard;

use format_guard::{committed, guard};

/// The codec blocks of the sim with every catalog feature.
fn codec_text() -> String {
    let features = fl_catalog::features();
    let content =
        load_content(&features, &[fl_catalog::base_mod_dir()]).expect("the base mod loads");
    let app = build_sim_app(
        &features,
        content,
        MatchSetup::default(),
        RunConfig::default(),
    )
    .expect("the sim builds");
    codec_formats(&app)
}

#[test]
fn save_codecs_of_every_feature() {
    let text = codec_text();
    assert!(text.contains("\ncore.pose@1:\n"), "{text}");
    assert!(text.contains("\ncore.match_seed@1:\n"), "{text}");
    if let Some(committed) = committed("save_codecs")
        && let Err(err) = guard(&committed, &text)
    {
        panic!("{err}");
    }
    insta::assert_snapshot!("save_codecs", text);
}
