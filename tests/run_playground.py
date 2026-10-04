"""Test the foreground entrypoint contract without touching a real Docker stack."""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class PlaygroundEntrypointTests(unittest.TestCase):
    def test_build_failure_prevents_start_and_success_uses_foreground(self):
        for fail in (True, False):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as temporary:
                project = Path(temporary)
                (project / "src/development").mkdir(parents=True)
                shutil.copy(ROOT / "src/development/playground.py", project / "src/development/playground.py")
                shutil.copy(ROOT / "src/development/data_roots.py", project / "src/development/data_roots.py")
                (project / "docker").mkdir()
                shutil.copy(ROOT / "docker/config.toml", project / "docker/config.toml")
                (project / "src/backend_ocr").mkdir(parents=True)
                shutil.copy(ROOT / "src/backend_ocr/config.toml", project / "src/backend_ocr/config.toml")
                shutil.copy(ROOT / "run_playground.sh", project / "run_playground.sh")
                build = project / "build_docker.sh"
                build.write_text("#!/bin/sh\nexit " + ("7" if fail else "0") + "\n")
                build.chmod(0o755)
                binary = project / "bin"
                binary.mkdir()
                trace = project / "trace.jsonl"
                docker = binary / "docker"
                config = {
                    "services": {
                        "caddy": {
                            "ports": [{"target": 443, "published": "24443"}],
                            "environment": {"HELPYOURSELF_DOMAIN": "localhost"},
                        }
                    }
                }
                docker.write_text(
                    "#!/usr/bin/env python3\nimport sys,json,os\nfrom pathlib import Path\n"
                    + f"with Path({str(trace)!r}).open('a') as log: log.write(json.dumps(sys.argv[1:])+'\\n')\n"
                    + f"if 'config' in sys.argv: print({json.dumps(config)!r})\n"
                )
                docker.chmod(0o755)
                environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ["PATH"])
                result = subprocess.run(
                    [str(project / "run_playground.sh")], cwd="/", env=environment, capture_output=True, text=True
                )
                if fail:
                    self.assertEqual(result.returncode, 7)
                    self.assertFalse(trace.exists())
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    commands = [json.loads(line) for line in trace.read_text().splitlines()]
                    self.assertEqual(commands[-1][-2:], ["up", "--no-build"])
                    self.assertNotIn("-d", commands[-1])
                    self.assertIn(str(project / "docker/docker-compose.yaml"), commands[-1])
                    self.assertIn("--project-name", commands[-1])
                    self.assertIn("https://localhost:24443", result.stdout)

    def test_arguments_are_rejected_before_start(self):
        result = subprocess.run([str(ROOT / "run_playground.sh"), "up"], capture_output=True)
        self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
