"""Self-tests for scripts/check_views.py's check_view_hierarchy, check_architecture and check_doc_links, on a
throwaway workspace.

Run: python -I -B scripts/tests/test_check_views.py
"""
from pathlib import Path
import importlib.util
import tempfile
import unittest

_SPEC = importlib.util.spec_from_file_location(
    "check_views", Path(__file__).resolve().parent.parent / "check_views.py")
check_views = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(check_views)

HEADER = ("| Module (file) | Owns | Does not own | Entry points / key types | Guard tests |\n"
          "|---|---|---|---|---|\n")
CORE_ROW = "| core `command.rs` | Commands | Saves | `Command` | `command_is_applied` |\n"
GAME_ROW = "| game `art/camera.rs` | The camera | Rules | `fit_field` | `field_fits_the_window` |\n"

CORE_COMMAND = """pub enum Command {}

#[cfg(test)]
mod tests {
    #[test]
    fn command_is_applied() {}
}
"""

GAME_CAMERA = """pub fn fit_field() {}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "slow"]
    fn field_fits_the_window() {}
}
"""


class CheckArchitectureTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.workspace = Path(self._directory.name).resolve()
        files = {
            "crates/horde_core/src/lib.rs": "pub mod command;\n",
            "crates/horde_core/src/command.rs": CORE_COMMAND,
            "crates/horde_game/src/main.rs": "mod art;\nfn main() {}\n",
            "crates/horde_game/src/art/mod.rs": "pub mod camera;\n",
            "crates/horde_game/src/art/camera.rs": GAME_CAMERA,
        }
        for name, text in files.items():
            path = self.workspace / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")

    def tearDown(self):
        self._directory.cleanup()

    def errors(self, rows: str) -> list[str]:
        text = "# Architecture\n\n## Ownership\n\n" + HEADER + rows + "\n## Next\n"
        (self.workspace / "docs").mkdir(exist_ok=True)
        (self.workspace / "docs/ARCHITECTURE.md").write_text(text, encoding="utf-8")
        return check_views.check_architecture(self.workspace)[1]

    def test_a_complete_table_passes(self):
        art = "| game `art` | Drawing | Rules | `art::plugin` | `field_fits_the_window` |\n"
        self.assertEqual([], self.errors(CORE_ROW + art + GAME_ROW))

    def test_a_guard_test_must_be_a_test_fn_not_a_production_fn(self):
        art = "| game `art` | Drawing | Rules | `art::plugin` | `fit_field` |\n"
        self.assertEqual(["ARCHITECTURE.md: guard test not found: fit_field"],
                         self.errors(CORE_ROW + art + GAME_ROW))

    ART_ROW = "| game `art` | Drawing | Rules | `art::plugin` | `field_fits_the_window` |\n"

    def test_a_row_without_a_backticked_module_is_an_error(self):
        plain = "| game art | Drawing | Rules | `art::plugin` | `field_fits_the_window` |\n"
        self.assertEqual(["ARCHITECTURE.md: ownership row names no backticked module or file: game art"],
                         self.errors(CORE_ROW + self.ART_ROW + GAME_ROW + plain))

    def test_a_row_with_an_empty_first_cell_is_not_a_separator(self):
        empty = "|  | Drawing | Rules | `art::plugin` | `field_fits_the_window` |\n"
        self.assertEqual(["ARCHITECTURE.md: ownership row names no backticked module or file: (empty first cell)"],
                         self.errors(CORE_ROW + self.ART_ROW + GAME_ROW + empty))

    def test_the_crate_qualifier_must_match_the_module_crate(self):
        wrong = "| core `art/camera.rs` | The camera | Rules | `fit_field` | `field_fits_the_window` |\n"
        self.assertEqual(["ARCHITECTURE.md: core row names no module in crates/horde_core/src: art/camera.rs",
                          "ARCHITECTURE.md: no ownership row for game module art/camera"],
                         self.errors(CORE_ROW + self.ART_ROW + wrong))

    def test_a_full_crate_path_must_agree_with_the_row_qualifier(self):
        wrong = ("| core `crates/horde_game/src/art/camera.rs` | The camera | Rules | `fit_field` "
                 "| `field_fits_the_window` |\n")
        self.assertEqual(["ARCHITECTURE.md: core row names a game path: crates/horde_game/src/art/camera.rs",
                          "ARCHITECTURE.md: no ownership row for game module art/camera"],
                         self.errors(CORE_ROW + self.ART_ROW + wrong))

    def test_an_unknown_qualifier_or_an_unqualified_module_is_an_error(self):
        unknown = "| engine `art/camera.rs` | The camera | Rules | `fit_field` | `field_fits_the_window` |\n"
        self.assertIn("ARCHITECTURE.md: unknown crate qualifier 'engine' (use core or game): engine `art/camera.rs`",
                      self.errors(CORE_ROW + self.ART_ROW + GAME_ROW + unknown))
        bare = "| `art/camera.rs` | The camera | Rules | `fit_field` | `field_fits_the_window` |\n"
        self.assertEqual(["ARCHITECTURE.md: row names no module or file: art/camera.rs"],
                         self.errors(CORE_ROW + self.ART_ROW + GAME_ROW + bare))


def view_file(view_id: str, parent: str) -> str:
    return (f"---\ntype: View\nid: {view_id}\ntitle: \"{view_id}\"\nparent: {parent}\n"
            f"status: Implemented\nsource: \"docs/VIEW_HIERARCHY.md\"\n---\n\n# {view_id}\n\nA view.\n")


TREE = "- `game`\n  - `battle`\n    - `battle-hud`\n  - `results`\n"


class CheckViewHierarchyTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.workspace = Path(self._directory.name).resolve()
        for view_id, parent in [("game", "null"), ("battle", "game"), ("battle-hud", "battle"), ("results", "game")]:
            self.write(f"docs/components/{view_id}.md", view_file(view_id, parent))

    def tearDown(self):
        self._directory.cleanup()

    def write(self, name: str, text: str):
        path = self.workspace / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def errors(self, tree: str, count: int = 4, required: tuple[str, ...] = ()) -> list[str]:
        self.write("docs/VIEW_HIERARCHY.md", f"# Views\n\n{tree}\nCurrent inventory: **{count} View files**.\n")
        return check_views.check_view_hierarchy(self.workspace, required)[1]

    def test_a_tree_that_matches_the_view_files_passes(self):
        self.assertEqual([], self.errors(TREE))
        self.assertEqual(4, check_views.check_view_hierarchy(self.workspace, ())[0])
        self.assertEqual([], check_views.check(self.workspace)[1])

    def test_an_unlisted_view_is_an_error(self):
        self.write("docs/components/toasts.md", view_file("toasts", "game"))
        self.assertEqual(["VIEW_HIERARCHY.md: does not list View `toasts` (parent `game`)"],
                         self.errors(TREE, count=5))

    def test_a_listed_id_needs_a_view_file(self):
        self.assertEqual(["VIEW_HIERARCHY.md:7: lists `camp-home`, which has no docs/components/camp-home.md"],
                         self.errors(TREE + "  - `camp-home`\n"))

    def test_nesting_must_match_the_front_matter_parent(self):
        tree = "- `game`\n  - `battle`\n  - `battle-hud`\n  - `results`\n"
        self.assertEqual(["VIEW_HIERARCHY.md:5: `battle-hud` is listed under `game`, "
                          "its View file names parent `battle`"], self.errors(tree))

    def test_a_view_is_listed_once_and_nested_by_two_spaces(self):
        self.assertEqual(["VIEW_HIERARCHY.md:7: `results` is listed twice"], self.errors(TREE + "  - `results`\n"))
        tree = "- `game`\n      - `battle`\n"
        self.assertIn("VIEW_HIERARCHY.md:4: `battle` is not nested two spaces under a listed View", self.errors(tree))

    def test_entries_inside_a_fenced_block_are_not_the_tree(self):
        self.assertEqual([], self.errors(TREE + "```\n- `example`\n```\n"))

    def test_the_inventory_line_states_the_view_count(self):
        self.assertEqual(["VIEW_HIERARCHY.md: says 70 View files, docs/components has 4"], self.errors(TREE, count=70))
        self.write("docs/VIEW_HIERARCHY.md", "# Views\n\n" + TREE)
        self.assertEqual(["VIEW_HIERARCHY.md: no 'Current inventory: **<n> View files**' line"],
                         check_views.check_view_hierarchy(self.workspace, ())[1])

    def test_every_required_production_view_has_a_file(self):
        self.assertEqual(["docs/components/camp-home.md: missing (a production-pass View, W7-07)"],
                         self.errors(TREE, required=("results", "camp-home")))
        self.assertIn("camp-home", check_views.PRODUCTION_VIEWS)


GATE = "A 60 s mix at default sliders reads **-23 ±2 LUFS integrated** with **true peak ≤ 0.95 FS**.\n"


class CheckDocLinksTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.workspace = Path(self._directory.name).resolve()
        self.write("scripts/package_playtest.ps1", "$CuratedDocs = @('PLAYTEST_GUIDE.md')\n")
        self.write("docs/audio/survival-audition.wav", "RIFF")
        self.write("docs/JOURNEYS.md", "# Journeys\n")
        self.write("docs/PLAYTEST_GUIDE.md", "# Guide\n")
        for name in check_views.LOUDNESS_GATE_DOCS:
            self.write(name, "# Gate\n\n" + GATE)

    def tearDown(self):
        self._directory.cleanup()

    def write(self, name: str, text: str):
        path = self.workspace / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def errors(self) -> list[str]:
        return check_views.check_doc_links(self.workspace)[1]

    def test_existing_web_and_anchor_links_pass(self):
        self.write("docs/GDD.md", "See [journeys](JOURNEYS.md#loop), [web](https://example.com/x.md),\n"
                                  "[top](#top), [mail](mailto:a@b.c) and [again](plans/../JOURNEYS.md).\n")
        self.assertEqual([], self.errors())
        self.assertEqual(2, check_views.check_doc_links(self.workspace)[0])

    def test_a_dead_relative_link_is_an_error(self):
        self.write("docs/GDD.md", "# GDD\nSee [gone](GONE.md).\n")
        self.assertEqual(["docs/GDD.md:2: dead link 'GONE.md'"], self.errors())

    def test_links_inside_code_are_not_links(self):
        self.write("docs/GDD.md", "Inline `[a](GONE.md)` code.\n```\n[b](GONE.md)\n```\n")
        self.assertEqual([], self.errors())

    def test_no_doc_links_the_v1_audition_reel(self):
        self.write("docs/AUDIO.md", "Hear [the reel](audio/survival-audition.wav).\n")
        self.assertEqual(["docs/AUDIO.md:1: links the v1 audition reel in docs/audio/, which is never packaged: "
                          "'audio/survival-audition.wav'"], self.errors())

    def test_a_packaged_doc_names_nothing_in_docs_audio(self):
        self.write("docs/PLAYTEST_GUIDE.md", "# Guide\nListen to docs/audio/survival-audition.wav.\n")
        self.assertEqual(["docs/PLAYTEST_GUIDE.md:2: the playtest package leaves out docs/audio/; "
                          "a packaged doc names nothing there"], self.errors())
        # The same words in a doc the package does not ship are fine.
        self.write("docs/PLAYTEST_GUIDE.md", "# Guide\n")
        self.write("docs/JOURNEYS.md", "The v1 reel is docs/audio/survival-audition.wav.\n")
        self.assertEqual([], self.errors())

    def test_each_gate_doc_records_the_loudness_gate(self):
        name = check_views.LOUDNESS_GATE_DOCS[0]
        self.write(name, "# Gate\n\n-16 to -14 LUFS.\n")
        self.assertEqual([f"{name}: does not record the loudness gate "
                          "('-23 ±2 LUFS integrated' and 'true peak ≤ 0.95 FS')"], self.errors())
        (self.workspace / name).unlink()
        self.assertEqual([f"{name}: missing (it records the loudness gate)"], self.errors())


if __name__ == "__main__":
    unittest.main()
