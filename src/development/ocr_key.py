"""Prepare private playground credentials without overwriting existing settings."""

import argparse
import os
import secrets
from pathlib import Path


def prepare(key: Path, config: Path, template: Path) -> None:
    key.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not key.exists():
        descriptor = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'w') as output:
            output.write(secrets.token_urlsafe(32) + '\n')
            output.flush()
            os.fsync(output.fileno())
    if not key.is_file() or not 24 <= len(key.read_text().strip()) <= 8192:
        raise ValueError('Existing OCR key is invalid; it was not overwritten')
    if not config.exists():
        config.parent.mkdir(parents=True, exist_ok=True)
        with config.open('x') as output:
            output.write(template.read_text())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--key', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--template', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.key, args.config, args.template)


if __name__ == '__main__':
    main()
