#!/usr/bin/env python3
"""Generate the shared language catalog from PHP-captured settings and aliases."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def generate() -> None:
    """Write canonical language metadata without consulting system locale databases."""
    fixture = json.loads((ROOT / 'crates/elephc-mbstring/tests/fixtures/languages.json').read_text())
    lines = [
        '//! Purpose:',
        '//! Records PHP language names, aliases, default detection order, and mail encodings.',
        '//!',
        '//! Called from:',
        '//! - `super::language` for shared request settings and mail defaults.',
        '//!',
        '//! Key details:',
        '//! - Regenerate with `python3 scripts/mbstring/generate_languages.py`.',
        '//! - Values come from the PHP language oracle, independently of platform locale.',
        '', 'use super::language::LanguageInfo;', '',
        '/// Complete language catalog, with the default neutral language first.',
        'pub(super) const LANGUAGES: &[LanguageInfo] = &[',
    ]
    for name, info in fixture['languages'].items():
        lines.extend(['    LanguageInfo {', f'        name: {json.dumps(name)},'])
        for field in ['aliases', 'detect_order']:
            values = ', '.join(json.dumps(item) for item in info[field])
            lines.append(f'        {field}: &[{values}],')
        for field in ['mail_charset', 'mail_header_encoding', 'mail_body_encoding']:
            lines.append(f'        {field}: {json.dumps(info[field])},')
        lines.append('    },')
    lines.extend(['];', ''])
    (ROOT / 'crates/elephc-mbstring/src/state/language_data.rs').write_text('\n'.join(lines))


if __name__ == '__main__':
    generate()
