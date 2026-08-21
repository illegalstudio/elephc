//! Purpose:
//! Lowers literal and dynamic function_exists queries against AOT callable metadata.
//!
//! Called from:
//! - `super` runtime-function dispatch.
//!
//! Key details:
//! - Keeps literal folding and baked dynamic lookup candidates derived from the same symbol sources.

use super::*;

/// Lowers `function_exists($name)` over the set of functions this program actually declares.
///
/// A literal name const-folds (see [`literal_function_exists`]); any other string expression
/// lowers to a baked case-insensitive membership test over the same set
/// (see [`lower_dynamic_function_exists`]), so feature-detection loops such as
/// `foreach ($names as $n) if (function_exists($n))` compile.
pub(crate) fn lower_function_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "function_exists", 1)?;
    let strict_php = super::instruction_strict_php_profile(inst);
    let value = expect_operand(inst, 0)?;
    if let Some(function_name) = maybe_const_string_operand(ctx, value)? {
        match function_exists_needle(&function_name) {
            None => emit_static_bool(ctx, false),
            Some(bare) => {
                if let Some(group_name) = ctx.function_variant_group_name(bare) {
                    emit_variant_function_exists(ctx, &group_name);
                } else {
                    emit_static_bool(ctx, literal_function_exists(ctx, bare, strict_php));
                }
            }
        }
    } else {
        lower_dynamic_function_exists(ctx, value, strict_php)?;
    }
    store_if_result(ctx, inst)
}

/// Normalizes a `function_exists()` argument into the bare name to look up, or `None` when no
/// declaration can ever carry it.
///
/// PHP accepts a single leading namespace separator (`function_exists('\strlen') === true`) but
/// nothing else: the empty string, a lone `\`, and a doubled separator (`'\\strlen'`) are all
/// `false`. `__rt_function_exists_lookup` applies exactly this normalization to the runtime
/// needle, so the literal fold and the dynamic lookup answer identically.
pub(in crate::codegen::lower_inst) fn function_exists_needle(name: &str) -> Option<&str> {
    let bare = name.strip_prefix('\\').unwrap_or(name);
    (!bare.is_empty() && !bare.starts_with('\\')).then_some(bare)
}

/// Returns whether a compile-time `function_exists("name")` folds to true for an already
/// normalized bare name, ignoring the include-variant groups the caller handles separately.
///
/// Recognizes user functions, externs, catalog builtins, and the date/time procedural aliases
/// that `name_resolver` desugars (including the injected timezone-introspection prelude
/// functions). The aliases are matched through `is_date_procedural_alias` rather than the catalog
/// because their call sites are rewritten before codegen, so they never reach the builtin catalog
/// yet must still report as existing to match PHP. Aliases and catalog builtins live in the
/// global namespace only, so a qualified name never resolves to one — `function_exists('Foo\mktime')`
/// is `false`, as in PHP.
///
/// [`dynamic_function_exists_candidates`] enumerates exactly the names this predicate accepts;
/// a unit test below checks that correspondence in both directions.
pub(in crate::codegen::lower_inst) fn literal_function_exists(
    ctx: &FunctionContext<'_>,
    bare_name: &str,
    strict_php: bool,
) -> bool {
    ctx.function_by_name(bare_name).is_some()
        || ctx.has_extern_function(bare_name)
        || is_php_visible_builtin_function_for_target(
            bare_name,
            strict_php,
            ctx.emitter.target.platform,
        )
        || (!bare_name.contains('\\')
            && crate::name_resolver::is_date_procedural_alias(bare_name))
}

/// One baked candidate for a dynamic `function_exists()` lookup: a declared name plus, for an
/// include-loaded variant group, the `.comm` symbol that holds the currently active
/// implementation pointer.
struct FunctionExistsCandidate {
    /// The declared PHP function name, compared case-insensitively at runtime.
    name: String,
    /// The include-variant "active" symbol, or `None` for an unconditional declaration.
    active_symbol: Option<String>,
}

/// Lowers a dynamic (non-literal) `function_exists($name)` membership test.
///
/// The *set* of declared functions is fully known at compile time — an AOT binary cannot gain a
/// function at runtime, it can only activate an include-loaded variant — so only the *name* is
/// dynamic. Rather than emitting one comparison per candidate (there are several hundred, which
/// would add tens of kilobytes of code to every call site), the candidate names are emitted once
/// into `.data` as a shared 24-bytes-per-entry table and scanned by the table-driven
/// `__rt_function_exists_lookup` helper. The table is deduplicated by `DataSection::add_words`,
/// so every dynamic call site in a program shares a single table.
///
/// The runtime string is materialized into one 16-byte temporary stack slot and re-loaded from
/// there into the helper's argument registers, following the same discipline as
/// [`lower_dynamic_extension_loaded`]; the helper itself re-loads the needle from its own frame
/// before each `__rt_strcasecmp` call.
pub(in crate::codegen::lower_inst) fn lower_dynamic_function_exists(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    strict_php: bool,
) -> Result<()> {
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-string dynamic name",
        ));
    }
    let candidates = dynamic_function_exists_candidates(ctx, strict_php);
    if candidates.is_empty() {
        emit_static_bool(ctx, false);
        return Ok(());
    }

    let table_label = emit_function_exists_candidate_table(ctx, &candidates);
    let count = candidates.len() as i64;

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    // The table address and count are materialized first so the needle registers are the last
    // thing written before the call and nothing can clobber them in between.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x2", &table_label);
            abi::emit_load_int_immediate(ctx.emitter, "x3", count);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdx", &table_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", count);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_function_exists_lookup");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Emits the shared `.data` candidate table and returns its label.
///
/// Each entry is three 8-byte words: name pointer, name length, and either the include-variant
/// "active" symbol address or 0. The variant symbols are declared with `add_comm` here exactly as
/// [`emit_variant_function_exists`] does for the literal path, so the table never references an
/// undefined symbol.
fn emit_function_exists_candidate_table(
    ctx: &mut FunctionContext<'_>,
    candidates: &[FunctionExistsCandidate],
) -> String {
    let mut words = Vec::with_capacity(candidates.len() * 3);
    for candidate in candidates {
        let (name_label, name_len) = ctx.data.add_string(candidate.name.as_bytes());
        words.push(DataWord::Symbol(name_label));
        words.push(DataWord::U64(name_len as u64));
        match &candidate.active_symbol {
            Some(symbol) => {
                ctx.data.add_comm(symbol.clone(), 8);
                words.push(DataWord::Symbol(symbol.clone()));
            }
            None => words.push(DataWord::U64(0)),
        }
    }
    ctx.data.add_words(words)
}

/// Collects the function names a dynamic `function_exists()` test compares against.
///
/// Every source here is the enumeration of one disjunct of the compile-time fold, so the dynamic
/// answer cannot disagree with the literal one:
///
/// | Compile-time check                              | Enumerated here                                  |
/// |-------------------------------------------------|--------------------------------------------------|
/// | `ctx.function_variant_group_name()`             | `collect_dispatch_groups()` (with active symbol) |
/// | `ctx.function_by_name()`                        | `module.functions` + `module.closures`           |
/// | `ctx.has_extern_function()`                     | `module.extern_decls`                            |
/// | `is_php_visible_builtin_function()`             | `supported_builtin_function_names()`             |
/// | `name_resolver::is_date_procedural_alias()`     | `name_resolver::date_procedural_alias_names()`   |
///
/// Names are de-duplicated case-insensitively with `php_symbol_key`, the same key the
/// compile-time lookups use. Variant groups are collected first so that a name which is both a
/// group and a plain declaration keeps its runtime activity check, mirroring the literal path's
/// ordering (it tests `function_variant_group_name` before anything else).
fn dynamic_function_exists_candidates(
    ctx: &FunctionContext<'_>,
    strict_php: bool,
) -> Vec<FunctionExistsCandidate> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut candidates: Vec<FunctionExistsCandidate> = Vec::new();
    let mut push = |name: &str, active_symbol: Option<String>| {
        let Some(bare) = function_exists_needle(name) else {
            return;
        };
        if !seen.insert(php_symbol_key(bare)) {
            return;
        }
        candidates.push(FunctionExistsCandidate {
            name: bare.to_string(),
            active_symbol,
        });
    };

    for group in crate::ir::function_variants::collect_dispatch_groups(ctx.module) {
        let symbol = crate::names::function_variant_active_symbol(&group.name);
        push(&group.name, Some(symbol));
    }
    for function in ctx.module.functions.iter().chain(ctx.module.closures.iter()) {
        push(&function.name, None);
    }
    for extern_decl in &ctx.module.extern_decls {
        push(&extern_decl.name, None);
    }
    for name in supported_builtin_function_names_for_target(
        strict_php,
        ctx.emitter.target.platform,
    ) {
        push(name, None);
    }
    for name in crate::name_resolver::date_procedural_alias_names() {
        push(name, None);
    }
    candidates
}

/// Emits a runtime check for whether an include-loaded function variant is active.
pub(in crate::codegen::lower_inst) fn emit_variant_function_exists(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let active_symbol = crate::names::function_variant_active_symbol(function_name);
    ctx.data.add_comm(active_symbol.clone(), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &active_symbol, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // test whether an include has activated this function variant
            ctx.emitter.instruction(&format!("cset {}, ne", result_reg));       // return true only when a function variant is active
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("test {}, {}", result_reg, result_reg)
            );                                                                  // test whether an include has activated this function variant
            ctx.emitter.instruction("setne al");                                // return true only when a function variant is active
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

#[cfg(test)]
mod function_exists_tests {
    use super::*;

    /// The dynamic `function_exists()` lookup bakes a name list into the binary while the literal
    /// fold calls a predicate. Everything the *table builder* contributes that does not depend on
    /// the EIR module must therefore be accepted by the predicate, or a name would report `true`
    /// for `function_exists('x')` and `false` for `function_exists($x)`.
    #[test]
    fn every_baked_catalog_name_is_accepted_by_the_literal_fold() {
        for name in
            crate::types::checker::builtins::catalog::supported_builtin_function_names()
        {
            assert!(
                crate::types::checker::builtins::is_php_visible_builtin_function(name),
                "baked catalog candidate {:?} is rejected by the literal function_exists() fold",
                name
            );
        }
    }

    /// Same guarantee for the procedural date/time aliases, which are not registry builtins: the
    /// enumeration and the predicate must both read `DATE_PROCEDURAL_ALIASES`.
    #[test]
    fn every_baked_date_alias_is_accepted_by_the_literal_fold() {
        for name in crate::name_resolver::date_procedural_alias_names() {
            assert!(
                crate::name_resolver::is_date_procedural_alias(name),
                "baked date alias candidate {:?} is rejected by is_date_procedural_alias()",
                name
            );
            assert!(
                !name.contains('\\') && name.to_ascii_lowercase() == *name,
                "date alias {:?} must be a bare lowercase name so the baked table matches",
                name
            );
        }
    }

    /// `function_exists_needle` implements PHP's leading-separator rule for both paths: exactly
    /// one optional `\`, and never an empty remainder.
    #[test]
    fn needle_normalization_matches_php_leading_separator_rule() {
        assert_eq!(function_exists_needle("strlen"), Some("strlen"));
        assert_eq!(function_exists_needle("\\strlen"), Some("strlen"));
        assert_eq!(function_exists_needle("Foo\\bar"), Some("Foo\\bar"));
        assert_eq!(function_exists_needle("\\Foo\\bar"), Some("Foo\\bar"));
        assert_eq!(function_exists_needle("\\\\strlen"), None);
        assert_eq!(function_exists_needle("\\"), None);
        assert_eq!(function_exists_needle(""), None);
    }
}
