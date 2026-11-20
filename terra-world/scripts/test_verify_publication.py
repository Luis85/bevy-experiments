"""Terminal receipt publication regressions; no Rust or window needed."""
from pathlib import Path
import json
import os
import tempfile
import unittest
from unittest.mock import patch
import verify


class TerminalPublicationTests(unittest.TestCase):
    def test_terminal_alias_and_exclusive_completion_match_exactly(self):
        with tempfile.TemporaryDirectory() as tmp:
            for status in ('passed', 'failed'):
                directory = Path(tmp) / status
                directory.mkdir()
                report = {'status': status, 'gates': [{'name': 'fixture'}]}
                verify.publish_terminal_report(directory, report)
                self.assertEqual((directory / 'gates.json').read_bytes(), (directory / 'completed-gates.json').read_bytes())
                self.assertEqual(json.loads((directory / 'completed-gates.json').read_bytes()), report)
                self.assertEqual(report['completion_record'], 'completed-gates.json')
                self.assertEqual(sorted(p.name for p in directory.iterdir()), ['completed-gates.json', 'gates.json'])

    def test_nonterminal_status_never_publishes_completion(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'completed-gates.json'
            with self.assertRaisesRegex(ValueError, 'terminal'):
                verify.publish_completion(path, {'status': 'running'})
            self.assertEqual(list(path.parent.iterdir()), [])

    def test_existing_completion_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'completed-gates.json'
            path.write_bytes(b'prior receipt')
            with self.assertRaises(FileExistsError):
                verify.publish_completion(path, {'status': 'passed'})
            self.assertEqual(path.read_bytes(), b'prior receipt')
            self.assertEqual([p.name for p in path.parent.iterdir()], ['completed-gates.json'])

    @unittest.skipUnless(os.name == 'posix', 'Real symlink creation qualified on POSIX')
    def test_existing_symlink_does_not_modify_its_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / 'other.json'
            target.write_bytes(b'unrelated')
            path = root / 'completed-gates.json'
            path.symlink_to(target)
            with self.assertRaises(FileExistsError):
                verify.publish_completion(path, {'status': 'passed'})
            self.assertTrue(path.is_symlink())
            self.assertEqual(target.read_bytes(), b'unrelated')

    def test_failed_link_leaves_no_partial_or_staging_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with patch.object(verify.os, 'link', side_effect=OSError('fixture write failure')):
                with self.assertRaisesRegex(OSError, 'fixture write failure'):
                    verify.publish_completion(root / 'completed-gates.json', {'status': 'passed'})
            self.assertEqual(list(root.iterdir()), [])

    def test_changed_published_bytes_fail_and_remain_for_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'completed-gates.json'
            link = os.link
            def changed(source, destination):
                link(source, destination)
                Path(destination).write_bytes(b'changed snapshot')
            with patch.object(verify.os, 'link', side_effect=changed):
                with self.assertRaisesRegex(ValueError, 'differs'):
                    verify.publish_completion(path, {'status': 'passed'})
            self.assertEqual(path.read_bytes(), b'changed snapshot')
            self.assertEqual([p.name for p in path.parent.iterdir()], ['completed-gates.json'])

    def test_stale_alias_readback_prevents_completion(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def stale(path, value):
                path.write_text('{"status":"running"}')
            with patch.object(verify, 'write_json', side_effect=stale):
                with self.assertRaisesRegex(ValueError, 'readback'):
                    verify.publish_terminal_report(root, {'status': 'passed'})
            self.assertFalse((root / 'completed-gates.json').exists())


if __name__ == '__main__':
    unittest.main()
