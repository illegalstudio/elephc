"""Regression tests for builtin documentation metadata extraction."""

from pathlib import Path
import unittest

from scripts.docs.elephc_builtins.extract import _builtin_to_dict, _normalize_type, _render_default, build_registry


class BuiltinDocumentationExtractionTests(unittest.TestCase):
    """Pins metadata that must survive from the Rust registry to rendered docs."""

    def test_php_string_defaults_preserve_control_bytes(self) -> None:
        """PHP single quotes cannot represent CRLF or other escaped control defaults."""
        for value, expected in [
            ("\r\n", '"\\r\\n"'),
            ("\x001", '"\\x001"'),
            ('"$value\t\\', '"\\"\\$value\\t\\\\"'),
            ("it's $value", "'it\\'s $value'"),
            ("\\r\\n", "'\\\\r\\\\n'"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(_render_default(value, True), expected)

    def test_neutral_union_alternatives_survive_normalization(self) -> None:
        """False, null, array, and scalar alternatives must remain visible in signatures."""
        for spelling in ["int|false", "string|bool", "array|string|null", "?string"]:
            with self.subTest(spelling=spelling):
                self.assertEqual(_normalize_type(spelling), spelling)

    def test_mbstring_union_returns_survive_registry_serialization(self) -> None:
        """The exported PHP signatures must preserve alternatives consumed by both backends."""
        repo = Path(__file__).resolve().parents[3]
        registry = {item.canonical_name: item for item in build_registry(repo)}
        for name, expected in {
            "mb_strpos": "int|false",
            "mb_ord": "int|false",
            "mb_preferred_mime_name": "string|false",
            "mb_internal_encoding": "string|bool",
        }.items():
            with self.subTest(name=name):
                self.assertEqual(_builtin_to_dict(registry[name])["sig"]["return_type"], expected)

    def test_get_object_vars_examples_survive_registry_serialization(self) -> None:
        """The static builtin example must reach the JSON consumed by the renderer."""
        repo = Path(__file__).resolve().parents[3]
        builtin = next(
            item
            for item in build_registry(repo)
            if item.canonical_name == "get_object_vars"
        )

        exported = _builtin_to_dict(builtin)

        self.assertTrue(exported["examples"])
        self.assertIn("examples/get-object-vars/main.php", exported["examples"][0])


if __name__ == "__main__":
    unittest.main()
