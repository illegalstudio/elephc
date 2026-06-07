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

use crate::codegen::platform::Target;
use crate::ir::{
    validate_module, ExternDecl, ExternParamDecl, Immediate, IrType, Module, Op,
};
use crate::ir_lower::{function, LoweringError};
use crate::parser::ast::{ClassMethod, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, PhpType};

/// Lowers an optimized typed AST program into a validated EIR module.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
) -> Result<Module, LoweringError> {
    let mut module = Module::new(target);
    let constants = crate::codegen::collect_constants(program, target.platform);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(program, &mut module, check_result, &constants);
    lower_class_like_methods(program, &mut module, check_result, &constants);
    lower_builtin_reflection_methods(&mut module, check_result, &constants);
    function::lower_main(program, &mut module, check_result, &constants);
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants);
    validate_module(&module)?;
    Ok(module)
}

/// Copies declaration metadata into the EIR module placeholder tables.
fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.trait_table.names = collect_declared_trait_names(program);
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

/// Returns deterministic sorted keys for metadata placeholder tables.
fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
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
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic: _,
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
            ),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations(body, module, check_result, constants);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants);
                for (_, body) in elseif_clauses {
                    lower_function_declarations(body, module, check_result, constants);
                }
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations(then_body, module, check_result, constants);
                if let Some(body) = else_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations(body, module, check_result, constants);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations(body, module, check_result, constants);
                }
                if let Some(body) = default {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations(try_body, module, check_result, constants);
                for catch in catches {
                    lower_function_declarations(&catch.body, module, check_result, constants);
                }
                if let Some(body) = finally_body {
                    lower_function_declarations(body, module, check_result, constants);
                }
            }
            _ => {}
        }
    }
}

/// Lowers methods declared on classes, interfaces, and traits when a body exists.
fn lower_class_like_methods(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } | StmtKind::TraitDecl { name, methods, .. } => {
                lower_methods_for_class_like(name, methods, module, check_result, constants);
            }
            StmtKind::InterfaceDecl { name, methods, .. } => {
                lower_methods_for_class_like(name, methods, module, check_result, constants);
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants);
                for (_, body) in elseif_clauses {
                    lower_class_like_methods(body, module, check_result, constants);
                }
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_class_like_methods(then_body, module, check_result, constants);
                if let Some(body) = else_body {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_class_like_methods(body, module, check_result, constants);
                }
                if let Some(body) = default {
                    lower_class_like_methods(body, module, check_result, constants);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_class_like_methods(try_body, module, check_result, constants);
                for catch in catches {
                    lower_class_like_methods(&catch.body, module, check_result, constants);
                }
                if let Some(body) = finally_body {
                    lower_class_like_methods(body, module, check_result, constants);
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
        );
    }
}

/// Lowers the synthetic reflection methods injected by the checker.
fn lower_builtin_reflection_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    for class_name in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
    ] {
        lower_builtin_reflection_class_methods(class_name, module, check_result, constants);
    }
}

/// Lowers all concrete synthetic methods for one builtin reflection class.
fn lower_builtin_reflection_class_methods(
    class_name: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    for method in &class_info.method_decls {
        if !method.has_body {
            continue;
        }
        let generated_body;
        let body = if class_name == "ReflectionAttribute"
            && crate::names::php_symbol_key(&method.name) == "newinstance"
        {
            generated_body =
                crate::codegen::reflection::build_attribute_new_instance_body(&check_result.classes);
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
        );
    }
}

/// Lowers the small builtin SPL method slice currently consumed by the EIR backend.
fn lower_referenced_builtin_spl_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    loop {
        let mut methods = referenced_builtin_spl_methods(module);
        methods.sort();
        methods.dedup();
        methods.retain(|(class_name, method_key)| {
            !class_method_already_lowered(module, class_name, method_key, false)
        });
        if methods.is_empty() {
            break;
        }

        let before = module.class_methods.len();
        for (class_name, method_key) in methods {
            lower_builtin_spl_method(&class_name, &method_key, module, check_result, constants);
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
                    let PhpType::Object(class_name) = receiver_ty else {
                        continue;
                    };
                    let normalized = class_name.trim_start_matches('\\');
                    let Some(method_name) = string_data_name(module, inst) else {
                        continue;
                    };
                    let method_key = php_method_key(method_name);
                    push_supported_builtin_spl_method_for_receiver(
                        &mut methods,
                        module,
                        normalized,
                        &method_key,
                    );
                }
                _ => {}
            }
        }
    }
    methods
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
        "AppendIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
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
        "CallbackFilterIterator" => matches!(method_key, "accept"),
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

/// Lowers one supported builtin SPL method body if it has not already been emitted.
fn lower_builtin_spl_method(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
) {
    if class_method_already_lowered(module, class_name, method_key, false)
        || !is_supported_builtin_spl_method(class_name, method_key)
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
