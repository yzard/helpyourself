import importlib.util
import os
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('ocr_key', ROOT / 'src/development/ocr_key.py')
ocr_key = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ocr_key)


class CredentialPreparationTests(unittest.TestCase):
    def test_key_and_operator_configuration_survive_repeated_start(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            key, config = root / 'secrets/key', root / 'ocr.toml'
            template = ROOT / 'src/backend_ocr/config.toml'
            ocr_key.prepare(key, config, template)
            original = key.read_bytes()
            self.assertEqual(os.stat(key).st_mode & 0o777, 0o600)
            config.write_text('operator settings')
            ocr_key.prepare(key, config, template)
            self.assertEqual(key.read_bytes(), original)
            self.assertEqual(config.read_text(), 'operator settings')

    def test_invalid_existing_key_is_not_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            key = root / 'key'
            key.write_text('invalid')
            with self.assertRaises(ValueError):
                ocr_key.prepare(key, root / 'ocr.toml', ROOT / 'src/backend_ocr/config.toml')
            self.assertEqual(key.read_text(), 'invalid')
