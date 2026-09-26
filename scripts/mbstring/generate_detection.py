#!/usr/bin/env python3
"""Generate PHP's version-pinned common-character detection bitmap."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE_SHA256 = '75fae52a0bc589edac4ee4338f638c7e022724fc398331e44b93438942fdb0b1'
SOURCE_URL = 'https://raw.githubusercontent.com/php/php-src/php-8.5.10/ext/mbstring/common_codepoints.txt'


def generate(source: Path) -> None:
    """Build a portable BMP bitmap from the pinned source ranges, retaining data provenance."""
    raw = source.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise ValueError('Unexpected PHP common-codepoint source SHA-256')
    common = bytearray(8192)
    for line in raw.decode().splitlines():
        line = line.split('#', 1)[0].strip()
        if not line:
            continue
        low, high = (int(word, 16) for word in line.split())
        if not 0 <= low <= high <= 0xFFFF:
            raise ValueError(f'Invalid BMP range: {line}')
        for code in range(low, high + 1):
            common[code >> 3] |= 1 << (code & 7)
    directory = ROOT / 'crates/elephc-mbstring/src/detect/data'
    directory.mkdir(parents=True, exist_ok=True)
    (directory / 'common.bin').write_bytes(common)
    manifest = {'php_version': '8.5.10', 'source': SOURCE_URL, 'source_sha256': SOURCE_SHA256,
                'license': 'PHP-3.01', 'common': {'file': 'common.bin', 'sha256': hashlib.sha256(common).hexdigest()}}
    (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    generate(Path(sys.argv[1]))
