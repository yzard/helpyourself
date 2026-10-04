import fcntl
import importlib.util
import os
import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('data_roots', ROOT / 'src/development/data_roots.py')
data_roots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(data_roots)
KEY = 'persisted-synthetic-service-key-123456789'


def fixture(project):
    for name in ['docker/config.toml', 'src/backend_ocr/config.toml']:
        target = project / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / name, target)


def config(root):
    return tomllib.loads((root / 'config.toml').read_text())


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

    def test_separate_roots_and_private_embedded_stable_keys_preserve_operator_edits(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            data_roots.prepare(project)
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            key = config(api)['ocr']['api_key']
            self.assertEqual(key, config(ocr)['server']['api_key'])
            self.assertEqual(os.stat(api / 'config.toml').st_mode & 0o777, 0o600)
            self.assertEqual(os.stat(ocr / 'config.toml').st_mode & 0o777, 0o600)
            text = (api / 'config.toml').read_text().replace('maximum_pdf_pages = 100', 'maximum_pdf_pages = 200')
            (api / 'config.toml').write_text(text)
            (api / 'database.sqlite').write_bytes(b'synthetic persisted archive')
            self.assertFalse(data_roots.needs_update(project))
            data_roots.prepare(project)
            self.assertEqual((api / 'config.toml').read_text(), text)
            self.assertEqual((api / 'database.sqlite').read_bytes(), b'synthetic persisted archive')
            self.assertEqual(config(api)['ocr']['api_key'], key)
            self.assertFalse((api / 'ocr-key').exists())
            self.assertFalse((ocr / 'ocr-key').exists())
            self.assertFalse((ocr / 'database.sqlite').exists())

    def test_legacy_move_preserves_database_sidecars_raw_files_and_embeds_secret(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            old = project / 'playground/data'
            (old / 'raw/photos').mkdir(parents=True)
            for name in ['database.sqlite', 'database.sqlite-wal', 'database.sqlite-shm', 'raw/photos/synthetic']:
                (old / name).write_bytes(name.encode())
            path = project / 'playground/config.toml'
            path.write_text(
                (project / 'docker/config.toml')
                .read_text()
                .replace('[server]', '[server]\ndata_dir = "/data"')
                .replace(
                    'api_key = "" # Set the shared OCR key here; playground initialization generates it.',
                    'api_key_file = "/secrets/ocr-key"',
                )
            )
            secret = project / 'playground/secrets/ocr-key'
            secret.parent.mkdir()
            secret.write_text(KEY)
            data_roots.prepare(project)
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            self.assertFalse(old.exists())
            self.assertFalse(path.exists())
            self.assertNotIn('data_dir', (api / 'config.toml').read_text())
            for name in ['database.sqlite', 'database.sqlite-wal', 'database.sqlite-shm', 'raw/photos/synthetic']:
                self.assertEqual((api / name).read_bytes(), name.encode())
            self.assertEqual(config(api)['ocr']['api_key'], KEY)
            self.assertEqual(config(ocr)['server']['api_key'], KEY)
            self.assertFalse(secret.exists())

    def test_existing_service_configs_embed_all_keys_preserving_values_and_other_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            for service, template in [
                ('backend_api', 'docker/config.toml'),
                ('backend_ocr', 'src/backend_ocr/config.toml'),
            ]:
                root = project / 'playground' / service
                root.mkdir(parents=True)
                content = (
                    (project / template)
                    .read_text()
                    .replace(
                        'api_key = "" # Set the shared OCR key here; playground initialization generates it.',
                        'api_key_file = "ocr-key"',
                    )
                )
                if service == 'backend_api':
                    content = content.replace('[ocr]', '[ocr]\nenabled = false')
                    content = content.replace(
                        'api_key = "" # Empty only for a provider that needs no authentication.',
                        'api_key_file = "analysis-key"',
                    )
                    (root / 'analysis-key').write_text('synthetic-provider-\\"-key')
                (root / 'config.toml').write_text(content)
                (root / 'ocr-key').write_text(KEY + '\n')
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            original = api / 'database.sqlite'
            original.write_bytes(b'unchanged synthetic database')
            self.assertTrue(data_roots.needs_update(project))
            data_roots.prepare(project)
            self.assertEqual(config(api)['ocr']['api_key'], KEY)
            self.assertEqual(config(ocr)['server']['api_key'], KEY)
            self.assertNotIn('enabled', config(api)['ocr'])
            self.assertFalse(config(api)['providers']['analysis']['enabled'])
            self.assertEqual(config(api)['providers']['analysis']['api_key'], 'synthetic-provider-\\"-key')
            self.assertEqual(original.read_bytes(), b'unchanged synthetic database')
            for root in [api, ocr]:
                self.assertNotIn('api_key_file', (root / 'config.toml').read_text())
                self.assertFalse((root / 'ocr-key').exists())
            self.assertFalse((api / 'analysis-key').exists())
            data_roots.prepare(project)
            self.assertEqual(config(api)['ocr']['api_key'], KEY)

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
            self.assertTrue((api / 'ocr-key').exists())

    def test_partial_configuration_replacement_can_resume_without_rotating_keys(self):
        with tempfile.TemporaryDirectory() as temporary:
            from unittest.mock import patch

            project = Path(temporary)
            fixture(project)
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            api.mkdir(parents=True)
            ocr.mkdir()
            for root in [api, ocr]:
                (root / 'ocr-key').write_text(KEY)
            write = data_roots.write_private

            def interrupted(path, content):
                if path.parent == ocr:
                    raise OSError('synthetic persistence failure')
                write(path, content)

            with patch.object(data_roots, 'write_private', interrupted), self.assertRaises(OSError):
                data_roots.prepare(project)
            self.assertEqual(config(api)['ocr']['api_key'], KEY)
            self.assertTrue((api / 'ocr-key').exists())
            data_roots.prepare(project)
            self.assertEqual(config(ocr)['server']['api_key'], KEY)
            self.assertFalse((api / 'ocr-key').exists())

    def test_inline_conflict_and_malformed_config_leave_originals_intact(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            fixture(project)
            data_roots.prepare(project)
            api, ocr = project / 'playground/backend_api', project / 'playground/backend_ocr'
            original = (api / 'config.toml').read_text()
            changed = (ocr / 'config.toml').read_text().replace(config(ocr)['server']['api_key'], KEY)
            (ocr / 'config.toml').write_text(changed)
            with self.assertRaises(ValueError):
                data_roots.prepare(project)
            self.assertEqual((api / 'config.toml').read_text(), original)
            self.assertEqual((ocr / 'config.toml').read_text(), changed)
            (ocr / 'config.toml').write_text('malformed configuration')
            with self.assertRaises(tomllib.TOMLDecodeError):
                data_roots.prepare(project)
            self.assertEqual((api / 'config.toml').read_text(), original)


if __name__ == '__main__':
    unittest.main()
