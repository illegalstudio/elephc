//! Purpose:
//! Shares compile-time reflection metadata helpers across class-method and
//! expression codegen.
//!
//! Called from:
//! - `crate::codegen::class_methods`
//! - `crate::codegen::builtins::system::class_get_attributes`
//! - `crate::codegen::expr::objects::reflection`
//!
//! Key details:
//! - Attribute factory ids are deterministic over the full class metadata
//!   table so `ReflectionAttribute::newInstance()` and metadata materializers
//!   agree without runtime registration state.

use std::collections::{BTreeMap, HashMap};

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::expr::objects::emit_new_object;
use crate::codegen::platform::Arch;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{AttrArgEntry, AttrArgValue, AttrKey, ClassInfo, PhpType};

#[derive(Clone)]
/// Factory record for compile-time reflection attribute metadata.
/// `id` is assigned sequentially and must match across all compilation units
/// so `ReflectionAttribute::newInstance()` and codegen agree on the factory index.
pub(crate) struct ReflectionAttributeFactory {
    pub(crate) id: i64,
    pub(crate) class_name: String,
    pub(crate) args: Vec<AttrArgEntry>,
    /// True when `class_name` resolves to a real class. `newInstance()` only
    /// emits a construction branch for resolvable factories; `getArguments()`
    /// uses every factory (including non-class attributes) to return arguments.
    pub(crate) resolvable: bool,
}

/// Looks up `class_name` in `classes` using PHPsymbol-key normalization
/// (leading-backslash stripping and case-insensitive comparison).
/// Returns the canonical class name string from the HashMap key, or `None`
/// if the class is not registered.
pub(crate) fn resolve_class_name<'a>(
    classes: &'a HashMap<String, ClassInfo>,
    class_name: &str,
) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Scans every class in `classes` and collects all distinct class-level,
/// method-level, and property-level attribute name/argument pairs into a
/// sorted vector of `ReflectionAttributeFactory` records with sequential ids.
pub(crate) fn collect_attribute_factories(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<ReflectionAttributeFactory> {
    let mut unique = BTreeMap::new();
    for class_info in classes.values() {
        collect_from_attribute_lists(
            classes,
            &class_info.attribute_names,
            &class_info.attribute_args,
            &mut unique,
        );
        for (member, names) in &class_info.method_attribute_names {
            if let Some(args) = class_info.method_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
        for (member, names) in &class_info.property_attribute_names {
            if let Some(args) = class_info.property_attribute_args.get(member) {
                collect_from_attribute_lists(classes, names, args, &mut unique);
            }
        }
    }

    unique
        .into_iter()
        .enumerate()
        .map(|(idx, ((class_name, args), resolvable))| ReflectionAttributeFactory {
            id: (idx as i64) + 1,
            class_name,
            args,
            resolvable,
        })
        .collect()
}

/// Returns the factory id for the given attribute `attr_name` with
/// `attr_args`. Returns 0 if the class cannot be resolved or no matching
/// factory exists.
pub(crate) fn attribute_factory_id(
    classes: &HashMap<String, ClassInfo>,
    attr_name: &str,
    attr_args: &[AttrArgEntry],
) -> i64 {
    // Non-class attributes are registered under their raw name (see
    // `collect_from_attribute_lists`), so fall back to it when the name does
    // not resolve to a real class.
    let lookup_name = resolve_class_name(classes, attr_name)
        .map(|resolved| resolved.to_string())
        .unwrap_or_else(|| attr_name.to_string());
    collect_attribute_factories(classes)
        .into_iter()
        .find(|factory| factory.class_name == lookup_name && factory.args == attr_args)
        .map(|factory| factory.id)
        .unwrap_or(0)
}

/// Builds the synthetic dispatch body for `ReflectionAttribute::newInstance()`.
pub(crate) fn build_attribute_new_instance_body(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<Stmt> {
    let span = crate::span::Span::dummy();
    let factories = collect_attribute_factories(classes);
    let mut body = Vec::new();
    for factory in factories {
        // Only resolvable attribute classes can be instantiated. Non-class
        // attributes are registered (so `getArguments()` can find them) but
        // have no construction branch here; they fall through to `return null`.
        if !factory.resolvable {
            continue;
        }
        let condition = factory_condition(factory.id);
        let then_body = vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::NewObject {
                    class_name: name_from_canonical(&factory.class_name),
                    args: factory.args.iter().map(|entry| attr_arg_expr(&entry.value)).collect(),
                },
                span,
            ))),
            span,
        )];
        body.push(Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    body.push(Stmt::new(
        StmtKind::Return(Some(Expr::new(ExprKind::Null, span))),
        span,
    ));
    body
}

/// Creates `this->__factory === factory_id` for `newInstance()` dispatch routing.
fn factory_condition(factory_id: i64) -> Expr {
    let span = crate::span::Span::dummy();
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, span)),
                    property: "__factory".to_string(),
                },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(factory_id), span)),
        },
        span,
    )
}

/// Converts one captured attribute argument into a synthetic AST expression.
/// Nested arrays (positional or associative) become the corresponding
/// array-literal expression, and symbolic references (global constant, class
/// constant, enum case) become the corresponding reference expression — all so
/// they lower through the normal, architecture-independent paths. Reference
/// names were canonicalised by name resolution at capture time, so the
/// re-emitted nodes resolve directly during lowering (the global/class constant
/// folds to its value; an enum case materializes the case object, matching
/// PHP's `ReflectionAttribute::getArguments()`).
fn attr_arg_expr(arg: &AttrArgValue) -> Expr {
    let span = crate::span::Span::dummy();
    match arg {
        AttrArgValue::Null => Expr::new(ExprKind::Null, span),
        AttrArgValue::Int(value) => Expr::new(ExprKind::IntLiteral(*value), span),
        AttrArgValue::Float(bits) => Expr::new(ExprKind::FloatLiteral(f64::from_bits(*bits)), span),
        AttrArgValue::Bool(value) => Expr::new(ExprKind::BoolLiteral(*value), span),
        AttrArgValue::Str(value) => Expr::new(ExprKind::StringLiteral(value.clone()), span),
        AttrArgValue::Array(entries) => entries_to_array_expr(entries, false),
        AttrArgValue::ConstRef(name) => {
            Expr::new(ExprKind::ConstRef(name_from_canonical(name)), span)
        }
        AttrArgValue::ScopedConst(type_name, member) => Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver: StaticReceiver::Named(name_from_canonical(type_name)),
                name: member.clone(),
            },
            span,
        ),
    }
}

/// Builds an array-literal AST expression from captured attribute-arg entries.
/// When `force_assoc` is set (or any entry carries a key — a named argument or
/// explicit array key) it produces an associative `ArrayLiteralAssoc` with
/// positional entries taking their sequential integer key, matching PHP's
/// `getArguments()` ordering; otherwise it produces a positional `ArrayLiteral`.
/// `force_assoc` keeps the top-level `getArguments()` result a single array kind
/// (a hash) so its declared associative type matches the runtime value.
fn entries_to_array_expr(entries: &[AttrArgEntry], force_assoc: bool) -> Expr {
    let span = crate::span::Span::dummy();
    if force_assoc || entries.iter().any(|entry| entry.key.is_some()) {
        let mut next_index = 0i64;
        let pairs = entries
            .iter()
            .map(|entry| {
                let key = match &entry.key {
                    Some(key) => attr_key_expr(key),
                    None => {
                        let index = next_index;
                        next_index += 1;
                        Expr::new(ExprKind::IntLiteral(index), span)
                    }
                };
                (key, attr_arg_expr(&entry.value))
            })
            .collect();
        Expr::new(ExprKind::ArrayLiteralAssoc(pairs), span)
    } else {
        Expr::new(
            ExprKind::ArrayLiteral(entries.iter().map(|entry| attr_arg_expr(&entry.value)).collect()),
            span,
        )
    }
}

/// Converts a captured attribute array/named key into a synthetic AST key
/// expression.
fn attr_key_expr(key: &AttrKey) -> Expr {
    let span = crate::span::Span::dummy();
    let kind = match key {
        AttrKey::Int(value) => ExprKind::IntLiteral(*value),
        AttrKey::Str(value) => ExprKind::StringLiteral(value.clone()),
    };
    Expr::new(kind, span)
}

/// Builds the synthetic body for `ReflectionAttribute::getArguments()`. For
/// each attribute whose class resolves, it dispatches on the factory id and
/// returns the captured arguments as a lowered array literal — so named
/// arguments and associative arrays are materialized through the normal array
/// path. Attributes without a resolvable class fall back to the `$__args`
/// property populated at construction.
pub(crate) fn build_attribute_get_arguments_body(
    classes: &HashMap<String, ClassInfo>,
) -> Vec<Stmt> {
    let span = crate::span::Span::dummy();
    let factories = collect_attribute_factories(classes);
    let mut body = Vec::new();
    for factory in factories {
        let condition = factory_condition(factory.id);
        let then_body = vec![Stmt::new(
            StmtKind::Return(Some(entries_to_array_expr(&factory.args, true))),
            span,
        )];
        body.push(Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    // Every attribute with supported arguments is registered as a factory
    // above (class or not), so this is only a defensive default; return an
    // empty associative array to match the declared return type.
    body.push(Stmt::new(
        StmtKind::Return(Some(entries_to_array_expr(&[], true))),
        span,
    ));
    body
}

/// Converts a canonical class string into the `Name` shape expected by `NewObject`.
fn name_from_canonical(class_name: &str) -> Name {
    Name::qualified(class_name.split('\\').map(str::to_string).collect())
}

/// Allocates and populates a PHP indexed array of `ReflectionAttribute`
/// objects for the given attribute names and argument lists. Each attribute
/// is constructed by allocating a `ReflectionAttribute` via `emit_new_object`,
/// then overwriting its `$__name`, `$__args`, and `$__factory` properties.
/// Returns the emitted array type stamp (`array<ReflectionAttribute>`).
pub(crate) fn emit_reflection_attribute_array(
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgEntry>>],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let result_reg = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);

    // -- allocate the result indexed array (one heap-pointer slot per attr) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_names.len().max(1))); // initial capacity (>=1 to avoid grow on first push)
            emitter.instruction("mov x1, #8");                                  // element stride: one heap pointer per slot (object handle)
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated array pointer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", attr_names.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov rsi, 8");                                  // element stride: one heap pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = array pointer
        }
    }
    emit_array_value_type_stamp(
        emitter,
        result_reg,
        &PhpType::Object("ReflectionAttribute".to_string()),
    );

    for (idx, attr_name) in attr_names.iter().enumerate() {
        let empty_args = Vec::new();
        let attr_arg_list = attr_args
            .get(idx)
            .and_then(Option::as_ref)
            .unwrap_or(&empty_args);
        let factory_id = attribute_factory_id(&ctx.classes, attr_name, attr_arg_list);

        // -- save the result array pointer below later temporaries --
        abi::emit_push_reg(emitter, result_reg);

        // -- allocate a fresh ReflectionAttribute via the normal new path --
        // emit_new_object walks the registered class and runs its private
        // synthetic zero-arg constructor; this internal emitter is the only
        // code path that can populate ReflectionAttribute metadata slots.
        emit_new_object("ReflectionAttribute", &[], emitter, ctx, data);

        // The new object pointer is now in the result reg. Save it below
        // both the array pointer and the spilled per-property scratch
        // values that follow.
        abi::emit_push_reg(emitter, result_reg);

        // -- overwrite `$__name` (offset 8 = lo, 16 = hi) --
        emit_set_string_property(emitter, data, attr_name, scratch, 8, 16);

        // -- build the mixed args array and overwrite `$__args` --
        emit_set_args_property(emitter, data, attr_arg_list, scratch);

        // -- store the newInstance factory id in `$__factory` --
        emit_set_factory_property(emitter, factory_id, scratch);

        // -- push the populated object pointer into the result array --
        // After emit_set_args_property, the spilled object pointer is still
        // on the stack one slot below the result array. Pop both back, push.
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop the populated ReflectionAttribute pointer into the value-arg register
                emitter.instruction("ldr x0, [sp], #16");                       // pop the result array pointer into the array-arg register
                emitter.instruction("bl __rt_array_push_int");                  // append the object handle to the result array
            }
            Arch::X86_64 => {
                abi::emit_pop_reg(emitter, "rsi");                              // pop the populated ReflectionAttribute pointer into the value-arg register
                abi::emit_pop_reg(emitter, "rdi");                              // pop the result array pointer into the array-arg register
                emitter.instruction("call __rt_array_push_int");                // append the object handle to the result array
            }
        }
    }

    PhpType::Array(Box::new(PhpType::Object("ReflectionAttribute".to_string())))
}

/// Overwrites a string property slot on the object at the top of the temporary
/// stack with a heap-persisted copy of `value`.
///
/// Frees the previous low-word string pointer using the safe heap free helper,
/// stores the new pointer at `low_offset`, and stores the byte length at
/// `high_offset`. The object pointer is left on the temporary stack.
pub(crate) fn emit_set_string_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    value: &str,
    obj_ptr_scratch: &str,
    low_offset: usize,
    high_offset: usize,
) {
    let (sym, len) = data.add_string(value.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer from the temporary stack
            emitter.instruction(&format!("ldr x0, [x9, #{}]", low_offset));     // load the old string pointer before overwriting it
            emitter.instruction("bl __rt_heap_free_safe");                      // release the previous owned string
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = source string address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = source string length
            emitter.instruction("bl __rt_str_persist");                         // x1 = heap-resident pointer, x2 = length
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer back
            emitter.instruction(&format!("str x1, [{}, #{}]", obj_ptr_scratch, low_offset)); // commit the string pointer
            emitter.instruction(&format!("str x2, [{}, #{}]", obj_ptr_scratch, high_offset)); // commit the string length
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {}]", low_offset)); // load the old string pointer
            emitter.instruction("call __rt_heap_free_safe");                    // release the previous owned string
            abi::emit_symbol_address(emitter, "rax", &sym);                     // rax = source string address
            emitter.instruction(&format!("mov rdx, {}", len));                  // rdx = source string length
            emitter.instruction("call __rt_str_persist");                       // rax = heap-resident pointer, rdx = length
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer back
            emitter.instruction(&format!("mov QWORD PTR [{} + {}], rax", obj_ptr_scratch, low_offset)); // commit the string pointer
            emitter.instruction(&format!("mov QWORD PTR [{} + {}], rdx", obj_ptr_scratch, high_offset)); // commit the string length
        }
    }
}

/// Overwrites the `$__args` slot with a freshly allocated `array<mixed>`
/// built from `attr_arg_list`. Decrements the refcount of the previously
/// default empty array. The object pointer is expected at the top of the
/// temporary stack and is left there.
fn emit_set_args_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    attr_arg_list: &[AttrArgEntry],
    obj_ptr_scratch: &str,
) {
    let result_reg = abi::int_result_reg(emitter);

    // -- decref the previous default `[]` value before overwriting --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer
            emitter.instruction("ldr x0, [x9, #24]");                           // load old __args.lo (heap array pointer)
            emitter.instruction("bl __rt_decref_array");                        // release the previous default empty array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 24]");               // load old __args.lo
            emitter.instruction("call __rt_decref_array");                      // release the previous default empty array
        }
    }

    // -- allocate a fresh mixed-cell pointer array for the literal args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_arg_list.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov x1, #8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated args array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", attr_arg_list.len().max(1))); // initial capacity (>=1)
            emitter.instruction("mov rsi, 8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = freshly allocated args array
        }
    }
    emit_array_value_type_stamp(emitter, result_reg, &PhpType::Mixed);

    // -- box and push each literal arg --
    for entry in attr_arg_list {
        let arg = &entry.value;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer across the boxing helper call
                emit_box_arg_aarch64(arg, emitter, data);                       // x0 = boxed mixed-cell pointer for this arg
                emitter.instruction("mov x1, x0");                              // x1 = mixed-cell pointer (push helper's value arg)
                emitter.instruction("ldr x0, [sp]");                            // x0 = args array pointer
                emitter.instruction("bl __rt_array_push_int");                  // x0 = (possibly realloc'd) args array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date array pointer
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer
                emit_box_arg_x86_64(arg, emitter, data);                        // rax = boxed mixed-cell pointer
                emitter.instruction("mov rsi, rax");                            // rsi = mixed-cell pointer
                emitter.instruction("mov rdi, QWORD PTR [rsp]");                // rdi = args array pointer
                emitter.instruction("call __rt_array_push_int");                // rax = updated args array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved args-array slot
            }
        }
    }

    // -- store the args array pointer + array kind tag in __args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer
            emitter.instruction(&format!("str {}, [{}, #24]", result_reg, obj_ptr_scratch)); // commit __args.lo (array pointer)
            emitter.instruction("mov x10, #4");                                 // runtime kind tag 4 = indexed array
            emitter.instruction(&format!("str x10, [{}, #32]", obj_ptr_scratch)); // commit __args.hi (kind tag)
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer
            emitter.instruction(&format!("mov QWORD PTR [{} + 24], {}", obj_ptr_scratch, result_reg)); // commit __args.lo (array pointer)
            emitter.instruction(&format!("mov QWORD PTR [{} + 32], 4", obj_ptr_scratch)); // commit __args.hi (kind tag = 4 = indexed array)
        }
    }
}

/// Overwrites the `$__factory` property of the object at the top of the
/// temporary stack with the given `factory_id`. Clears the unused high word
/// of the int property slot to preserve runtime invariants.
fn emit_set_factory_property(
    emitter: &mut Emitter,
    factory_id: i64,
    obj_ptr_scratch: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer
            abi::emit_load_int_immediate(emitter, "x10", factory_id);
            emitter.instruction(&format!("str x10, [{}, #40]", obj_ptr_scratch)); // commit __factory id for newInstance()
            emitter.instruction(&format!("str xzr, [{}, #48]", obj_ptr_scratch)); // clear the unused high word of the int property slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer
            abi::emit_load_int_immediate(emitter, "r10", factory_id);
            emitter.instruction(&format!("mov QWORD PTR [{} + 40], r10", obj_ptr_scratch)); // commit __factory id for newInstance()
            emitter.instruction(&format!("mov QWORD PTR [{} + 48], 0", obj_ptr_scratch)); // clear the unused high word of the int property slot
        }
    }
}

/// Emits a boxed `Mixed` cell for `arg` using ARM64 calling conventions.
/// Loads the runtime tag into `x0`, the low payload into `x1`, and the
/// high payload into `x2`, then calls `__rt_mixed_from_value` to
/// produce an owned boxed cell returned in `x0`.
fn emit_box_arg_aarch64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null payload
            emitter.instruction("mov x1, xzr");                                 // null carries no low word
            emitter.instruction("mov x2, xzr");                                 // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer payload
            abi::emit_load_int_immediate(emitter, "x1", *value);
            emitter.instruction("mov x2, xzr");                                 // ints do not use the high word
        }
        AttrArgValue::Float(bits) => {
            emitter.instruction("mov x0, #2");                                  // runtime tag 2 = float payload
            abi::emit_load_int_immediate(emitter, "x1", *bits as i64);          // x1 = IEEE-754 bit pattern
            emitter.instruction("mov x2, xzr");                                 // floats do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov x1, #{}", *value as u64));        // x1 = 0 or 1
            emitter.instruction("mov x2, xzr");                                 // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = string data address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = string length
        }
        AttrArgValue::Array(_) | AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(..) => {
            // The frozen legacy AST backend does not materialize nested arrays or
            // deferred symbolic references (global/class constants, enum cases) in
            // the fallback `$__args` array; emit a null placeholder. The active EIR
            // backend materializes the real value through the factory-dispatched
            // `getArguments()` body instead.
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null placeholder
            emitter.instruction("mov x1, xzr");                                 // null carries no low word
            emitter.instruction("mov x2, xzr");                                 // null carries no high word
        }
    }
    emitter.instruction("bl __rt_mixed_from_value");                            // box the captured payload into an owned mixed cell
}

/// Emits a boxed `Mixed` cell for `arg` using x86_64 System V calling conventions.
/// Loads the runtime tag into `rax`, the low payload into `rdi`, and the
/// high payload into `rsi`, then calls `__rt_mixed_from_value` to
/// produce an owned boxed cell returned in `rax`.
fn emit_box_arg_x86_64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null payload
            emitter.instruction("xor rdi, rdi");                                // null carries no low word
            emitter.instruction("xor rsi, rsi");                                // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov rax, 0");                                  // runtime tag 0 = integer payload
            abi::emit_load_int_immediate(emitter, "rdi", *value);
            emitter.instruction("xor rsi, rsi");                                // ints do not use the high word
        }
        AttrArgValue::Float(bits) => {
            emitter.instruction("mov rax, 2");                                  // runtime tag 2 = float payload
            abi::emit_load_int_immediate(emitter, "rdi", *bits as i64);         // rdi = IEEE-754 bit pattern
            emitter.instruction("xor rsi, rsi");                                // floats do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov rax, 3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov rdi, {}", *value as u64));        // rdi = 0 or 1
            emitter.instruction("xor rsi, rsi");                                // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov rax, 1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "rdi", &sym);                     // rdi = string data address
            emitter.instruction(&format!("mov rsi, {}", len));                  // rsi = string length
        }
        AttrArgValue::Array(_) | AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(..) => {
            // The frozen legacy AST backend does not materialize nested arrays or
            // deferred symbolic references (global/class constants, enum cases) in
            // the fallback `$__args` array; emit a null placeholder. The active EIR
            // backend materializes the real value through the factory-dispatched
            // `getArguments()` body instead.
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null placeholder
            emitter.instruction("xor rdi, rdi");                                // null carries no low word
            emitter.instruction("xor rsi, rsi");                                // null carries no high word
        }
    }
    emitter.instruction("call __rt_mixed_from_value");                          // box the captured payload into an owned mixed cell
}

/// Iterates over parallel `names` and `args` slices and inserts each
/// resolved (class-name, args) pair into `unique`. Skips entries where
/// args is `None` or the class name cannot be resolved.
fn collect_from_attribute_lists(
    classes: &HashMap<String, ClassInfo>,
    names: &[String],
    args: &[Option<Vec<AttrArgEntry>>],
    unique: &mut BTreeMap<(String, Vec<AttrArgEntry>), bool>,
) {
    if names.len() != args.len() {
        return;
    }
    for (idx, attr_name) in names.iter().enumerate() {
        let Some(Some(attr_args)) = args.get(idx) else {
            continue;
        };
        // Non-class attributes (`#[Foo(1)]` with no `Foo` class) still expose
        // their arguments through reflection, so they are registered under
        // their raw name with `resolvable = false`. The map value records
        // resolvability so `newInstance()` can skip them.
        let (name, resolvable) = match resolve_class_name(classes, attr_name) {
            Some(resolved) => (resolved.to_string(), true),
            None => (attr_name.clone(), false),
        };
        unique.entry((name, attr_args.clone())).or_insert(resolvable);
    }
}
