"""Self-tests for scripts/check_views.py's check_architecture, on a throwaway workspace.

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


if __name__ == "__main__":
    unittest.main()
