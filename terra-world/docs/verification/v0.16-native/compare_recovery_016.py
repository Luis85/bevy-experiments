#!/usr/bin/env python3
"""Read-only, pre-open comparison of the native 0.16 recovery review fixture.

Run after the launcher reports that it recovered the original Recovery valley
backup, before starting the recovered world. This is review tooling, not game
implementation. It creates only the exclusive JSON result file.
"""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3


FIXTURE = Path('/tmp/terra-native/launcher-review-fixture-016')
DATA = FIXTURE / 'player-data'
BASELINE = None
BACKUP = DATA / 'backups/backup-1793247356-59872-0'
BUNDLE = BACKUP / 'bundle'
OUTPUT = None
EXPECTED_TITLE = None
PROJECT = Path('/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next')


def require(value: bool, reason: str) -> None:
    if not value:
        raise RuntimeError(reason)


def sha(path: Path) -> str:
    require(path.is_file() and not path.is_symlink(), f'Expected regular file: {path}')
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read(path: Path):
    return json.loads(path.read_text())


def reference(path: Path) -> dict:
    return {'path': str(path), 'sha256': sha(path), 'bytes': path.stat().st_size}


def encoded(value):
    if value is None:
        return ['null', None]
    if isinstance(value, bytes):
        return ['blob', value.hex()]
    if isinstance(value, str):
        return ['text', value]
    if isinstance(value, int):
        return ['integer', str(value)]
    if isinstance(value, float):
        return ['real', repr(value)]
    raise TypeError(type(value))


def table_snapshot(path: Path) -> tuple[list, dict]:
    require(path.is_file() and not path.is_symlink(), f'Database missing: {path}')
    for suffix in ('-wal', '-shm', '-journal'):
        require(not Path(str(path) + suffix).exists(), f'Database already opened or has a sidecar: {path}{suffix}')
    # immutable avoids creating sidecars; no writer should exist before Open.
    with sqlite3.connect(path.as_uri() + '?mode=ro&immutable=1', uri=True) as db:
        require(db.execute('PRAGMA integrity_check').fetchall() == [('ok',)], f'Database integrity check failed: {path}')
        schema = db.execute('SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name').fetchall()
        tables = {row[1] for row in schema if row[0] == 'table'}
        require(tables == {'regions', 'world'}, f'Unexpected database tables: {tables}')
        content = {}
        for name, ordering in (('regions', 'level,x,y'), ('world', 'singleton')):
            columns = db.execute(f'PRAGMA table_info({name})').fetchall()
            rows = db.execute(f'SELECT * FROM {name} ORDER BY {ordering}').fetchall()
            content[name] = {'columns': columns, 'rows': rows}
        return schema, content


def main() -> None:
    require(not OUTPUT.exists(), f'Refusing to replace prior evidence: {OUTPUT}')
    baseline = read(BASELINE)
    require(len(baseline) >= 43 and 'controls.json' in baseline, 'Unexpected retained fixture baseline')
    for relative, expected in baseline.items():
        require(sha(DATA / relative) == expected, f'Existing fixture file changed: {relative}')

    old_profiles = {DATA / relative for relative in baseline if relative.startswith('worlds/') and relative.endswith('/world.json')}
    profiles = set((DATA / 'worlds').glob('*/world.json'))
    require(old_profiles and len(profiles) == len(old_profiles) + 1, 'Expected exactly one new managed profile')
    created = profiles - old_profiles
    require(len(created) == 1 and old_profiles.issubset(profiles), 'Restoration replaced or did not add a profile')
    profile_path = created.pop()
    require(profile_path.parent.name.startswith('recovered-'), 'New profile is not a managed recovery')
    profile = read(profile_path)
    wrapper = read(BACKUP / 'world-backup.json')
    bundle_record = read(BUNDLE / 'recovery-bundle.json')
    portable_config = read(BUNDLE / 'world-config.json')
    restored = profile_path.parent / 'world'
    config = profile['config']

    require(wrapper['title'] == 'Recovery valley', 'Wrong selected backup')
    require(wrapper['source_generation'] == bundle_record['source_generation'] == 2, 'Wrong recovery generation')
    require(profile['title'] == EXPECTED_TITLE, 'Wrong recovered title')
    require(profile['checkpoint_present'] is True, 'Recovered profile has no checkpoint')
    for name in ('character', 'quality', 'start_hour'):
        require(profile[name] == wrapper[name], f'Recovered {name} differs from selected backup')
    require(profile['created_unix_seconds'] >= wrapper['created_unix_seconds'], 'Recovered profile creation time predates backup')
    require(profile['last_played_unix_seconds'] == profile['created_unix_seconds'], 'Profile appears to have been played already')

    path_fields = ('cache_dir', 'save_path', 'region_store', 'heightmap_manifest', 'politics_file')
    for field in path_fields:
        actual = Path(config[field])
        require(actual.is_absolute() and actual == restored / portable_config[field], f'Runtime path not correctly rehomed: {field}')
    for field, value in portable_config.items():
        if field not in path_fields:
            require(config[field] == value, f'Non-path configuration changed: {field}')
    require(config['historical_pack'] is None, 'Unexpected external historical dependency')
    require(config == read(restored / 'world-config.json'), 'Managed profile and restored configuration differ')

    copied = []
    for entry in bundle_record['files']:
        relative = entry['path']
        source = BUNDLE / relative
        require(sha(source) == entry['sha256'].removeprefix('sha256:'), f'Bundle receipt does not match file: {relative}')
        require(source.stat().st_size == entry['bytes'], f'Bundle size differs: {relative}')
        if relative.startswith('source/') or relative in ('politics.geojson', 'catalog.json'):
            target = restored / relative
            require(sha(target) == sha(source), f'Restored dependency differs: {relative}')
            copied.append({'path': relative, 'sha256': sha(target), 'bytes': target.stat().st_size})
    expected_source_paths = {entry['path'] for entry in copied if entry['path'].startswith('source/')}
    actual_source_paths = {path.relative_to(restored).as_posix() for path in (restored / 'source').rglob('*') if path.is_file()}
    require(expected_source_paths == actual_source_paths, 'Restored source tree has missing or extra files')

    expected_schema, expected_tables = table_snapshot(BUNDLE / 'world.db')
    actual_schema, actual_tables = table_snapshot(Path(config['region_store']))
    require(actual_schema == expected_schema, 'Database schema differs')
    require(actual_tables == expected_tables, 'Database rows or columns differ')
    table_records = {}
    for name, table in actual_tables.items():
        canonical = json.dumps([[encoded(value) for value in row] for row in table['rows']], separators=(',', ':'), ensure_ascii=True).encode()
        table_records[name] = {
            'rows': len(table['rows']),
            'columns': [column[1] for column in table['columns']],
            'all_values_equal': True,
            'typed_row_content_sha256': hashlib.sha256(canonical).hexdigest(),
        }
    require(table_records['regions']['rows'] == 19 and table_records['world']['rows'] == 1, 'Unexpected retained fixture size')

    retained_receipt = PROJECT / 'docs/verification/v0.16-review/recovery-fixture-receipt.json'
    receipt = read(retained_receipt)
    # The retained test deleted both original directories before restoring its
    # first copy. Recheck those conditions now; original world names use world-*.
    source_absent = not (FIXTURE / 'original-inputs').exists()
    original_worlds_absent = not list((DATA / 'worlds').glob('world-*'))
    require(source_absent and original_worlds_absent, 'An original dependency tree is still present')
    require(receipt.get('report', {}).get('original_world_and_inputs_removed') is True, 'Retained fixture has no original-loss evidence')
    require(profile['config']['base_data_revision'] == bundle_record['source']['source_identity'], 'Source identity changed')
    require(profile['config']['political_dataset_revision'] == bundle_record['politics']['dataset_identity'], 'Political dataset identity changed')

    checks = {
        'exactly_one_additional_managed_profile': True,
        'all_existing_fixture_files_unchanged': True,
        'selected_backup_and_existing_world_retained': True,
        'database_integrity_checks_passed': True,
        'all_database_definitions_exact': True,
        'every_database_row_and_column_exact': True,
        'character_quality_and_start_hour_exact': True,
        'runtime_paths_absolute_and_rehomed': True,
        'non_path_configuration_exact': True,
        'managed_and_backend_configuration_equal': True,
        'prepared_and_political_inputs_byte_exact': True,
        'source_and_political_identities_exact': True,
        'original_input_directory_still_absent': True,
        'original_managed_world_directory_still_absent': True,
        'complete_checkpoint_present_before_open': True,
    }
    result = {
        'release': '0.16.0', 'status': 'passed',
        'completed_utc': datetime.now(timezone.utc).isoformat(),
        'scope': 'Pre-open comparison after native launcher recovery of the original prepared Recovery valley backup. Native opening and final binary acceptance are recorded separately. Original directory deletion is established by the retained fixture test and rechecked here.',
        'profile': str(profile_path),
        'edited_title': EXPECTED_TITLE, 'existing_fixture_files': len(baseline),
        'selected_backup': str(BACKUP),
        'bundle_identity': wrapper['bundle_identity'],
        'source_generation': 2,
        'tables': table_records,
        'copied_dependencies': copied,
        'checks': checks,
        'profile_creation': {
            'original_world_created_unix_seconds': wrapper['world_created_unix_seconds'],
            'backup_created_unix_seconds': wrapper['created_unix_seconds'],
            'recovered_profile_created_unix_seconds': profile['created_unix_seconds'],
            'note': 'The recovered managed profile intentionally has a new creation time; original creation time remains in the backup wrapper.',
        },
        'references': {
            'baseline': reference(BASELINE),
            'retained_fixture_receipt': reference(retained_receipt),
            'profile_before_open': reference(profile_path),
            'wrapper': reference(BACKUP / 'world-backup.json'),
            'restore_receipt': reference(restored / 'restore-receipt.json'),
            'comparison_helper': reference(Path(__file__)),
        },
    }
    with OUTPUT.open('x') as stream:
        stream.write(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'status': 'passed', 'receipt': str(OUTPUT), 'profile': str(profile_path),
        'edited_title': EXPECTED_TITLE, 'existing_fixture_files': len(baseline), 'database_rows': {name: table['rows'] for name, table in table_records.items()}}, indent=2))


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--title', required=True)
    args = parser.parse_args()
    for path in (args.baseline, args.output):
        require(path.parent == Path('/tmp/terra-native') and path.name.startswith('v16-') and path.suffix == '.json', 'Expected a new native 0.16 review path')
    require(0 < len(args.title.strip()) <= 48, 'Expected a valid chosen name')
    BASELINE, OUTPUT, EXPECTED_TITLE = args.baseline, args.output, args.title.strip()
    main()
