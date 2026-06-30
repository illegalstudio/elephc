//! Purpose:
//! Orchestrates AST-to-EIR lowering for a complete checked program.
//!
//! Called from:
//! - `crate::ir_lower::lower_program()`.
//!
//! Key details:
//! - Declaration bodies are lowered before synthetic `main`; declaration
//!   statements themselves are no-ops inside `main`.
//! - The module is validated before it is returned to CLI/test callers.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{
    validate_module, ExternDecl, ExternParamDecl, Function, Immediate, IrType, Module, Op,
};
use crate::ir_lower::{builtin_datetime, function, LoweringError};
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, ClassInfo, InterfaceInfo, PhpType};

/// Lowers an optimized typed AST program into a validated EIR module.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
) -> Result<Module, LoweringError> {
    let mut module = Module::new(target);
    let constants = crate::codegen::collect_constants(program, target.platform);
    let fiber_return_sigs = crate::ir_lower::fibers::collect_fiber_return_sigs(program);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(program, &mut module, check_result, &constants, &fiber_return_sigs);
    lower_class_like_methods(program, &mut module, check_result, &constants, &fiber_return_sigs);
    lower_property_init_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    lower_builtin_reflection_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    function::lower_main(program, &mut module, check_result, &constants, &fiber_return_sigs);
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    builtin_datetime::lower_referenced_builtin_datetime_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    include_lowered_runtime_features(&mut module);
    validate_module(&module)?;
    Ok(module)
}

/// Copies declaration metadata into the EIR module placeholder tables.
fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.trait_table.names = collect_declared_trait_names(program);
    module.declared_class_names = collect_declared_class_names(program, &check_result.classes);
    module.declared_interface_names =
        collect_declared_interface_names(program, &check_result.interfaces);
    module.declared_trait_names = collect_declared_trait_names(program);
    module.declared_trait_uses = collect_declared_trait_uses(program);
    module.class_infos = check_result.classes.clone();
    module.interface_infos = check_result.interfaces.clone();
    module.enum_infos = check_result.enums.clone();
    module.extern_class_infos = check_result.extern_classes.clone();
    module.packed_class_infos = check_result.packed_classes.clone();
    module.packed_layouts.names = sorted_keys(&check_result.packed_classes);
    module.callable_param_sigs = check_result.callable_param_sigs.clone();
    module.extern_decls = check_result
        .extern_functions
        .values()
        .map(|sig| ExternDecl {
            name: sig.name.clone(),
            params: sig
                .params
                .iter()
                .map(|(name, php_type)| ExternParamDecl {
                    name: name.clone(),
                    ir_type: value_or_void_ir_type(php_type),
                    php_type: php_type.clone(),
                })
                .collect(),
            return_type: value_or_void_ir_type(&sig.return_type),
            return_php_type: sig.return_type.clone(),
            link_libs: sig.library.iter().cloned().collect(),
        })
        .collect();
    module.required_runtime_features =
        crate::codegen::runtime_features_for_program_and_classes(program, &check_result.classes);
}

/// Adds optional runtime features referenced by synthetic or lowered EIR functions.
fn include_lowered_runtime_features(module: &mut Module) {
    let features = lowered_runtime_features(module);
    module.required_runtime_features.regex |= features.regex;
    module.required_runtime_features.phar_archive |= features.phar_archive;
    module.required_runtime_features.descriptor_invoker |= features.descriptor_invoker;
}

/// Derives optional runtime features from the actual EIR instruction stream.
fn lowered_runtime_features(module: &Module) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
    for function in all_lowered_functions(module) {
        for inst in &function.instructions {
            match inst.op {
                Op::BuiltinCall => {
                    if builtin_call_requires_regex(module, inst) {
                        features.regex = true;
                    }
                    if builtin_call_requires_phar_archive(module, function, inst) {
                        features.phar_archive = true;
                    }
                    if builtin_call_requires_descriptor_invoker(module, function, inst) {
                        features.descriptor_invoker = true;
                    }
                }
                Op::ExprCall | Op::CallableDescriptorInvoke => {
                    features.descriptor_invoker = true;
                }
                _ => {}
            }
        }
    }
    features
}

/// Iterates every function-like body already materialized into the EIR module.
fn all_lowered_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a lowered builtin call references the optional regex runtime family.
fn builtin_call_requires_regex(module: &Module, inst: &crate::ir::Instruction) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(name) = module.data.function_names.get(data.as_raw() as usize) else {
        return false;
    };
    is_regex_builtin_name(name)
}

/// Returns true when a lowered builtin call emits PHAR bridge pointer publishing.
fn builtin_call_requires_phar_archive(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(name) = builtin_call_name(module, inst) else {
        return false;
    };
    is_phar_archive_builtin_name(name) && function_belongs_to_phar_archive_helper_class(function)
}

/// Returns true when a class method belongs to a stream/archive helper class.
fn function_belongs_to_phar_archive_helper_class(function: &Function) -> bool {
    let Some((class_name, _)) = function.name.split_once("::") else {
        return false;
    };
    is_phar_archive_helper_class_name(class_name)
}

/// Returns true when a lowered builtin call emits runtime string-callable dispatch.
fn builtin_call_requires_descriptor_invoker(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(name) = builtin_call_name(module, inst) else {
        return false;
    };
    let Some(callback_index) = string_callback_operand_index(name) else {
        return false;
    };
    let Some(callback) = inst.operands.get(callback_index).copied() else {
        return false;
    };
    function
        .value(callback)
        .is_some_and(|value| value.php_type.codegen_repr() == PhpType::Str)
}

/// Returns the canonical builtin name attached to a lowered builtin instruction.
fn builtin_call_name<'a>(module: &'a Module, inst: &crate::ir::Instruction) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns the callback operand index for builtins with runtime string callbacks.
fn string_callback_operand_index(name: &str) -> Option<usize> {
    match crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "array_map" => Some(0),
        "array_filter" | "array_reduce" | "array_walk" | "array_walk_recursive" | "usort"
        | "uksort" | "uasort" | "iterator_apply" | "preg_replace_callback" | "array_find"
        | "array_any" | "array_all" => Some(1),
        "array_udiff" | "array_uintersect" => Some(2),
        _ => None,
    }
}

/// Returns true when a builtin name is lowered through the regex runtime helpers.
fn is_regex_builtin_name(name: &str) -> bool {
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "preg_match" | "preg_match_all" | "preg_replace" | "preg_replace_callback" | "preg_split"
    )
}

/// Returns true when an EIR builtin lowerer can publish PHAR bridge symbols.
fn is_phar_archive_builtin_name(name: &str) -> bool {
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "__elephc_phar_list_entries"
            | "__elephc_phar_get_metadata"
            | "__elephc_phar_get_stub"
            | "__elephc_phar_set_metadata"
            | "__elephc_phar_set_stub"
            | "__elephc_phar_get_file_metadata"
            | "__elephc_phar_set_file_metadata"
            | "__elephc_phar_gzip_archive"
            | "__elephc_phar_bzip2_archive"
            | "__elephc_phar_decompress_archive"
            | "__elephc_phar_sign_openssl"
            | "__elephc_phar_sign_hash"
            | "__elephc_phar_set_zip_password"
            | "__elephc_phar_get_signature_hash"
            | "__elephc_phar_get_signature_type"
            | "file_get_contents"
            | "file_put_contents"
            | "fopen"
    )
}

/// Returns true when a class has generated methods that can route paths through PHAR helpers.
fn is_phar_archive_helper_class_name(name: &str) -> bool {
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "phar" | "phardata" | "splfileobject" | "spltempfileobject"
    )
}

/// Lowers per-class property-default thunks referenced by `_class_propinit_ptrs`.
fn lower_property_init_thunks(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let mut classes = check_result.classes.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        function::lower_property_init_thunk(
            class_name,
            class_info,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Returns deterministic sorted keys for metadata placeholder tables.
fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

/// Collects PHP-visible class and enum names in the order `get_declared_classes()` must expose.
fn collect_declared_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        classes,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                Some(name.as_str())
            }
            _ => None,
        },
    );
    prepend_internal_names(classes.keys(), &user_names)
}

/// Collects PHP-visible interface names in the order `get_declared_interfaces()` must expose.
fn collect_declared_interface_names(
    program: &Program,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        interfaces,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::InterfaceDecl { name, .. } => Some(name.as_str()),
            _ => None,
        },
    );
    prepend_internal_names(interfaces.keys(), &user_names)
}

/// Collects user-declared trait names in source order, including namespace blocks.
fn collect_declared_trait_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => names.push(name.clone()),
            StmtKind::NamespaceBlock { body, .. } => {
                names.extend(collect_declared_trait_names(body));
            }
            _ => {}
        }
    }
    names
}

/// Collects direct trait-use declarations keyed by the declaring trait name.
fn collect_declared_trait_uses(program: &Program) -> HashMap<String, Vec<String>> {
    let mut uses = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name, trait_uses, ..
            } => {
                uses.insert(
                    name.clone(),
                    trait_uses
                        .iter()
                        .flat_map(|trait_use| trait_use.trait_names.iter())
                        .map(|trait_name| trait_name.as_str().to_string())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                uses.extend(collect_declared_trait_uses(body));
            }
            _ => {}
        }
    }
    uses
}

/// Recursively collects source-declared names that are present in checked metadata.
fn collect_program_declared_names<T>(
    program: &Program,
    known: &HashMap<String, T>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    pick: impl Copy + Fn(&Stmt) -> Option<&str>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_declared_names(body, known, seen, out, pick);
            }
            _ => {
                let Some(name) = pick(stmt) else {
                    continue;
                };
                let key = crate::names::php_symbol_key(name);
                let is_known = known.contains_key(name)
                    || known.keys().any(|candidate| {
                        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
                    });
                if is_known && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

/// Prepends deterministic internal names before source-order user declarations.
fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !is_internal_synthetic_class_name(name))
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

/// Returns true for compiler-internal helper classes hidden from PHP introspection.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    crate::names::php_symbol_key(name).starts_with("__elephc")
}

/// Converts a PHP type to EIR storage while preserving true void returns.
fn value_or_void_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}

/// Lowers every function declaration reachable in the statement tree.
fn lower_function_declarations(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                by_ref_return: _,
                name,
                params,
                variadic: _,
                variadic_type: _,
                return_type,
                body,
            } => function::lower_user_function(
                name,
                params,
                return_type.as_ref(),
                body,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            ),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants, fiber_return_sigs);
                for (_, body) in elseif_clauses {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants, fiber_return_sigs);
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = default {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations(try_body, module, check_result, constants, fiber_return_sigs);
                for catch in catches {
                    lower_function_declarations(&catch.body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = finally_body {
                    lower_function_declarations(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            _ => {}
        }
    }
}

/// Lowers concrete class/interface methods, including trait methods flattened into classes.
fn lower_class_like_methods(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(name, methods, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::TraitDecl { .. } => {}
            StmtKind::InterfaceDecl { name, methods, .. } => {
                lower_methods_for_class_like(name, methods, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::EnumDecl { name, methods, .. } => {
                // Enum methods are lowered like class methods on the case singleton; prefer the
                // checker's flattened declarations (with `self` types resolved to the enum).
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(name, methods, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants, fiber_return_sigs);
                for (_, body) in elseif_clauses {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants, fiber_return_sigs);
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = default {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_class_like_methods(try_body, module, check_result, constants, fiber_return_sigs);
                for catch in catches {
                    lower_class_like_methods(&catch.body, module, check_result, constants, fiber_return_sigs);
                }
                if let Some(body) = finally_body {
                    lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
                }
            }
            _ => {}
        }
    }
}

/// Lowers all concrete methods for one class-like declaration.
fn lower_methods_for_class_like(
    class_name: &str,
    methods: &[ClassMethod],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for method in methods {
        if !method.has_body {
            continue;
        }
        let method_key = php_method_key(&method.name);
        if class_method_already_lowered(module, class_name, &method_key, method.is_static) {
            continue;
        }
        function::lower_class_method(
            class_name,
            &method.name,
            method.is_static,
            &method.params,
            method.return_type.as_ref(),
            &method.body,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Lowers the synthetic reflection methods injected by the checker.
fn lower_builtin_reflection_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for class_name in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionFunction",
        "ReflectionParameter",
        "ReflectionNamedType",
    ] {
        lower_builtin_reflection_class_methods(class_name, module, check_result, constants, fiber_return_sigs);
    }
}

/// Lowers all concrete synthetic methods for one builtin reflection class.
fn lower_builtin_reflection_class_methods(
    class_name: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    for method in &class_info.method_decls {
        if !method.has_body {
            continue;
        }
        let generated_body;
        let method_key = crate::names::php_symbol_key(&method.name);
        let body = if class_name == "ReflectionAttribute" && method_key == "newinstance" {
            generated_body =
                crate::codegen::reflection::build_attribute_new_instance_body(&check_result.classes);
            generated_body.as_slice()
        } else if class_name == "ReflectionAttribute" && method_key == "getarguments" {
            // Materialize captured attribute arguments through the normal array
            // lowering (named arguments and associative arrays included) rather
            // than a bespoke codegen path.
            generated_body =
                crate::codegen::reflection::build_attribute_get_arguments_body(&check_result.classes);
            generated_body.as_slice()
        } else {
            &method.body
        };
        function::lower_class_method(
            class_name,
            &method.name,
            method.is_static,
            &method.params,
            method.return_type.as_ref(),
            body,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Lowers the small builtin SPL method slice currently consumed by the EIR backend.
fn lower_referenced_builtin_spl_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    loop {
        let mut methods = referenced_builtin_spl_methods(module);
        methods.sort();
        methods.dedup();
        methods.retain(|(class_name, method_key)| {
            !class_method_already_lowered(module, class_name, method_key, false)
                && !runtime_intrinsic_method_has_wrapper(class_name, method_key, false)
        });
        if methods.is_empty() {
            break;
        }

        let before = module.class_methods.len();
        for (class_name, method_key) in methods {
            lower_builtin_spl_method(&class_name, &method_key, module, check_result, constants, fiber_return_sigs);
        }
        if module.class_methods.len() == before {
            break;
        }
    }
}

/// Finds builtin SPL methods whose symbols are required by already-lowered EIR.
fn referenced_builtin_spl_methods(module: &Module) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            match inst.op {
                Op::ObjectNew => {
                    if let Some(class_name) = class_data_name(module, inst) {
                        let construct_key = php_method_key("__construct");
                        push_supported_builtin_spl_method_for_receiver(
                            &mut methods,
                            module,
                            class_name,
                            &construct_key,
                        );
                        push_builtin_spl_metadata_methods(&mut methods, module, class_name);
                    }
                }
                Op::DynamicObjectNew => {
                    if let Some((fallback_class, required_parent)) =
                        dynamic_object_new_metadata_names(module, inst)
                    {
                        let construct_key = php_method_key("__construct");
                        if is_supported_builtin_spl_method(fallback_class, &construct_key) {
                            methods.push((fallback_class.to_string(), construct_key.clone()));
                        }
                        if is_supported_builtin_spl_method(required_parent, &construct_key) {
                            methods.push((required_parent.to_string(), construct_key));
                        }
                        push_builtin_spl_metadata_methods(&mut methods, module, fallback_class);
                        push_builtin_spl_metadata_methods(&mut methods, module, required_parent);
                    }
                }
                Op::DynamicObjectNewMixed => {
                    let construct_key = php_method_key("__construct");
                    for class_name in module.class_infos.keys() {
                        if !is_dynamic_new_mixed_metadata_candidate(class_name) {
                            continue;
                        }
                        push_supported_builtin_spl_method_for_receiver(
                            &mut methods,
                            module,
                            class_name,
                            &construct_key,
                        );
                        push_builtin_spl_metadata_methods(&mut methods, module, class_name);
                    }
                }
                Op::MethodCall | Op::NullsafeMethodCall => {
                    let Some(receiver) = inst.operands.first().copied() else {
                        continue;
                    };
                    let Some(receiver_ty) = function
                        .value(receiver)
                        .map(|value| value.php_type.codegen_repr())
                    else {
                        continue;
                    };
                    let Some(method_name) = string_data_name(module, inst) else {
                        continue;
                    };
                    let method_key = php_method_key(method_name);
                    match receiver_ty {
                        PhpType::Object(class_name) => {
                            let normalized = class_name.trim_start_matches('\\');
                            push_supported_builtin_spl_method_for_receiver(
                                &mut methods,
                                module,
                                normalized,
                                &method_key,
                            );
                        }
                        // A Mixed/Union receiver dispatches at runtime over every class whose
                        // flattened method set contains this name (mirrors `mixed_method_candidates`
                        // in the EIR backend). Register the builtin SPL implementation behind each
                        // candidate so its vtable slot is emitted; otherwise the runtime class-id
                        // dispatch jumps through a null vtable slot and segfaults. This covers
                        // method calls on a `mixed` value and on foreach values from object
                        // iterators (e.g. DirectoryIterator), which the EIR lowers as Mixed locals.
                        PhpType::Mixed | PhpType::Union(_) => {
                            for (candidate_class, class_info) in &module.class_infos {
                                if class_info.methods.contains_key(&method_key) {
                                    push_supported_builtin_spl_method_for_receiver(
                                        &mut methods,
                                        module,
                                        candidate_class,
                                        &method_key,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    methods
}

/// Returns true when generic `new $class` can emit static metadata for this class.
fn is_dynamic_new_mixed_metadata_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_name(class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_name(class_name)
}

/// Returns true for builtin classes with safe static allocation paths in generic dynamic new.
fn supported_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "callbackfilteriterator"
            | "domainexception"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "logicexception"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "rangeexception"
            | "recursivecallbackfilteriterator"
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionproperty"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfixedarray"
            | "splqueue"
            | "splstack"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Returns true for builtin classes that generic dynamic new must not treat as user classes.
fn known_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "appenditerator"
            | "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "cachingiterator"
            | "callbackfilteriterator"
            | "directoryiterator"
            | "domainexception"
            | "emptyiterator"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "filesystemiterator"
            | "filteriterator"
            | "generator"
            | "globiterator"
            | "infiniteiterator"
            | "internaliterator"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "limititerator"
            | "logicexception"
            | "multipleiterator"
            | "norewinditerator"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "parentiterator"
            | "phar"
            | "phardata"
            | "rangeexception"
            | "recursivearrayiterator"
            | "recursivecachingiterator"
            | "recursivecallbackfilteriterator"
            | "recursivedirectoryiterator"
            | "recursivefilteriterator"
            | "recursiveiteratoriterator"
            | "recursiveregexiterator"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionproperty"
            | "regexiterator"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfileinfo"
            | "splfileobject"
            | "splfixedarray"
            | "splheap"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "splqueue"
            | "splstack"
            | "spltempfileobject"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Adds the supported builtin SPL method owner for a receiver class or one of its parents.
fn push_supported_builtin_spl_method_for_receiver(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
    method_key: &str,
) {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if is_supported_builtin_spl_method(name, method_key) {
            methods.push((name.to_string(), method_key.to_string()));
            return;
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
}

/// Returns the class-name immediate attached to an instruction.
fn class_data_name<'a>(module: &'a Module, inst: &crate::ir::Instruction) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Parses dynamic object factory fallback and required-parent metadata.
fn dynamic_object_new_metadata_names<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<(&'a str, &'a str)> {
    class_data_name(module, inst)?.split_once('|')
}

/// Returns the string immediate attached to an instruction.
fn string_data_name<'a>(module: &'a Module, inst: &crate::ir::Instruction) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Normalizes a PHP method name for metadata lookups.
fn php_method_key(method_name: &str) -> String {
    crate::names::php_symbol_key(method_name)
}

/// Adds builtin SPL methods required by runtime class/interface metadata.
fn push_builtin_spl_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let mut current = Some(class_name);
    while let Some(name) = current {
        push_builtin_spl_interface_metadata_methods(methods, module, name);
        for method_name in required_builtin_spl_metadata_methods(name) {
            let method_key = php_method_key(method_name);
            if is_supported_builtin_spl_method(name, &method_key) {
                methods.push((name.to_string(), method_key));
            }
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
}

/// Adds builtin SPL methods referenced by runtime interface dispatch tables for one class.
fn push_builtin_spl_interface_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let mut seen = HashSet::new();
    let mut stack = class_info.interfaces.iter().map(String::as_str).collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = module.interface_infos.get(interface_name) else {
            continue;
        };
        for method_key in &interface_info.method_order {
            if let Some(impl_class) = class_info.method_impl_classes.get(method_key) {
                if is_supported_builtin_spl_method(impl_class, method_key) {
                    methods.push((impl_class.clone(), method_key.clone()));
                    continue;
                }
            }
            push_supported_builtin_spl_method_for_receiver(
                methods,
                module,
                class_name,
                method_key,
            );
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
}

/// Returns methods needed even when user code does not call them directly.
fn required_builtin_spl_metadata_methods(class_name: &str) -> &'static [&'static str] {
    match class_name {
        "EmptyIterator" => &["current", "key", "next", "rewind", "valid"],
        "ArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        "ArrayObject" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "SplFixedArray" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "jsonSerialize",
        ],
        "InternalIterator" => &["current", "key", "next", "rewind", "valid"],
        "IteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "LimitIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "getPosition",
        ],
        "NoRewindIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "InfiniteIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "FilterIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "CallbackFilterIterator" => &["accept"],
        "CachingIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "hasNext",
            "__toString",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "getCache",
            "count",
        ],
        "AppendIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "MultipleIterator" => &["current", "key", "next", "rewind", "valid"],
        "__ElephcAppendIteratorArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        "SplDoublyLinkedList" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "SplHeap" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplMaxHeap" | "SplMinHeap" => &["compare"],
        "SplPriorityQueue" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplObjectStorage" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "RegexIterator" => &["accept", "current", "key"],
        "RecursiveArrayIterator" => &["hasChildren", "getChildren"],
        "RecursiveFilterIterator" => &["hasChildren"],
        "RecursiveCallbackFilterIterator" => &["hasChildren", "getChildren"],
        "RecursiveRegexIterator" => &["accept", "current", "key", "hasChildren", "getChildren"],
        "ParentIterator" => &["accept", "getChildren"],
        "RecursiveIteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "SplFileInfo" => &["__toString"],
        "SplFileObject" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "hasChildren",
            "getChildren",
        ],
        "DirectoryIterator" => &["current", "key", "next", "rewind", "valid", "seek"],
        "FilesystemIterator" => &["current", "key"],
        "GlobIterator" => &["count"],
        "RecursiveDirectoryIterator" => &["hasChildren", "getChildren"],
        "RecursiveCachingIterator" => &["hasChildren", "getChildren"],
        _ => &[],
    }
}

/// Returns true for builtin SPL methods intentionally lowered into EIR today.
fn is_supported_builtin_spl_method(class_name: &str, method_key: &str) -> bool {
    match class_name {
        "SplFileInfo" => matches!(
            method_key,
            "__construct"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
                | "getfileinfo"
                | "getpathinfo"
                | "setinfoclass"
                | "openfile"
                | "setfileclass"
        ),
        "SplFileObject" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "haschildren"
                | "getchildren"
                | "eof"
                | "fgets"
                | "getcurrentline"
                | "fgetc"
                | "fread"
                | "fwrite"
                | "ftruncate"
                | "ftell"
                | "fseek"
                | "getflags"
                | "setflags"
                | "getmaxlinelen"
                | "setmaxlinelen"
                | "setcsvcontrol"
                | "fgetcsv"
                | "fputcsv"
        ),
        "SplTempFileObject" => matches!(
            method_key,
            "__construct"
                | "eof"
                | "fgetc"
                | "fflush"
                | "fgets"
                | "fread"
                | "fwrite"
                | "fstat"
                | "ftell"
                | "fseek"
                | "ftruncate"
                | "rewind"
                | "__elephcspilltofile"
        ),
        "DirectoryIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "seek"
                | "valid"
                | "isdot"
                | "__tostring"
                | "__elephcrefreshpath"
        ),
        "FilesystemIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "getflags" | "setflags"
        ),
        "GlobIterator" => matches!(method_key, "__construct" | "count" | "setflags"),
        "RecursiveDirectoryIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren"
        ),
        "RecursiveCachingIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "EmptyIterator" => matches!(
            method_key,
            "current" | "key" | "next" | "rewind" | "valid"
        ),
        "ArrayIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "ArrayObject" => matches!(
            method_key,
            "__construct"
                | "getiterator"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "SplFixedArray" => matches!(
            method_key,
            "__construct"
                | "__wakeup"
                | "__serialize"
                | "__unserialize"
                | "count"
                | "getiterator"
                | "toarray"
                | "getsize"
                | "setsize"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "jsonserialize"
        ),
        "InternalIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "next" | "rewind" | "valid"
        ),
        "SplDoublyLinkedList" | "SplStack" | "SplQueue" => matches!(
            method_key,
            "add"
                | "pop"
                | "shift"
                | "push"
                | "unshift"
                | "top"
                | "bottom"
                | "count"
                | "isempty"
                | "setiteratormode"
                | "getiteratormode"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "rewind"
                | "current"
                | "key"
                | "prev"
                | "next"
                | "valid"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "enqueue"
                | "dequeue"
        ),
        "SplHeap" => matches!(
            method_key,
            "__construct"
                | "insert"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "compare"
                | "__elephcbestindex"
                | "__elephcremoveat"
        ),
        "SplMaxHeap" | "SplMinHeap" => matches!(method_key, "compare"),
        "SplPriorityQueue" => matches!(
            method_key,
            "__construct"
                | "compare"
                | "insert"
                | "setextractflags"
                | "getextractflags"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "__elephcbestindex"
                | "__elephcoutputat"
                | "__elephcremoveat"
        ),
        "SplObjectStorage" => matches!(
            method_key,
            "__construct"
                | "attach"
                | "detach"
                | "contains"
                | "addall"
                | "removeall"
                | "removeallexcept"
                | "getinfo"
                | "setinfo"
                | "count"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
                | "seek"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "gethash"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "__elephcindexof"
        ),
        "Phar" | "PharData" => matches!(
            method_key,
            "__construct"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "addfromstring"
                | "__tostring"
                | "getpath"
                | "getpathname"
                | "getfilename"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "setstub"
                | "getstub"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
                | "count"
                | "compressfiles"
                | "decompressfiles"
                | "compress"
                | "decompress"
                | "setsignaturealgorithm"
                | "getsignature"
                | "setzippassword"
                | "delete"
        ),
        "PharFileInfo" => matches!(
            method_key,
            "__construct"
                | "getcontent"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
        ),
        "IteratorIterator" => matches!(
            method_key,
            "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "getinneriterator"
        ),
        "LimitIterator" => matches!(
            method_key,
            "__construct" | "rewind" | "next" | "valid" | "seek" | "getposition"
        ),
        "NoRewindIterator" => matches!(method_key, "__construct" | "rewind"),
        "InfiniteIterator" => matches!(method_key, "__construct" | "next"),
        "FilterIterator" => matches!(method_key, "__construct" | "rewind" | "next"),
        "CallbackFilterIterator" => matches!(method_key, "accept" | "__elephcsetcallbackenv"),
        "CachingIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "next"
                | "current"
                | "key"
                | "hasnext"
                | "__tostring"
                | "getflags"
                | "setflags"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "offsetexists"
                | "getcache"
                | "count"
                | "__elephccapturecurrent"
        ),
        "AppendIterator" => matches!(
            method_key,
            "__construct"
                | "append"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getinneriterator"
                | "getiteratorindex"
                | "getarrayiterator"
                | "__elephcstoragecount"
                | "__elephcstoragephysicalcount"
                | "__elephcstorageisactive"
                | "__elephcstorageappend"
                | "__elephcstorageoffsetset"
                | "__elephcstorageoffsetexists"
                | "__elephcstorageoffsetget"
                | "__elephcstorageoffsetunset"
                | "__elephcstoragegetarraycopy"
                | "__elephcstoragekey"
                | "__elephcstoragecurrent"
        ),
        "MultipleIterator" => matches!(
            method_key,
            "__construct"
                | "getflags"
                | "setflags"
                | "attachiterator"
                | "detachiterator"
                | "containsiterator"
                | "countiterators"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
        ),
        "RegexIterator" | "RecursiveRegexIterator" => matches!(
            method_key,
            "__construct"
                | "accept"
                | "current"
                | "key"
                | "getmode"
                | "setmode"
                | "getflags"
                | "setflags"
                | "getregex"
                | "getpregflags"
                | "setpregflags"
                | "__elephcregextarget"
                | "__elephcfirstmatch"
                | "__elephcallmatches"
                | "__elephcsplit"
                | "haschildren"
                | "getchildren"
                | "__elephcassumerecursiveiterator"
        ),
        "RecursiveArrayIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveCallbackFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "ParentIterator" => matches!(
            method_key,
            "__construct" | "accept" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveIteratorIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getdepth"
                | "getinneriterator"
                | "getsubiterator"
                | "__elephcadvance"
                | "__elephcslotfordepth"
                | "__elephcassumerecursiveiterator"
        ),
        "__ElephcAppendIteratorArrayIterator" => matches!(
            method_key,
            "__construct"
                | "count"
                | "append"
                | "offsetset"
                | "offsetexists"
                | "offsetget"
                | "offsetunset"
                | "getarraycopy"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
        ),
        _ => false,
    }
}

/// Returns true when this SPL method is implemented by an intrinsic runtime wrapper.
fn runtime_intrinsic_method_has_wrapper(class_name: &str, method_key: &str, is_static: bool) -> bool {
    let intrinsic = if is_static {
        IntrinsicCall::static_method(class_name, method_key)
    } else {
        IntrinsicCall::instance_method(class_name, method_key)
    };
    intrinsic.is_some_and(|intrinsic| intrinsic.runtime_helper().is_some())
}

/// Lowers one supported builtin SPL method body if it has not already been emitted.
fn lower_builtin_spl_method(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    if class_method_already_lowered(module, class_name, method_key, false)
        || !is_supported_builtin_spl_method(class_name, method_key)
        || runtime_intrinsic_method_has_wrapper(class_name, method_key, false)
    {
        return;
    }
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let Some(method) = class_info
        .method_decls
        .iter()
        .find(|method| php_method_key(&method.name) == method_key && method.has_body)
    else {
        return;
    };
    function::lower_class_method(
        class_name,
        &method.name,
        method.is_static,
        &method.params,
        method.return_type.as_ref(),
        &method.body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Returns true when `module.class_methods` already contains a class-method body.
fn class_method_already_lowered(
    module: &Module,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_method_key(candidate_method) == method_key
                })
    })
}
