#!/usr/bin/env python3
"""Generate a private ASP operator app; provision its private configs before install."""
import argparse
import json
from pathlib import Path
import re
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--image', required=True)
parser.add_argument('--postgres-image', required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--app-id', default='paperclip-asp')
args = parser.parse_args()
for image in (args.image, args.postgres_image):
    if not re.fullmatch(r'[a-z0-9][a-z0-9.:/_-]*:[A-Za-z0-9_.-]+@sha256:[0-9a-f]{64}', image):
        parser.error('Use immutable image references with tag and digest')
if not re.fullmatch(r'[a-z][a-z0-9]*(-[a-z0-9]+)+', args.app_id):
    parser.error('Invalid app ID')
out = args.output
out.mkdir(parents=True, exist_ok=False)
app = args.app_id
manifest = {
    'manifestVersion': 1, 'id': app, 'name': 'Paperclip ASP', 'version': '0.7.2',
    'tagline': 'Private XBT Ark service test', 'category': 'bitcoin', 'port': 38181,
    'description': 'Experimental ASP and watchman with a dedicated PostgreSQL database. Authenticated operator status and explicit first-run initialization. Funded XBT Lightning is available with explicit test configuration, a private CLN hold backend, and channel/pool liquidity.',
    'developer': 'Paperclip', 'website': 'https://github.com/connorslab/paperclip-asp',
    'repo': 'https://github.com/connorslab/paperclip-asp',
    'icon': 'https://ark.paperclippool.xyz/mark.svg',
    'support': 'https://github.com/connorslab/paperclip-asp/issues',
    'dependencies': [], 'gallery': [], 'path': '', 'defaultUsername': '',
    'deterministicPassword': True, 'releaseNotes': 'Private installation test; not production-ready.'
}
compose = {'services': {
    'app_proxy': {'environment': {'APP_HOST': app + '_operator_1', 'APP_PORT': '3000'}},
    'operator': {'image': args.image, 'user': '1000:1000', 'restart': 'on-failure',
        'command': ['python3', '/opt/paperclip/deployment/managed-chain.py'],
        'environment': {'APP_PASSWORD': '${APP_PASSWORD}', 'PAPERCLIP_XBT_MAINNET': '1', 'PAPERCLIP_PRUNED_RPC': '1'},
        'volumes': ['${APP_DATA_DIR}/data/asp:/var/lib/paperclip-asp', '${APP_DATA_DIR}/data/config:/config:ro'],
        'depends_on': {'postgres': {'condition': 'service_healthy'}},
        'cap_drop': ['ALL'], 'security_opt': ['no-new-privileges:true'], 'stop_grace_period': '2m'},
    'postgres': {'image': args.postgres_image, 'restart': 'on-failure',
        'environment': {'PGDATA': '/var/lib/postgresql/data/pgdata',
                        'POSTGRES_DB': 'paperclip_asp', 'POSTGRES_USER': 'paperclip-asp',
                        'POSTGRES_PASSWORD_FILE': '/run/secrets/postgres_password'},
        'volumes': ['${APP_DATA_DIR}/data/postgres:/var/lib/postgresql/data',
                    '${APP_DATA_DIR}/data/secrets/postgres_password:/run/secrets/postgres_password:ro'],
        'healthcheck': {'test': ['CMD-SHELL', 'pg_isready -U paperclip-asp -d paperclip_asp'],
                        'interval': '10s', 'timeout': '5s', 'retries': 12},
        'stop_grace_period': '2m'}
}}
for name, content in [('umbrel-app.yml', manifest), ('docker-compose.yml', compose)]:
    (out / name).write_text(json.dumps(content, indent=2) + '\n')
for guide in ('BUNDLED-PRUNED.md', 'PRUNED-NODES.md'):
    shutil.copyfile(Path(__file__).with_name(guide), out / guide)
for directory in ('asp', 'config', 'postgres', 'secrets'):
    (out / 'data' / directory).mkdir(parents=True)
    (out / 'data' / directory / '.gitkeep').touch()
print('Private app template created; supply private node configuration before installing.')
