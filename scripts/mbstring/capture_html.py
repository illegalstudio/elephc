#!/usr/bin/env python3
"""Capture mbstring's HTML entity maps from PHP using the standard HTML5 name inventory."""

import hashlib
import html.entities
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2] / "crates/elephc-mbstring/src/encoding/transfer/html"
PHP = r'''
error_reporting(E_ALL & ~E_DEPRECATED);
$names = json_decode(stream_get_contents(STDIN), true, flags: JSON_THROW_ON_ERROR);
$decoded = $encoded = [];
foreach ($names as $name) {
    $output = mb_convert_encoding('&' . $name . ';', 'UCS-4BE', 'HTML-ENTITIES');
    if (strlen($output) === 4) { $decoded[$name] = unpack('N', $output)[1]; }
}
$hash = hash_init('sha256');
for ($code = 0; $code <= 0x10FFFF; ++$code) {
    $output = mb_convert_encoding(pack('N', $code), 'HTML-ENTITIES', 'UCS-4BE');
    hash_update($hash, pack('V', strlen($output)) . $output);
    if (preg_match('/^&[A-Za-z][A-Za-z0-9]*;$/D', $output)) { $encoded[$code] = $output; }
}
echo json_encode(['php_version' => PHP_VERSION, 'decode' => $decoded, 'encode' => $encoded,
    'encoder_hash' => hash_final($hash)], JSON_THROW_ON_ERROR);
'''


def generate():
    """Write only PHP-accepted names and PHP's independently selected preferred encoder names."""
    names = sorted({name.removesuffix(";") for name in html.entities.html5})
    result = subprocess.run(["php", "-r", PHP], input=json.dumps(names), text=True,
                            capture_output=True, check=True)
    data = json.loads(result.stdout)
    lines = [
        "//! Purpose:",
        "//! Provides PHP-captured named HTML entity mappings for the mbstring transfer codec.",
        "//!",
        "//! Called from:",
        "//! - `super` for named entity decoding and preferred encoder spelling.",
        "//!",
        "//! Key details:",
        "//! - Regenerate with `python3 scripts/mbstring/capture_html.py`.",
        "//! - Decode and encode maps are captured independently from the PHP baseline.",
        "",
        "/// Accepted case-sensitive entity names, sorted lexicographically.",
        "pub(super) const DECODE: &[(&[u8], u32)] = &[",
    ]
    lines.extend(f'    (b"{name}", {code}),'
                 for name, code in sorted(data["decode"].items()))
    lines.extend([
        "];", "", "/// PHP's preferred named output for non-ASCII codepoints, sorted numerically.",
        "pub(super) const ENCODE: &[(u32, &[u8])] = &[",
    ])
    lines.extend(f'    ({code}, b"{value}"),'
                 for code, value in sorted(data["encode"].items(), key=lambda pair: int(pair[0])))
    lines.extend(["];", ""])
    ROOT.mkdir(parents=True, exist_ok=True)
    source = "\n".join(lines)
    (ROOT / "data.rs").write_text(source)
    (ROOT / "manifest.json").write_text(json.dumps({
        "php_version": data["php_version"],
        "name_inventory": "Python standard library html.entities.html5",
        "decoder_names": len(data["decode"]), "encoder_names": len(data["encode"]),
        "generated_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "encoder_hash": data["encoder_hash"],
    }, indent=2) + "\n")


if __name__ == "__main__":
    generate()
