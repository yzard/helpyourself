import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class IOSBuildEntrypointTests(unittest.TestCase):
    def test_linux_never_reports_a_full_ios_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary)
            uname = binary / "uname"
            uname.write_text("#!/bin/sh\necho Linux\n")
            uname.chmod(0o755)
            environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ["PATH"])
            result = subprocess.run(
                [str(ROOT / "build_ios.sh")], env=environment, cwd="/", capture_output=True, text=True
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("require macOS/Xcode", result.stderr)

    def test_invalid_options_fail_before_any_build(self):
        for options in [["--unknown"], ["--test-destination"], ["--core-only", "--test-destination", "simulator"]]:
            result = subprocess.run([str(ROOT / "build_ios.sh"), *options], capture_output=True)
            self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
