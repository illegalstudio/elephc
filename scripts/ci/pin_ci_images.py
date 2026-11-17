#!/usr/bin/env python3
"""Pin all verified CI image roles from one successful CI Image workflow source SHA."""

import json
from pathlib import Path
import re
import subprocess
import sys

ROLES = ('ci', 'smoke', 'smoke22', 'php-nts', 'php-ts', 'pdo', 'kdc', 'reference84', 'reference85')
REGISTRY = 'ghcr.io/illegalstudio/elephc-ci'


def resolve(sha):
    """Resolve every role before writing, rejecting missing platform manifests."""
    if not re.fullmatch('[0-9a-f]{40}', sha):
        raise ValueError('A complete image workflow source SHA is required')
    images = {}
    for role in ROLES:
        tag = f'{REGISTRY}:sha-{sha}-{role}'
        digest = subprocess.check_output(['docker', 'buildx', 'imagetools', 'inspect', tag,
                                         '--format', '{{.Manifest.Digest}}'], text=True).strip()
        if not re.fullmatch('sha256:[0-9a-f]{64}', digest):
            raise ValueError(f'Invalid digest for {role}')
        reference = f'{REGISTRY}@{digest}'
        manifest = json.loads(subprocess.check_output(
            ['docker', 'buildx', 'imagetools', 'inspect', reference, '--raw'], text=True))
        platforms = {(item.get('platform', {}).get('os'), item.get('platform', {}).get('architecture'))
                     for item in manifest.get('manifests', [])}
        expected = {('linux', 'amd64')} if role in ('pdo', 'kdc', 'reference84', 'reference85') else {('linux', 'amd64'), ('linux', 'arm64')}
        if platforms != expected:
            raise ValueError(f'Unexpected image platforms for {role}: {platforms}')
        images[role] = reference
    images['pdo-core'] = images['ci']
    return images


def main():
    """Update the committed catalog only when every image is available and verified."""
    images = resolve(sys.argv[1])
    Path(__file__).with_name('images.json').write_text(json.dumps(images, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
