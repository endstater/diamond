# SPDX-License-Identifier: GPL-3.0-only
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('clean_binaries', Path(__file__).with_name('clean_binaries.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class CleanerTests(unittest.TestCase):
    def test_preview_then_delete_preserves_resources_and_git(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixtures = {'target/release/diamond': b'\x7fELF' + bytes(100),
                        'library.a': b'!<arch>\n' + bytes(40), 'plugin.wasm': b'\x00asm\x01\x00\x00\x00',
                        'assets/image.png': b'\x89PNG\r\n\x1a\n', 'assets/font.ttf': b'\x00\x01\x00\x00',
                        'assets/diamond.svg': b'<svg/>', 'example.exe': b'ordinary text',
                        '.git/objects/keep': b'\x7fELF' + bytes(100)}
            for name, data in fixtures.items():
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(module.clean(root), 0)
                for name in fixtures: self.assertTrue((root / name).is_file())
                self.assertEqual(module.clean(root, True), 0)
            for name in ['target/release/diamond', 'library.a', 'plugin.wasm']:
                self.assertFalse((root / name).exists())
            for name, data in fixtures.items():
                if (root / name).exists(): self.assertEqual((root / name).read_bytes(), data)

    def test_symlinks_and_installed_dependencies_are_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'project'; root.mkdir()
            outside = Path(directory) / 'outside'; outside.write_bytes(b'\x7fELF' + bytes(100))
            (root / 'link').symlink_to(outside)
            (root / 'linked-directory').symlink_to(root.parent, target_is_directory=True)
            (root / '.venv').mkdir(); (root / '.venv' / 'runtime.so').write_bytes(outside.read_bytes())
            with contextlib.redirect_stdout(io.StringIO()): self.assertEqual(module.clean(root, True), 0)
            self.assertTrue(outside.exists()); self.assertTrue((root / 'link').is_symlink())
            self.assertTrue((root / '.venv' / 'runtime.so').exists())

if __name__ == '__main__':
    unittest.main()
