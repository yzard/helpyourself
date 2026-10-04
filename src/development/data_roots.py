"""Initialize service TOMLs and embed existing credentials without changing their values."""

import argparse
import fcntl
import json
import os
import re
import secrets
import tempfile
import tomllib
from contextlib import ExitStack
from pathlib import Path


def update_fields(text: str, changes: dict[tuple[str, str], str | None]) -> str:
    """Edit known single-line settings while preserving unrelated operator settings/comments."""
    pending = dict(changes)
    section = ''
    sections = {''}
    result = []

    def append_missing() -> None:
        for owner, name in list(pending):
            if owner == section:
                value = pending.pop((owner, name))
                if value is not None:
                    result.append(f'{name} = {json.dumps(value, ensure_ascii=False)}\n')

    for line in text.splitlines(keepends=True):
        header = re.match(r'\s*\[([^]]+)\]', line)
        if header:
            append_missing()
            section = header[1]
            sections.add(section)
        field = re.match(r'\s*([a-zA-Z_]\w*)\s*=', line)
        key = (section, field[1]) if field else None
        if key in pending:
            value = pending.pop(key)
            if value is not None:
                result.append(f'{field[1]} = {json.dumps(value, ensure_ascii=False)}\n')
        else:
            result.append(line if line.endswith('\n') else line + '\n')
    append_missing()
    if any(owner not in sections for owner, _ in pending):
        raise ValueError('Required service configuration section is missing')
    content = ''.join(result)
    tomllib.loads(content)
    return content


def migrated_config(source: Path, api: bool) -> str:
    text = source.read_text()
    parsed = tomllib.loads(text)
    changes = {}
    if api:
        old_root = parsed.get('server', {}).get('data_dir')
        if old_root is not None and old_root not in ('/data', 'data', str(source.parent / 'data')):
            raise ValueError('Legacy config uses a custom data root; relocate that directory explicitly first')
        changes[('server', 'data_dir')] = None
    else:
        model = Path(parsed['engine']['path'])
        if not model.is_absolute():
            changes[('engine', 'path')] = str((source.parent / model).resolve())
    return update_fields(text, changes)


def validate_key(value: str, minimum: int) -> str:
    if not isinstance(value, str) or not minimum <= len(value) <= 8192 or not all(33 <= ord(c) <= 126 for c in value):
        raise ValueError('Invalid service credential; no configuration was replaced')
    return value


def embedded_key(source: Path, owner: dict, project: Path, minimum: int) -> tuple[str, list[Path]]:
    inline = owner.get('api_key', '')
    paths = []
    if inline:
        validate_key(inline, minimum)
    if 'api_key_file' in owner:
        value = Path(owner['api_key_file'])
        path = project / 'playground/secrets/ocr-key' if value == Path('/secrets/ocr-key') else value
        if not path.is_absolute():
            path = source.parent / path
        file_key = validate_key(path.read_text().strip(), minimum)
        if inline and inline != file_key:
            raise ValueError('Inline and file credentials differ; neither was replaced')
        inline = file_key
        paths.append(path.resolve())
    return inline, paths


def write_private(path: Path, content: str) -> None:
    descriptor, temporary_name = tempfile.mkstemp(prefix='.config-', suffix='.tmp', dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, 'w') as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        temporary.unlink(missing_ok=True)


def needs_update(project: Path) -> bool:
    legacy = ['data', 'config.toml', 'backend_ocr.toml', 'secrets/ocr-key']
    if any((project / 'playground' / name).exists() for name in legacy):
        return True
    for service, section in [('backend_api', 'ocr'), ('backend_ocr', 'server')]:
        root = project / 'playground' / service
        path = root / 'config.toml'
        if not path.exists() or (root / 'ocr-key').exists():
            return True
        config = tomllib.loads(path.read_text())
        if service == 'backend_api' and 'enabled' in config.get('ocr', {}):
            return True
        if not config.get(section, {}).get('api_key'):
            return True
        if any(
            'api_key_file' in owner
            for owner in [config.get(section, {}), config.get('providers', {}).get('analysis', {})]
        ):
            return True
    return False


def prepare(project: Path) -> None:
    if not project.is_absolute():
        raise ValueError('Project path must be absolute')
    changing = needs_update(project)
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
    if legacy_data.exists() and (legacy_data / 'config.toml').exists():
        raise ValueError('Legacy data already contains config.toml; resolve this conflict first')
    configs = [
        (api, legacy_api, project / 'docker/config.toml', True),
        (ocr, legacy_ocr, project / 'src/backend_ocr/config.toml', False),
    ]
    planned = []
    keys = []
    obsolete = set()
    for root, old, template, is_api in configs:
        target = root / 'config.toml'
        if old.exists() and target.exists():
            raise ValueError('Both old and new configurations exist; neither was changed')
        source = old if old.exists() else target if target.exists() else template
        content = migrated_config(source, is_api) if old.exists() else source.read_text()
        parsed = tomllib.loads(content)
        section = 'ocr' if is_api else 'server'
        key, paths = embedded_key(source, parsed[section], project, 24)
        if key:
            keys.append(key)
        obsolete.update(paths)
        changes = {(section, 'api_key_file'): None}
        if is_api:
            changes[('ocr', 'enabled')] = None
            provider_key, paths = embedded_key(source, parsed['providers']['analysis'], project, 1)
            obsolete.update(paths)
            changes.update(
                {('providers.analysis', 'api_key_file'): None, ('providers.analysis', 'api_key'): provider_key}
            )
        planned.append((target, old, content, section, changes))
    for path in [api / 'ocr-key', ocr / 'ocr-key', playground / 'secrets/ocr-key']:
        if path.exists():
            keys.append(validate_key(path.read_text().strip(), 24))
            obsolete.add(path.resolve())
    if len(set(keys)) > 1:
        raise ValueError('Existing OCR keys differ; no configurations or data were replaced')
    key = keys[0] if keys else secrets.token_urlsafe(32)
    # Validate every planned TOML before moving or replacing any data/configuration.
    outputs = []
    for target, old, content, section, changes in planned:
        changes[(section, 'api_key')] = key
        output = update_fields(content, changes)
        if tomllib.loads(output)[section]['api_key'] != key:
            raise ValueError('Embedded credential verification failed')
        outputs.append((target, old, output, section))
    if legacy_data.exists():
        legacy_data.rename(api)
    for root in (api, ocr):
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
    for target, old, output, section in outputs:
        if not target.exists() or target.read_text() != output:
            write_private(target, output)
        os.chmod(target, 0o600)
    for target, _, _, section in outputs:
        if tomllib.loads(target.read_text())[section]['api_key'] != key:
            raise ValueError('Persisted credential verification failed; old files were retained')
    for _, old, _, _ in outputs:
        old.unlink(missing_ok=True)
    for path in obsolete:
        # A referenced external credential may belong to another project; remove only ours.
        if path.is_relative_to(playground.resolve()):
            path.unlink(missing_ok=True)
    secrets_directory = playground / 'secrets'
    if secrets_directory.exists() and not any(secrets_directory.iterdir()):
        secrets_directory.rmdir()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--needs-update', action='store_true')
    arguments = parser.parse_args()
    try:
        if arguments.needs_update:
            print('yes' if needs_update(arguments.project) else 'no')
        else:
            prepare(arguments.project)
    except (ValueError, OSError):
        parser.error('Service configuration preparation failed; check layout, credentials and exclusive data access')


if __name__ == '__main__':
    main()
