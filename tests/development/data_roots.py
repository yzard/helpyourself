import fcntl
import importlib.util
import os
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('data_roots', ROOT / 'src/development/data_roots.py')
data_roots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(data_roots)


def fixture(project):
    for name in ['docker/config.toml', 'src/backend_ocr/config.toml']:
        target = project / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / name, target)


class DataRootTests(unittest.TestCase):
    def test_live_server_lock_prevents_layout_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            old = project / 'playground/data'
            old.mkdir(parents=True)
            with (old / 'server.lock').open('a+b') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    data_roots.prepare(project)
            self.assertTrue(old.exists())
            self.assertFalse((project / 'playground/backend_api').exists())

    def test_separate_roots_and_private_stable_keys_preserve_operator_edits(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            data_roots.prepare(project)
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            key = (api / 'ocr-key').read_bytes()
            self.assertEqual(key, (ocr / 'ocr-key').read_bytes())
            self.assertEqual(os.stat(api / 'ocr-key').st_mode & 0o777, 0o600)
            (api / 'config.toml').write_text('operator settings')
            (api / 'database.sqlite').write_bytes(b'synthetic persisted archive')
            data_roots.prepare(project)
            self.assertEqual((api / 'config.toml').read_text(), 'operator settings')
            self.assertEqual((api / 'database.sqlite').read_bytes(), b'synthetic persisted archive')
            self.assertEqual((api / 'ocr-key').read_bytes(), key)
            self.assertFalse((ocr / 'database.sqlite').exists())

    def test_legacy_move_preserves_database_sidecars_raw_files_and_secret(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            old = project / 'playground/data'
            (old / 'raw/photos').mkdir(parents=True)
            for name in ['database.sqlite', 'database.sqlite-wal', 'database.sqlite-shm', 'raw/photos/synthetic']:
                (old / name).write_bytes(name.encode())
            config = project / 'playground/config.toml'
            config.write_text(
                (project / 'docker/config.toml')
                .read_text()
                .replace('[server]', '[server]\ndata_dir = "/data"')
                .replace('api_key_file = "ocr-key"', 'api_key_file = "/secrets/ocr-key"')
            )
            secret = project / 'playground/secrets/ocr-key'
            secret.parent.mkdir()
            secret.write_text('persisted-synthetic-service-key-123456789')
            data_roots.prepare(project)
            api = project / 'playground/backend_api'
            self.assertFalse(old.exists())
            self.assertFalse(config.exists())
            self.assertNotIn('data_dir', (api / 'config.toml').read_text())
            for name in ['database.sqlite', 'database.sqlite-wal', 'database.sqlite-shm', 'raw/photos/synthetic']:
                self.assertEqual((api / name).read_bytes(), name.encode())
            self.assertEqual((api / 'ocr-key').read_text().strip(), 'persisted-synthetic-service-key-123456789')

    def test_conflicting_layout_or_keys_fail_before_moving_data(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            old = project / 'playground/data'
            old.mkdir(parents=True)
            api = project / 'playground/backend_api'
            api.mkdir()
            with self.assertRaises(ValueError):
                data_roots.prepare(project)
            self.assertTrue(old.exists())
            shutil.rmtree(old)
            (api / 'ocr-key').write_text('first-persisted-key-123456789')
            ocr = project / 'playground/backend_ocr'
            ocr.mkdir()
            (ocr / 'ocr-key').write_text('other-persisted-key-123456789')
            with self.assertRaises(ValueError):
                data_roots.prepare(project)
            self.assertFalse((api / 'config.toml').exists())


if __name__ == '__main__':
    unittest.main()
