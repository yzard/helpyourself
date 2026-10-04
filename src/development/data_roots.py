"""Initialize separate service roots and preserve the previous playground layout."""

import argparse
import fcntl
import json
import os
import re
import secrets
import tomllib
from contextlib import ExitStack
from pathlib import Path


def migrated_config(source: Path, api: bool) -> str:
    text = source.read_text()
    parsed = tomllib.loads(text)
    if api:
        old_root = parsed.get('server', {}).get('data_dir')
        if old_root is not None and old_root not in ('/data', 'data', str(source.parent / 'data')):
            raise ValueError('Legacy config uses a custom data root; relocate that directory explicitly first')
    section = ''
    result = []
    for line in text.splitlines(keepends=True):
        header = re.match(r'\s*\[([^]]+)\]', line)
        if header:
            section = header[1]
        if api and section == 'server' and re.match(r'\s*data_dir\s*=', line):
            continue
        match = re.match(r'(\s*(api_key_file|path)\s*=\s*)(["\'])(.*?)(\3)(.*)', line)
        if match and (match[2] == 'api_key_file' or (not api and section == 'engine')):
            owner = parsed
            for part in section.split('.'):
                owner = owner[part]
            value = owner[match[2]]
            if (
                value in ('/secrets/ocr-key', 'secrets/ocr-key', 'ocr-key')
                or Path(value) == source.parent / 'secrets/ocr-key'
            ):
                value = 'ocr-key'
            elif not Path(value).is_absolute():
                value = str((source.parent / value).resolve())
            line = match[1] + json.dumps(value, ensure_ascii=False) + match[6] + '\n'
        result.append(line)
    return ''.join(result)


def write_private(path: Path, content: str) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'w') as output:
        output.write(content)
        output.flush()
        os.fsync(output.fileno())


def prepare(project: Path) -> None:
    if not project.is_absolute():
        raise ValueError('Project path must be absolute')
    roots = [project / 'playground/backend_api', project / 'playground/backend_ocr']
    old_paths = [
        project / 'playground/data',
        project / 'playground/config.toml',
        project / 'playground/backend_ocr.toml',
        project / 'playground/secrets/ocr-key',
    ]
    changing = any(path.exists() for path in old_paths) or any(
        not (root / name).exists() for root in roots for name in ('config.toml', 'ocr-key')
    )
    with ExitStack() as locks:
        for root in (project / 'playground/data', project / 'playground/backend_api'):
            if changing and root.is_dir():
                lock = locks.enter_context((root / 'server.lock').open('a+b'))
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        prepare_locked(project)


def prepare_locked(project: Path) -> None:
    playground = project / 'playground'
    api, ocr = playground / 'backend_api', playground / 'backend_ocr'
    legacy_data = playground / 'data'
    legacy_api, legacy_ocr = playground / 'config.toml', playground / 'backend_ocr.toml'
    if legacy_data.exists() and api.exists():
        raise ValueError('Both old and new API data roots exist; resolve the conflict without overwriting either')
    configs = [
        (api, legacy_api, project / 'docker/config.toml', True),
        (ocr, legacy_ocr, project / 'src/backend_ocr/config.toml', False),
    ]
    planned = []
    for root, old, template, is_api in configs:
        target = root / 'config.toml'
        if old.exists() and target.exists():
            raise ValueError('Both old and new configurations exist; neither was changed')
        content = migrated_config(old, is_api) if old.exists() else template.read_text()
        tomllib.loads(content)
        planned.append((target, old, content))
    candidates = [api / 'ocr-key', ocr / 'ocr-key', playground / 'secrets/ocr-key']
    keys = [path.read_text().strip() for path in candidates if path.exists()]
    if any(not 24 <= len(key) <= 8192 for key in keys) or len(set(keys)) > 1:
        raise ValueError('Existing OCR keys are invalid or different; none were replaced')
    key = keys[0] if keys else secrets.token_urlsafe(32)
    # The caller must stop the playground before moving a live database.
    if legacy_data.exists():
        if (legacy_data / 'config.toml').exists():
            raise ValueError('Legacy data already contains config.toml; resolve this conflict first')
        legacy_data.rename(api)
    for root in (api, ocr):
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
    for target, old, content in planned:
        if not target.exists():
            write_private(target, content)
        if old.exists():
            old.unlink()
    for path in (api / 'ocr-key', ocr / 'ocr-key'):
        if not path.exists():
            write_private(path, key + '\n')
    legacy_key = playground / 'secrets/ocr-key'
    if legacy_key.exists():
        legacy_key.unlink()
        if not any(legacy_key.parent.iterdir()):
            legacy_key.parent.rmdir()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--project', type=Path, required=True)
    prepare(parser.parse_args().project)


if __name__ == '__main__':
    main()
