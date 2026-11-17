"""Specialized archives preserve all scheduler and timeout settings."""

from pathlib import Path
import tomllib
import unittest

from prepare_variant_config import configure


class VariantConfigurationTests(unittest.TestCase):
    def test_only_archive_inclusions_change(self):
        root = Path(__file__).resolve().parents[2]
        source = (root / ".config/nextest.toml").read_text()
        original = tomllib.loads(source)
        includes = original["profile"]["ci"].pop("archive")["include"]
        for variant in ("pdo", "tools"):
            result = tomllib.loads(configure(source, variant))
            actual = result["profile"]["ci"].pop("archive", {}).get("include", [])
            self.assertEqual(result, original)
            expected = [] if variant == "tools" else [item for item in includes if item["path"] != "debug/elephc-oniguruma"]
            self.assertEqual(actual, expected)
