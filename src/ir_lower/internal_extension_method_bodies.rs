//! Purpose:
//! Demand-driven EIR lowering for internal-extension class methods that carry
//! synthetic PHP bodies. Lets the compiler override locked native-bridge opcodes
//! for specific methods without changing the locked operation manifest.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` before the builtin SPL method pass.
//!
//! Key details:
//! - Scans already-lowered functions and methods for `MethodCall` / `NullsafeMethodCall`
//!   receivers that are native-wrapper classes.
//! - Lowers any referenced method whose flattened declaration has a non-empty synthetic body.
//! - Iterates to a fixpoint so transitive synthetic-body references are emitted.

use std::collections::HashMap;

use crate::ir::{Function, Instruction, Module, Op};
use crate::ir_lower::function;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

/// Lowers every referenced internal-extension method body that has a synthetic PHP body.
pub(crate) fn lower_referenced_internal_extension_method_bodies(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    lower_interface_runtime_entries(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    loop {
        let mut methods = Vec::new();
        for function in all_functions(module) {
            for instruction in &function.instructions {
                if !matches!(instruction.op, Op::MethodCall | Op::NullsafeMethodCall) {
                    continue;
                }
                let Some(receiver) = instruction.operands.first().copied() else {
                    continue;
                };
                let Some(receiver_type) = function.value(receiver).map(|value| value.php_type.clone()) else {
                    continue;
                };
                let class_name = match receiver_type {
                    PhpType::Object(class_name) => class_name,
                    PhpType::Union(members) => {
                        let mut candidate: Option<String> = None;
                        for member in members {
                            match member {
                                PhpType::Void
                                | PhpType::False
                                | PhpType::Bool
                                | PhpType::Int
                                | PhpType::Float
                                | PhpType::Str => {}
                                PhpType::Object(class_name)
                                    if crate::internal_extensions::is_native_wrapper_class(
                                        &class_name,
                                    ) =>
                                {
                                    if candidate
                                        .as_ref()
                                        .is_some_and(|existing| existing != &class_name)
                                    {
                                        candidate = None;
                                        break;
                                    }
                                    candidate = Some(class_name);
                                }
                                _ => {
                                    candidate = None;
                                    break;
                                }
                            }
                        }
                        let Some(class_name) = candidate else {
                            continue;
                        };
                        class_name
                    }
                    _ => continue,
                };
                let Some(method_name) = string_data_name(module, instruction) else {
                    continue;
                };
                let method_key = php_symbol_key(method_name);
                let Some(class_info) = check_result.classes.get(&class_name) else {
                    continue;
                };
                if !class_info.method_decls.iter().any(|method| {
                    php_symbol_key(&method.name) == method_key
                    && method.has_body
                    && !method.body.is_empty()
                }) {
                    continue;
                }
                methods.push((class_name, method_key));
            }
        }

        methods.sort();
        methods.dedup();
        methods.retain(|(class_name, method_key)| {
            !super::program::class_method_already_lowered(
                module, class_name, method_key, false,
            )
        });
        if methods.is_empty() {
            break;
        }

        for (class_name, method_key) in methods {
            lower_internal_extension_method_body(
                &class_name,
                &method_key,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
    }

    lower_simplexml_debug_info_runtime_entry(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    lower_dom_namespace_debug_info_runtime_entry(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    lower_legacy_dom_element_debug_info_runtime_entry(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    lower_dom_collection_debug_info_runtime_entries(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    lower_simplexml_iterator_runtime_entries(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    lower_simplexml_scalar_runtime_entries(
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers php-src's ordered virtual-property projection for legacy `DOMElement`.
fn lower_legacy_dom_element_debug_info_runtime_entry(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    lower_dom_virtual_debug_info_runtime_entry(
        "DOMElement",
        &[
            "tagName",
            "className",
            "id",
            "schemaTypeInfo",
            "firstElementChild",
            "lastElementChild",
            "childElementCount",
            "previousElementSibling",
            "nextElementSibling",
            "nodeName",
            "nodeValue",
            "nodeType",
            "parentNode",
            "parentElement",
            "childNodes",
            "firstChild",
            "lastChild",
            "previousSibling",
            "nextSibling",
            "attributes",
            "isConnected",
            "ownerDocument",
            "namespaceURI",
            "prefix",
            "localName",
            "baseURI",
            "textContent",
        ],
        &[
            "tagName",
            "className",
            "id",
            "nodeName",
            "prefix",
            "localName",
            "baseURI",
            "textContent",
        ],
        &[
            "firstElementChild",
            "lastElementChild",
            "previousElementSibling",
            "nextElementSibling",
            "parentNode",
            "parentElement",
            "childNodes",
            "firstChild",
            "lastChild",
            "previousSibling",
            "nextSibling",
            "attributes",
            "ownerDocument",
        ],
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers php-src's native `DOMNameSpaceNode` debug-property projection.
///
/// DOM exposes these values through an object handler rather than a user-callable
/// `__debugInfo()` method. The runtime object walker still needs a normal method ABI
/// target, so this compiler-only body reads the seven scalar virtual properties and
/// substitutes php-src's recursion-safe marker for the three object-valued properties.
fn lower_dom_namespace_debug_info_runtime_entry(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    lower_dom_virtual_debug_info_runtime_entry(
        "DOMNameSpaceNode",
        &[
            "nodeName",
            "nodeValue",
            "nodeType",
            "prefix",
            "localName",
            "namespaceURI",
            "isConnected",
        ],
        &[
            "nodeName",
            "nodeValue",
            "prefix",
            "localName",
            "namespaceURI",
        ],
        &["ownerDocument", "parentNode", "parentElement"],
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers php-src's native virtual-property projections for every DOM collection wrapper.
fn lower_dom_collection_debug_info_runtime_entries(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const LENGTH: &[&str] = &["length"];
    const LENGTH_AND_VALUE: &[&str] = &["length", "value"];
    const COLLECTIONS: &[(&str, &[&str], &[&str])] = &[
        ("DOMNodeList", LENGTH, &[]),
        ("DOMNamedNodeMap", LENGTH, &[]),
        ("Dom\\NodeList", LENGTH, &[]),
        ("Dom\\NamedNodeMap", LENGTH, &[]),
        ("Dom\\DtdNamedNodeMap", LENGTH, &[]),
        ("Dom\\HTMLCollection", LENGTH, &[]),
        ("Dom\\TokenList", LENGTH_AND_VALUE, &["value"]),
    ];

    for (class_name, properties, string_properties) in COLLECTIONS {
        lower_dom_virtual_debug_info_runtime_entry(
            class_name,
            properties,
            string_properties,
            &[],
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Lowers one hidden DOM object-handler projection through virtual property reads.
#[allow(clippy::too_many_arguments)]
fn lower_dom_virtual_debug_info_runtime_entry(
    class_name: &str,
    properties: &[&str],
    string_properties: &[&str],
    object_properties: &[&str],
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const METHOD_KEY: &str = "__debuginfo";

    if !module.required_runtime_features.dom_bridge
        || super::program::class_method_already_lowered(
            module,
            class_name,
            METHOD_KEY,
            false,
        )
        || !check_result.classes.contains_key(class_name)
    {
        return;
    }

    let span = Span::dummy();
    let property = |name: &str| {
        Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::This, span)),
                property: name.to_string(),
            },
            span,
        )
    };
    let string = |value: &str| Expr::new(ExprKind::StringLiteral(value.to_string()), span);
    // php-src reads one virtual property at a time. When the handler returns an
    // object, it destroys that temporary before inserting the fixed marker in
    // the debug hashtable (`dom_get_debug_info_helper`). Retaining every wrapper
    // until the function epilogue changes observable object-handle reuse.
    let result_name = "__dom_debug_result".to_string();
    let variable = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let release_local = |name: &str| {
        Stmt::new(
            StmtKind::Assign {
                name: name.to_string(),
                value: Expr::new(ExprKind::Null, span),
            },
            span,
        )
    };
    let assign_entry = |key: Expr, value: Expr| {
        Stmt::new(
            StmtKind::ArrayAssign {
                array: result_name.clone(),
                index: key,
                value,
            },
            span,
        )
    };
    let mut body = vec![Stmt::new(
        StmtKind::Assign {
            name: result_name.clone(),
            value: Expr::new(ExprKind::ArrayLiteralAssoc(Vec::new()), span),
        },
        span,
    )];
    for name in properties {
        let key = string(name);
        if object_properties.contains(name) {
            let local_name = format!("__dom_debug_{name}");
            let object = variable(&local_name);
            body.push(Stmt::new(
                StmtKind::Assign {
                    name: local_name.clone(),
                    value: property(name),
                },
                span,
            ));
            body.push(Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::unqualified("is_object"),
                            args: vec![object.clone()],
                        },
                        span,
                    ),
                    then_body: vec![
                        release_local(&local_name),
                        assign_entry(key.clone(), string("(object value omitted)")),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![
                        assign_entry(key, object),
                        release_local(&local_name),
                    ]),
                },
                span,
            ));
        } else if string_properties.contains(name) {
            let local_name = format!("__dom_debug_{name}");
            body.push(Stmt::new(
                StmtKind::Assign {
                    name: local_name.clone(),
                    value: property(name),
                },
                span,
            ));
            body.push(assign_entry(key, variable(&local_name)));
            body.push(release_local(&local_name));
        } else {
            body.push(assign_entry(key, property(name)));
        }
    }
    body.push(Stmt::new(
        StmtKind::Return(Some(variable(&result_name))),
        span,
    ));
    let signature = FunctionSig {
        params: Vec::new(),
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        declared_return: false,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    };
    module
        .class_infos
        .get_mut(class_name)
        .expect("checked DOM projection class must exist in the EIR schema")
        .methods
        .insert(METHOD_KEY.to_string(), signature);
    function::lower_class_method(
        class_name,
        "__debugInfo",
        false,
        &Vec::new(),
        None,
        &body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
    let class_info = module
        .class_infos
        .get_mut(class_name)
        .expect("checked DOM projection class must remain in the EIR schema");
    class_info.methods.remove(METHOD_KEY);
    class_info
        .method_impl_classes
        .insert(METHOD_KEY.to_string(), class_name.to_string());
}

/// Lowers synthetic native-wrapper bodies reachable only through interface tables.
///
/// Operations such as `foreach` do not leave an ordinary `MethodCall` in EIR when they
/// invoke `IteratorAggregate::getIterator()`. The backend nevertheless emits the class's
/// interface table, so every synthetic implementation referenced by that table must have
/// a real method symbol before runtime metadata is trimmed to emitted methods.
fn lower_interface_runtime_entries(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    if !module.required_runtime_features.dom_bridge {
        return;
    }

    let mut methods = Vec::new();
    for class_info in module.class_infos.values() {
        for interface_name in &class_info.interfaces {
            let Some(interface_info) = module.interface_infos.get(interface_name) else {
                continue;
            };
            for method_key in &interface_info.method_order {
                let Some(impl_class) = class_info.method_impl_classes.get(method_key) else {
                    continue;
                };
                if !crate::internal_extensions::is_native_wrapper_class(impl_class) {
                    continue;
                }
                let has_synthetic_body = check_result
                    .classes
                    .get(impl_class)
                    .is_some_and(|impl_info| {
                        impl_info.method_decls.iter().any(|method| {
                            php_symbol_key(&method.name) == *method_key
                                && method.has_body
                                && !method.body.is_empty()
                        })
                    });
                if has_synthetic_body {
                    methods.push((impl_class.clone(), method_key.clone()));
                }
            }
        }
    }

    methods.sort();
    methods.dedup();
    for (class_name, method_key) in methods {
        if !super::program::class_method_already_lowered(
            module,
            &class_name,
            &method_key,
            false,
        ) {
            lower_internal_extension_method_body(
                &class_name,
                &method_key,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
    }
}

/// Lowers the native SimpleXML debug method as a real runtime-callable method symbol.
///
/// Ordinary source calls to bodyless internal-extension methods lower straight to
/// `InternalExtensionCall`, so no `_method_*` symbol normally exists. Recursive
/// `var_dump()` and `print_r()` walkers cannot carry an EIR call site, however:
/// they resolve `__debugInfo()` from the concrete object's class id at runtime.
/// This synthetic body deliberately calls the still-bodyless declaration, which
/// lowers to opcode 4426 and gives the runtime table a normal method ABI entry.
fn lower_simplexml_debug_info_runtime_entry(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const CLASS_NAME: &str = "SimpleXMLElement";
    const METHOD_KEY: &str = "__debuginfo";

    if !module.required_runtime_features.dom_bridge
        || super::program::class_method_already_lowered(
            module,
            CLASS_NAME,
            METHOD_KEY,
            false,
        )
    {
        return;
    }
    let Some(method) = check_result
        .classes
        .get(CLASS_NAME)
        .and_then(|class_info| {
            class_info
                .method_decls
                .iter()
                .find(|method| php_symbol_key(&method.name) == METHOD_KEY && !method.is_static)
        })
    else {
        return;
    };

    let span = Span::dummy();
    let call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::This, span)),
            method: method.name.clone(),
            args: Vec::new(),
        },
        span,
    );
    let body = [Stmt::new(StmtKind::Return(Some(call)), span)];
    function::lower_class_method(
        CLASS_NAME,
        &method.name,
        false,
        &method.params,
        method.return_type.as_ref(),
        &body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers bodyless SimpleXML Iterator protocols into callable ABI entries and wires descendants.
///
/// Direct source calls already lower to locked internal-extension opcodes. Dynamic `foreach`,
/// however, calls through the `Iterator` implementation table and therefore needs real method
/// symbols. Recursive SPL iterators likewise dispatch `hasChildren()` and `getChildren()` through
/// `RecursiveIterator`. The synthetic bodies preserve the direct native lowering while the
/// metadata wiring makes every non-overriding `SimpleXMLElement` descendant reuse those symbols.
fn lower_simplexml_iterator_runtime_entries(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const METHOD_KEYS: &[&str] = &[
        "rewind",
        "valid",
        "current",
        "key",
        "next",
        "haschildren",
        "getchildren",
    ];

    if !module.required_runtime_features.dom_bridge {
        return;
    }
    for method_key in METHOD_KEYS {
        lower_simplexml_native_runtime_entry(
            method_key,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
    wire_simplexml_method_impl_classes(module, METHOD_KEYS);
}

/// Lowers bodyless SimpleXML scalar methods that need real callable ABI entries.
///
/// Iterator `current()` returns a boxed object through the interface ABI. String consumers such
/// as `trim($value)` then resolve `__toString()` through the class vtable, so the locked native
/// operation needs the same real method symbol and descendant metadata as the Iterator methods.
/// Userland overrides that call `parent::count()` likewise reference the base method symbol rather
/// than issuing a direct bridge call, so `count()` must be materialized by the same mechanism.
fn lower_simplexml_scalar_runtime_entries(
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const METHOD_KEYS: &[&str] = &["__tostring", "count"];

    if !module.required_runtime_features.dom_bridge {
        return;
    }
    for method_key in METHOD_KEYS {
        lower_simplexml_native_runtime_entry(
            method_key,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
    wire_simplexml_method_impl_classes(module, METHOD_KEYS);
}

/// Wires native SimpleXML method symbols into the base class and non-overriding descendants.
fn wire_simplexml_method_impl_classes(module: &mut Module, method_keys: &[&str]) {
    const CLASS_NAME: &str = "SimpleXMLElement";

    let descendant_names = module
        .class_infos
        .keys()
        .filter(|class_name| {
            class_is_same_or_descendant(&module.class_infos, class_name, CLASS_NAME)
        })
        .cloned()
        .collect::<Vec<_>>();
    for class_name in descendant_names {
        let Some(class_info) = module.class_infos.get_mut(&class_name) else {
            continue;
        };
        for method_key in method_keys {
            if class_info.methods.contains_key(*method_key) {
                class_info
                    .method_impl_classes
                    .entry((*method_key).to_string())
                    .or_insert_with(|| CLASS_NAME.to_string());
            }
        }
    }
}

/// Lowers one bodyless SimpleXML method through its existing native operation.
fn lower_simplexml_native_runtime_entry(
    method_key: &str,
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    const CLASS_NAME: &str = "SimpleXMLElement";

    if super::program::class_method_already_lowered(
        module,
        CLASS_NAME,
        method_key,
        false,
    ) {
        return;
    }
    let Some(method) = check_result
        .classes
        .get(CLASS_NAME)
        .and_then(|class_info| {
            class_info.method_decls.iter().find(|method| {
                php_symbol_key(&method.name) == method_key && !method.is_static
            })
        })
    else {
        return;
    };

    let span = Span::dummy();
    let call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::This, span)),
            method: method.name.clone(),
            args: Vec::new(),
        },
        span,
    );
    let body = if matches!(method_key, "rewind" | "next") {
        vec![Stmt::new(StmtKind::ExprStmt(call), span)]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), span)]
    };
    function::lower_class_method(
        CLASS_NAME,
        &method.name,
        false,
        &method.params,
        method.return_type.as_ref(),
        &body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Returns whether one class is the requested base or inherits from it.
fn class_is_same_or_descendant(
    classes: &HashMap<String, crate::types::ClassInfo>,
    candidate: &str,
    base: &str,
) -> bool {
    let mut current = Some(candidate);
    for _ in 0..=classes.len() {
        let Some(class_name) = current else {
            return false;
        };
        if class_name.eq_ignore_ascii_case(base) {
            return true;
        }
        current = classes
            .get(class_name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Lowers one synthetic internal-extension method body into the module.
fn lower_internal_extension_method_body(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &crate::types::CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let Some(method) = class_info
        .method_decls
        .iter()
        .find(|method| {
            php_symbol_key(&method.name) == method_key
                && method.has_body
                && !method.body.is_empty()
        })
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

/// Returns every function-like body currently present in the module.
fn all_functions(module: &Module) -> impl Iterator<Item = &Function> {
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

/// Returns the string immediate attached to an instruction, if any.
fn string_data_name<'a>(
    module: &'a Module,
    instruction: &Instruction,
) -> Option<&'a str> {
    let Some(crate::ir::Immediate::Data(data)) = instruction.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}
