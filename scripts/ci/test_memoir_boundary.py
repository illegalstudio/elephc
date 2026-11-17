"""Regression cases for Memoir's compiler-only dependency and source boundary."""
from pathlib import Path
import unittest

from scripts.audit_memoir_boundary import audit_metadata, audit_source, rust_tokens

ROOT = Path("/workspace")
MEMOIR = ROOT / "crates/elephc-memoir"


def package(name, directory, dependencies=()):
    """Construct the fields Cargo metadata exposes to the boundary audit."""
    return {"id": name, "name": name, "manifest_path": str(directory / "Cargo.toml"),
            "dependencies": list(dependencies), "links": None,
            "targets": [{"kind": ["rlib"], "crate_types": ["rlib"],
                         "src_path": str(directory / "src/lib.rs")}]}


def dependency(name="elephc-memoir", directory=MEMOIR, **overrides):
    """Keep canonical package name separate from its possible Rust alias."""
    result = {"name": name, "path": str(directory), "kind": None,
              "optional": False, "target": None, "rename": None}
    result.update(overrides)
    return result


def metadata():
    """Model a compiler, Memoir and a generated-program bridge in one workspace."""
    packages = [package("elephc", ROOT, [dependency()]), package("elephc-memoir", MEMOIR),
                package("bridge", ROOT / "crates/bridge")]
    return {"workspace_root": str(ROOT), "workspace_members": [p["id"] for p in packages],
            "packages": packages}


class MetadataBoundaryTests(unittest.TestCase):
    """Exercise Cargo target kinds and the complete local production-consumer graph."""

    def codes(self, data):
        """Return stable finding categories instead of depending on prose."""
        return {f.code for f in audit_metadata(data, ROOT)}

    def test_compiler_only_workspace_passes(self):
        self.assertEqual(self.codes(metadata()), set())

    def test_every_memoir_dependency_kind_is_rejected(self):
        for kind, target in [(None, None), ("dev", None), ("build", None), (None, "cfg(windows)")]:
            with self.subTest(kind=kind, target=target):
                data = metadata()
                data["packages"][1]["dependencies"] = [dependency("other", ROOT / "other", kind=kind, target=target)]
                self.assertIn("dependency", self.codes(data))

    def test_native_library_types_are_rejected(self):
        for types in [["staticlib"], ["cdylib"], ["rlib", "staticlib"], ["proc-macro"], ["lib"]]:
            with self.subTest(types=types):
                data = metadata()
                data["packages"][1]["targets"][0]["crate_types"] = types
                self.assertIn("crate-type", self.codes(data))

    def test_native_archive_hidden_in_an_example_target_is_rejected(self):
        data = metadata()
        data["packages"][1]["targets"].append({"kind": ["example"], "crate_types": ["staticlib"],
                                              "src_path": str(MEMOIR / "examples/runtime.rs")})
        self.assertIn("crate-type", self.codes(data))

    def test_build_scripts_and_native_links_are_rejected(self):
        data = metadata()
        data["packages"][1]["links"] = "native_memory"
        data["packages"][1]["targets"].append({"kind": ["custom-build"], "crate_types": ["bin"],
                                              "src_path": str(MEMOIR / "build.rs")})
        self.assertTrue({"native-link", "native-build"} <= self.codes(data))

    def test_optional_target_specific_and_foreign_compiler_dependencies_are_rejected(self):
        for change in [{"optional": True}, {"target": "cfg(unix)"}, {"path": "/elsewhere/memoir"}]:
            with self.subTest(change=change):
                data = metadata()
                data["packages"][0]["dependencies"][0].update(change)
                self.assertIn("compiler-link", self.codes(data))

    def test_missing_compiler_dependency_is_rejected(self):
        data = metadata()
        data["packages"][0]["dependencies"] = []
        self.assertIn("compiler-link", self.codes(data))

    def test_renamed_target_specific_runtime_dependency_is_rejected(self):
        data = metadata()
        data["packages"][2]["dependencies"] = [dependency(rename="storage", target="cfg(target_os = \"ios\")", optional=True)]
        self.assertIn("runtime-consumer", self.codes(data))

    def test_indirect_production_dependency_is_rejected(self):
        data = metadata()
        helper = package("helper", ROOT / "crates/helper", [dependency()])
        data["packages"].append(helper)
        data["workspace_members"].append(helper["id"])
        data["packages"][2]["dependencies"] = [dependency("helper", ROOT / "crates/helper", rename="indirect")]
        failures = [f.location for f in audit_metadata(data, ROOT) if f.code == "runtime-consumer"]
        self.assertEqual(set(failures), {"helper", "bridge"})

    def test_normal_path_through_compiler_is_also_rejected(self):
        data = metadata()
        data["packages"][2]["dependencies"] = [dependency("elephc", ROOT)]
        self.assertIn("runtime-consumer", self.codes(data))

    def test_dev_and_build_consumers_do_not_link_into_generated_programs(self):
        data = metadata()
        data["packages"][2]["dependencies"] = [dependency(kind="dev"), dependency(kind="build")]
        self.assertEqual(self.codes(data), set())

    def test_non_memoir_cycles_terminate_without_false_positives(self):
        data = metadata()
        data["packages"][2]["dependencies"] = [dependency("bridge", ROOT / "crates/bridge")]
        self.assertEqual(self.codes(data), set())

    def test_wrong_workspace_or_missing_local_package_is_rejected(self):
        data = metadata()
        data["workspace_root"] = "/different"
        self.assertEqual(self.codes(data), {"workspace"})
        data = metadata()
        data["packages"][1]["manifest_path"] = "/different/Cargo.toml"
        self.assertIn("package", self.codes(data))

    def test_non_workspace_packages_are_not_production_consumers(self):
        data = metadata()
        data["packages"].append(package("external-test-fixture", ROOT / "external", [dependency()]))
        self.assertEqual(self.codes(data), set())


class SourceBoundaryTests(unittest.TestCase):
    """Ensure lexical trivia cannot hide coupling or create false positives."""

    def codes(self, source, root=False):
        """Audit a helper or the crate root without creating filesystem fixtures."""
        path = "crates/elephc-memoir/src/lib.rs" if root else "crates/elephc-memoir/src/helper.rs"
        return {f.code for f in audit_source(path, source)}

    def test_comments_and_all_string_forms_are_ignored(self):
        source = r'''// use crate::types::PhpType;
/* nested /* #[no_mangle] */ RuntimeFnId */
let a = "escaped \\\" PhpType #[link]";
let b = r###"BuiltinId #[export_name = "x"]"###;
let c = br#"RuntimeFnId"#;
let d = c"PhpType";
let e = cr##"crate::ir::RuntimeFnId"##;
'''
        self.assertEqual(self.codes(source), set())

    def test_lifetimes_do_not_swallow_type_identifiers(self):
        self.assertIn("php-eir", self.codes("fn view<'a>(value: &'a PhpType) {}"))

    def test_characters_are_skipped_and_following_types_are_still_checked(self):
        source = r'''let a = 'x'; let b = '\''; let c = '\u{3b1}'; let d = b'\x41'; let p: PhpType;'''
        self.assertIn("php-eir", self.codes(source))
        tokens = [t.text for t in rust_tokens("let c = '{'; fn view<'a>(value: &'a str) {}")]
        self.assertEqual(tokens.count("{"), 1)  # Only the function body, not the character literal.
        self.assertIn("view", tokens)

    def test_php_and_eir_types_are_rejected(self):
        for name in ["PhpType", "BuiltinId", "RuntimeFnId"]:
            with self.subTest(name=name):
                self.assertIn("php-eir", self.codes(f"use neutral::{name};"))

    def test_compiler_paths_including_raw_identifiers_are_rejected(self):
        for name in ["types", "codegen", "codegen_support", "ir", "ir_lower", "pipeline", "r#types"]:
            with self.subTest(name=name):
                self.assertIn("compiler-module", self.codes(f"use crate::{name}::Value;"))

    def test_native_export_attributes_and_cfg_wrappers_are_rejected(self):
        for attribute in ['#[no_mangle]', '#[unsafe(no_mangle)]', '#[link(name = "c")]',
                          '#[export_name = "entry"]', '#![crate_type = "staticlib"]',
                          '#[cfg_attr(feature = "native", no_mangle)]', '#[proc_macro]']:
            with self.subTest(attribute=attribute):
                self.assertIn("native-export", self.codes(attribute + " fn entry() {}"))

    def test_attribute_literal_text_does_not_create_exports(self):
        self.assertEqual(self.codes('#[doc = "#[link] PhpType"] #[cfg(feature = "no_mangle")] fn f() {}'), set())

    def test_crate_root_requires_real_unconditional_unsafe_policy(self):
        for source in ['// #![forbid(unsafe_code)]', 'r#"\n#![forbid(unsafe_code)]\n"#;',
                       '#![cfg_attr(feature = "x", forbid(unsafe_code))]',
                       'mod nested { #![forbid(unsafe_code)] }']:
            with self.subTest(source=source):
                self.assertIn("unsafe-policy", self.codes(source, root=True))
        self.assertEqual(self.codes('# ! [ forbid ( unsafe_code ) ]\npub struct Value;', root=True), set())

    def test_unterminated_comments_and_strings_fail_closed(self):
        for source in ['/* comment', 'r##"missing terminator', '"missing terminator']:
            with self.subTest(source=source):
                self.assertIn("rust-source", self.codes(source))

    def test_diagnostics_retain_source_line(self):
        findings = audit_source("helper.rs", "// note\n\nuse crate::types::PhpType;")
        self.assertTrue(all(f.location == "helper.rs:3" for f in findings))


if __name__ == "__main__":
    unittest.main()
