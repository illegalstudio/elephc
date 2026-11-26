"""Regression cases for Memoir Cargo features, source and linked bridge contents."""
from pathlib import Path
import unittest

from scripts.audit_memoir_boundary import (
    MIGRATED_OWNER_FIELDS,
    MIGRATED_SOURCE_CONTRACTS,
    audit_archive_symbols,
    audit_metadata,
    audit_migrated_owner_fields,
    audit_migrated_source_contract,
    audit_source,
    rust_tokens,
)

ROOT = Path("/workspace")
MEMOIR = ROOT / "crates/elephc-memoir"
MAGICIAN = ROOT / "crates/elephc-magician"


def package(name, directory, dependencies=()):
    """Construct the fields Cargo metadata exposes to the boundary audit."""
    return {"id": name, "name": name, "manifest_path": str(directory / "Cargo.toml"),
            "dependencies": list(dependencies), "links": None, "features": {},
            "targets": [{"kind": ["rlib"], "crate_types": ["rlib"],
                         "src_path": str(directory / "src/lib.rs")}]}


def dependency(name="elephc-memoir", directory=MEMOIR, feature="codegen", **overrides):
    """Keep canonical package name separate from its possible Rust alias."""
    result = {"name": name, "path": str(directory), "kind": None,
              "optional": False, "target": None, "rename": None,
              "uses_default_features": False, "features": [feature]}
    result.update(overrides)
    return result


def metadata():
    """Model a compiler, Memoir and a generated-program bridge in one workspace."""
    libc = dependency("libc", optional=True, path=None, features=[], uses_default_features=True)
    memoir = package("elephc-memoir", MEMOIR, [libc])
    memoir["features"] = {"default": [], "codegen": [], "runtime": ["dep:libc"]}
    packages = [package("elephc", ROOT, [dependency()]), memoir,
                package("bridge", ROOT / "crates/bridge"),
                package("elephc-magician", MAGICIAN, [dependency(feature="runtime")])]
    return {"workspace_root": str(ROOT), "workspace_members": [p["id"] for p in packages],
            "packages": packages}


class MetadataBoundaryTests(unittest.TestCase):
    """Exercise Cargo target kinds and the complete local production-consumer graph."""

    def codes(self, data):
        """Return stable finding categories instead of depending on prose."""
        return {f.code for f in audit_metadata(data, ROOT)}

    def test_isolated_compiler_and_magician_features_pass(self):
        self.assertEqual(self.codes(metadata()), set())

    def test_only_optional_libc_is_admitted(self):
        for kind, target in [(None, None), ("dev", None), ("build", None), (None, "cfg(windows)")]:
            with self.subTest(kind=kind, target=target):
                data = metadata()
                data["packages"][1]["dependencies"] = [dependency("other", ROOT / "other", kind=kind, target=target)]
                self.assertIn("dependency", self.codes(data))
        data = metadata()
        data["packages"][1]["dependencies"][0]["optional"] = False
        self.assertIn("dependency", self.codes(data))

    def test_memoir_features_cannot_enable_each_other_or_be_default(self):
        for name, values in [("default", ["codegen"]), ("codegen", ["runtime"]), ("runtime", ["codegen", "dep:libc"])]:
            with self.subTest(name=name):
                data = metadata()
                data["packages"][1]["features"][name] = values
                self.assertIn("features", self.codes(data))

    def test_consumer_features_cannot_enable_the_other_side(self):
        for index, code in [(0, "compiler-link"), (3, "magician-link")]:
            for change in [{"uses_default_features": True}, {"features": ["codegen", "runtime"]}]:
                with self.subTest(index=index, change=change):
                    data = metadata()
                    data["packages"][index]["dependencies"][0].update(change)
                    self.assertIn(code, self.codes(data))

    def test_magician_requires_a_direct_local_runtime_dependency(self):
        data = metadata()
        data["packages"][3]["dependencies"] = []
        self.assertIn("magician-link", self.codes(data))

    def test_magician_indirect_consumers_are_rejected(self):
        data = metadata()
        data["packages"][2]["dependencies"] = [dependency("elephc-magician", MAGICIAN, rename="eval")]
        self.assertIn("runtime-consumer", self.codes(data))

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

    def codes(self, source, root=False, codegen=False):
        """Audit a helper or the crate root without creating filesystem fixtures."""
        name = "lib.rs" if root else "codegen_root.rs" if codegen else "helper.rs"
        return {f.code for f in audit_source(f"crates/elephc-memoir/src/{name}", source)}

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

    def test_crate_and_codegen_roots_require_unconditional_unsafe_policies(self):
        for scope, policy in [({"root": True}, "deny"), ({"codegen": True}, "forbid")]:
            attribute = f"#![{policy}(unsafe_code)]"
            for source in [f"// {attribute}", f'r#"{attribute}"#;',
                           f'#![cfg_attr(feature = "x", {policy}(unsafe_code))]',
                           f"mod nested {{ {attribute} }}"]:
                with self.subTest(source=source):
                    self.assertIn("unsafe-policy", self.codes(source, **scope))
            self.assertEqual(self.codes(f"# ! [ {policy} ( unsafe_code ) ]\npub struct Value;", **scope), set())

    def test_unterminated_comments_and_strings_fail_closed(self):
        for source in ['/* comment', 'r##"missing terminator', '"missing terminator']:
            with self.subTest(source=source):
                self.assertIn("rust-source", self.codes(source))

    def test_diagnostics_retain_source_line(self):
        findings = audit_source("helper.rs", "// note\n\nuse crate::types::PhpType;")
        self.assertTrue(all(f.location == "helper.rs:3" for f in findings))


class MigratedOwnerBoundaryTests(unittest.TestCase):
    """Prevent persistent Magician cells from returning to raw consumer containers."""

    def source(self, replacement=None):
        """Build one minimal context field declaration for every migrated inventory."""
        replacement = replacement or {}
        return "\n".join(
            f"pub(super) {field}: {replacement.get(field, expected)}<usize>,"
            for field, expected in MIGRATED_OWNER_FIELDS.items()
        )

    def test_all_migrated_owner_fields_use_memoir_protocols(self):
        findings = audit_migrated_owner_fields("context/core.rs", self.source())
        self.assertEqual(findings, [])

    def test_raw_owner_container_is_rejected(self):
        findings = audit_migrated_owner_fields(
            "context/core.rs", self.source({"constants": "HashMap"}))
        self.assertEqual({finding.code for finding in findings}, {"owner-container"})

    def test_missing_migrated_inventory_is_rejected(self):
        source = self.source().replace(
            "pub(super) error_handlers: RuntimeOwnerHistory<usize>,", "")
        findings = audit_migrated_owner_fields("context/core.rs", source)
        self.assertEqual({finding.code for finding in findings}, {"owner-field"})


class MigratedSourceContractTests(unittest.TestCase):
    """Keep compiler and Magician consumers on the Memoir APIs they adopted."""

    def test_required_delegation_passes_without_local_policy(self):
        """Accept a consumer that uses every required API and no forbidden primitive."""
        contract = {
            "required": ("memoir::replace_owner(", "memoir::load_header("),
            "forbidden": ("__rt_heap_alloc",),
        }
        source = "memoir::replace_owner(); memoir::load_header();"
        self.assertEqual(audit_migrated_source_contract("consumer.rs", source, contract), [])

    def test_missing_delegation_is_rejected(self):
        """Report each Memoir API that disappears from a migrated consumer."""
        contract = {"required": ("memoir::replace_owner(", "memoir::load_header(")}
        findings = audit_migrated_source_contract(
            "consumer.rs", "memoir::replace_owner();", contract)
        self.assertEqual({finding.code for finding in findings}, {"memoir-delegation"})

    def test_reintroduced_physical_policy_is_rejected(self):
        """Reject direct allocator or runtime-global policy after centralization."""
        contract = {"forbidden": ("__rt_heap_alloc", "_heap_max")}
        findings = audit_migrated_source_contract(
            "consumer.rs", 'emit("__rt_heap_alloc"); emit("_heap_max");', contract)
        self.assertEqual([finding.code for finding in findings],
                         ["consumer-policy", "consumer-policy"])

    def test_repository_contracts_cover_all_eight_migration_groups(self):
        """Guard the owner, descriptor, Throwable, argument, header, symbol, global and tail groups."""
        locations = set(MIGRATED_SOURCE_CONTRACTS)
        witnesses = {
            "crates/elephc-magician/src/scope.rs",
            "src/codegen_support/runtime/callables/closure_copy.rs",
            "src/codegen_support/runtime/exceptions/initialize.rs",
            "src/codegen_support/wrappers/fiber.rs",
            "src/codegen/lower_inst/builtins/isset.rs",
            "src/codegen_support/abi/symbols.rs",
            "src/codegen_support/runtime/strings/bin2hex_len.rs",
            "src/codegen_support/runtime/objects/dump_dynamic_props.rs",
        }
        self.assertTrue(witnesses <= locations)

    def test_owner_publication_and_shutdown_consumers_are_guarded(self):
        """Require both native shutdown routes and Magician's scope cleanup to delegate."""
        for path, helper in {
            "src/codegen/frame.rs": "abi::emit_drain_static_owner(",
            "src/codegen/web.rs": "abi::emit_drain_static_owner(",
            "src/codegen/web/application/shutdown.rs": "abi::emit_drain_static_owner(",
            "src/codegen/lower_inst/objects/property_stores.rs": "symbol_owners::emit_publish_address_owner(",
            "crates/elephc-magician/src/interpreter/scope_cells.rs": "finish_eval_owner_batch(",
        }.items():
            with self.subTest(path=path):
                self.assertIn(helper, MIGRATED_SOURCE_CONTRACTS[path]["required"])

    def test_terminal_and_batch_migrations_are_guarded(self):
        """Keep terminal fixed points, graph cleanup and fallible owner batches centralized."""
        for path, helper in {
            "src/codegen/frame.rs": "terminal_cleanup::emit_fixed_point_pass(",
            "src/codegen/web/application/shutdown.rs": "scope.allow_collection(",
            "src/codegen_support/runtime/emitters/managed.rs": "arrays::emit_published_graph_destructors(",
            "src/codegen/lower_inst/builtins/system.rs": "__rt_exception_cleanup_frames",
            "src/codegen_support/runtime/system/pcntl.rs": "symbol_owners::emit_replace_owner_record(",
            "src/codegen/lower_inst/local_stores.rs": "cleanup_fast_paths::scalar_mixed::emit_local_retirement_gate(",
            "crates/elephc-magician/src/interpreter/builtins/core/runtime_introspection.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/dynamic_functions/argument_owners.rs": "RuntimeOwnerBatch",
            "crates/elephc-magician/src/interpreter/scope_cells.rs": "RuntimeOwnerBatch",
            "crates/elephc-magician/src/interpreter/statements/property_write_receivers.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/statements/property_dimension_writes.rs": "take_at(",
            "crates/elephc-magician/src/interpreter/statements/dimension_writes.rs": "with_runtime_owners(",
            "crates/elephc-magician/src/interpreter/statements/property_dimension_unset.rs": "release_receivers(",
            "crates/elephc-magician/src/ffi/mbstring_array_references.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/expressions/owned.rs": "take_all().release(",
            "crates/elephc-magician/src/interpreter/array_references.rs": "owners.stage(",
            "crates/elephc-magician/src/interpreter/statements/instance_property_access.rs": "Option<&StagedRuntimeOwners>",
            "crates/elephc-magician/src/interpreter/statements/static_property_access.rs": "Option<&StagedRuntimeOwners>",
            "crates/elephc-magician/src/interpreter/output_handlers.rs": "RuntimeOwnerBatch",
            "crates/elephc-magician/src/interpreter/runtime_ops/simplexml_properties.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/statements/local_offset_writes.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/statements/class_resolution.rs": "release_receivers(",
            "crates/elephc-magician/src/interpreter/builtins/registry/owned_arguments.rs": "finish_eval_owner_batch(",
            "crates/elephc-magician/src/interpreter/builtins/registry/binding.rs": "Option<&StagedRuntimeOwners>",
            "crates/elephc-magician/src/interpreter/dynamic_functions.rs": "owners.take_first(",
            "crates/elephc-magician/src/interpreter/dynamic_functions/builtin_arguments.rs": "StagedRuntimeOwners",
            "crates/elephc-magician/src/interpreter/call_argument_owners.rs": "owners.take_all().transfer(",
            "crates/elephc-magician/src/interpreter/scope_cells.rs": "RuntimeOwnerSlot::for_ownership(",
            "crates/elephc-magician/src/interpreter/expressions/evaluation.rs": "eval_owned_dynamic_class_name(",
        }.items():
            with self.subTest(path=path):
                self.assertIn(helper, MIGRATED_SOURCE_CONTRACTS[path]["required"])

        self.assertIn(
            "__elephc_eval_context_drain_owners_v1",
            MIGRATED_SOURCE_CONTRACTS["src/codegen/frame.rs"]["required"],
        )

    def test_mutable_eval_owner_frames_and_callback_transfers_are_guarded(self):
        """Keep growing inventories, VM roots and source-sensitive calls on Memoir protocols."""
        for path, helper in {
            "crates/elephc-memoir/src/runtime_owner_frame.rs": "with_mutable_owner_frame",
            "crates/elephc-memoir/src/runtime_owners.rs": "pub fn pop(&mut self)",
            "crates/elephc-magician/src/runtime_hooks/owner_frames.rs": "with_mutable_runtime_owners",
            "crates/elephc-magician/src/interpreter/bytecode/vm.rs": "impl RuntimeOwners for Vm",
            "crates/elephc-magician/src/interpreter/control.rs": "impl RuntimeOwners for EvaluatedCallArg",
            "crates/elephc-magician/src/context/reference_metadata.rs": "impl RuntimeOwners for EvalReferenceTarget",
            "crates/elephc-magician/src/interpreter/pcntl_escape.rs": "pending.pop()",
            "crates/elephc-magician/src/interpreter/builtins/registry/callable/execution.rs": "arguments.finish(",
            "crates/elephc-magician/src/interpreter/return_values.rs": "finish_eval_argument_values(",
            "crates/elephc-magician/src/interpreter/builtins/core/call_user_func.rs": "callback.borrowed()",
            "crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_free.rs": "RuntimeOwnerSlot",
            "crates/elephc-magician/src/interpreter/statements/loop_statements.rs": "retire_replaced_scope_cells(",
            "crates/elephc-magician/src/interpreter/builtins/symbols/isset.rs": "with_runtime_owners(&result",
            "crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs": "with_runtime_owners(&value",
            "crates/elephc-magician/src/interpreter/statements/constant_evaluation.rs": "with_runtime_owners(&equal",
            "crates/elephc-magician/src/interpreter/statements/native_static_dispatch.rs": "with_runtime_owners(&(&case, &equal)",
            "crates/elephc-magician/src/interpreter/builtins/array/sort.rs": "with_runtime_owners(&result",
            "crates/elephc-magician/src/interpreter/builtins/math/round.rs": "owners.stage(eval_expr(",
            "crates/elephc-magician/src/interpreter/builtins/array/count.rs": "stage_collection_entry",
            "crates/elephc-magician/src/interpreter/builtins/class_metadata.rs": "with_collection_cells",
            "crates/elephc-magician/src/interpreter/builtins/core/object_inventory.rs": "owners.stage(values.reflection_property_names(",
            "crates/elephc-magician/src/interpreter/builtins/symbols/is_a.rs": "owners.stage(values.array_get(",
            "crates/elephc-magician/src/interpreter/builtins/symbols/get_object_vars.rs": "result.with_values(",
            "crates/elephc-magician/src/interpreter/reflection/member_construction.rs": "owners.stage(values.object_property_iter_key(",
            "crates/elephc-magician/src/interpreter/statements/array_append.rs": "with_collection_result",
            "crates/elephc-magician/src/interpreter/statements/callable_objects.rs": "EvalArrayBuilder",
            "crates/elephc-magician/src/interpreter/runtime_ops/object_operators.rs": "owners.finish(",
            "crates/elephc-magician/src/interpreter/generics/types.rs": "owners.stage(values.array_iter_value(",
            "crates/elephc-magician/src/interpreter/builtins/json/json_decode.rs": "with_collection_result",
            "crates/elephc-magician/src/ffi/execute.rs": "with_runtime_scope_owners",
            "crates/elephc-magician/src/interpreter/include_exec.rs": "with_runtime_owners(&source",
            "crates/elephc-magician/src/interpreter/expressions/control_operators.rs": "with_runtime_owners(&thrown",
            "crates/elephc-magician/src/interpreter/statements/property_dimension_guards.rs": "with_runtime_owners(&current",
            "crates/elephc-magician/src/interpreter/builtins/array/iterator_apply.rs": "with_eval_operands",
            "crates/elephc-magician/src/interpreter/builtins/core/print_r.rs": "with_eval_value_lease",
            "crates/elephc-magician/src/interpreter/builtins/core/var_dump.rs": "with_eval_value_lease",
            "crates/elephc-magician/src/interpreter/builtins/symbols/function_exists.rs": "with_runtime_owners(&(&value, &result)",
            "crates/elephc-magician/src/interpreter/statements/const_declarations.rs": "with_runtime_owners(&value",
            "crates/elephc-magician/src/interpreter/statements/dispatch.rs": "with_runtime_owners(&(&current, &value)",
            "crates/elephc-magician/src/interpreter/statements/core_attributes.rs": "with_runtime_owners(&result",
            "crates/elephc-magician/src/interpreter/reflection/owner_materialization.rs": "with_runtime_owners(&closure",
            "crates/elephc-magician/src/interpreter/statements/reference_writeback.rs": "eval_release_value(context, values, replaced)",
        }.items():
            with self.subTest(path=path):
                self.assertIn(helper, MIGRATED_SOURCE_CONTRACTS[path]["required"])


class ArchiveBoundaryTests(unittest.TestCase):
    """Distinguish linked Rust runtime functions from compiler emitters."""

    runtime = "\n".join(f"00000000 T _ZN13elephc_memoir12eval_buffers{len(name)}{name}17hashE"
                        for name in ("allocate", "resolve", "release"))

    def test_runtime_only_archive_passes(self):
        self.assertEqual(audit_archive_symbols("libelephc_magician.a", self.runtime), [])

    def test_archive_requires_the_memoir_runtime(self):
        self.assertEqual({f.code for f in audit_archive_symbols("libelephc_magician.a", "")}, {"archive-runtime"})

    def test_monomorphized_registry_helpers_do_not_satisfy_public_boundary(self):
        helper = "00000000 T _RINv13elephc_memoir12eval_buffers13with_registry8allocate"
        findings = audit_archive_symbols("libelephc_magician.a", helper)
        self.assertEqual(len(findings), 3)

    def test_archive_rejects_codegen_even_with_the_runtime(self):
        symbols = self.runtime + "\n00000000 T _ZN13elephc_memoir12codegen_root10heap_alloc"
        self.assertEqual({f.code for f in audit_archive_symbols("libelephc_magician.a", symbols)}, {"archive-codegen"})


if __name__ == "__main__":
    unittest.main()
