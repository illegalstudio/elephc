//! Purpose:
//! The module reflection table: one data row per reflected class-like, and the three EIR
//! primitives (`ReflRowLookup`, `ReflWord`, `ReflPoolStr`) the synthesized `ReflectionClass`
//! constructors read it with.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()`, which emits the table before the module's bodies.
//! - `crate::codegen::lower_inst::objects::lower_object_new()`, which sends a `new ReflectionClass`
//!   (or `ReflectionObject`/`ReflectionEnum`) through the constructor instead of a per-site
//!   materializer whenever [`table_builds_reflector`] says the table answers it.
//!
//! Key details:
//! - A reflector used to be built by code emitted per class: every name array, flag and list was a
//!   sequence of stores, repeated for each reflected class. The constructor now reads those from
//!   the row, so what grows with the number of reflected classes is data.
//! - What cannot be data (attributes with their argument values, constant and default values,
//!   live static properties, the member fillers of a default build) hangs off the row as a code
//!   address, [`reflection_table::word::EXTRAS`]; classes with none of it have 0 there.
//! - A default build puts a row in the table for every class-like some `new ReflectionClass(...)`
//!   names by a literal; a closed-world build has no interpreter to answer any other name, so it
//!   gets a row for every class-like of the program.

use std::collections::BTreeSet;

use super::*;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::shared_reflection::ReflectionOwnerKind;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{Function, Module};
use crate::reflection_table::{flag, value, word, CLASS_ROW_WORDS, LOOKUP_SYMBOL, ROWS_LABEL, ROW_WORDS};
use super::member_table::{collect_constant_members, collect_function_members, MemberTables};

/// The class-like reflectors whose constructors read the table.
pub(super) fn is_table_reflector(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionObject"
            | "ReflectionEnum"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
            | "ReflectionProperty"
            | "ReflectionMethod"
            | "ReflectionParameter"
            | "ReflectionFunction"
    )
}

/// The member reflectors, which take `(class, member name)`.
fn is_member_reflector(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
            | "ReflectionProperty"
            | "ReflectionMethod"
    )
}

/// Whether the module lowered a table-reading constructor, which is what asks for the table.
fn module_reads_table(module: &Module) -> bool {
    if !crate::reflection_table::enabled_for(module.closed_world) {
        return false;
    }
    module.class_methods.iter().any(|function| {
        matches!(
            function.name.as_str(),
            "ReflectionClass::__construct"
                | "ReflectionObject::__construct"
                | "ReflectionEnum::__construct"
                | "ReflectionClassConstant::__construct"
                | "ReflectionEnumUnitCase::__construct"
                | "ReflectionEnumBackedCase::__construct"
                | "ReflectionProperty::__construct"
                | "ReflectionMethod::__construct"
                | "ReflectionParameter::__construct"
                | "ReflectionFunction::__construct"
        )
    })
}

/// Whether `new <class_name>(...)` at `inst` is answered by the constructor and the table.
///
/// A closed-world build answers every construction there. A default build keeps the interpreter
/// for names it cannot see (so runtime names and eval-declared classes behave as before) and
/// takes the table path for a literal naming a class-like the program compiled.
pub(in crate::codegen::lower_inst::objects) fn table_builds_reflector(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<bool> {
    if !is_table_reflector(class_name) || !module_reads_table(ctx.module) {
        return Ok(false);
    }
    // The parameter table constructor takes an internal row, not PHP's callable/selector pair.
    // Only table-generated reflector bodies supply that row. Public constructors retain the
    // native metadata materializer, including its three operands for class-method targets.
    if class_name == "ReflectionParameter"
        && !(ctx.function.flags.is_synthetic && ctx.function.name.starts_with("Reflection"))
    {
        return Ok(false);
    }
    if ctx.module.closed_world {
        return Ok(true);
    }
    // The synthesized bodies of a table-built reflector (member fillers, `getDeclaringClass()`,
    // `getEnum()`) construct their reflectors from the rows, which the closure over parents and
    // declaring classes made sure exist.
    if class_name != "ReflectionObject"
        && ctx.function.flags.is_synthetic
        && ctx.function.name.starts_with("Reflection")
    {
        return Ok(true);
    }
    if is_member_reflector(class_name) {
        return Ok(inst.operands.len() == 2 && literal_first_operand_names_a_compiled_class_like(ctx, inst));
    }
    if class_name == "ReflectionFunction" {
        return Ok(inst.operands.len() == 1 && literal_function_operand_is_known(ctx, inst));
    }
    if class_name == "ReflectionObject" || inst.operands.len() != 1 {
        return Ok(false);
    }
    reflection_class_literal_names_a_compiled_class_like(ctx, class_name, inst)
}

/// The literal text a constructor operand carries, looking through representation wrappers.
fn literal_text(module: &Module, function: &Function, mut value: crate::ir::ValueId) -> Option<String> {
    loop {
        let value_ref = function.value(value)?;
        let ValueDef::Instruction { inst, .. } = value_ref.def else { return None };
        let inst = function.instruction(inst)?;
        match (inst.op, inst.immediate.as_ref()) {
            (Op::MixedBox | Op::MixedClone | Op::Acquire, _) => value = *inst.operands.first()?,
            (Op::ConstStr, Some(Immediate::Data(data))) => {
                return module.data.strings.get(data.as_raw() as usize).cloned();
            }
            (Op::ConstClassName, Some(Immediate::Data(data))) => {
                return module.data.class_names.get(data.as_raw() as usize).cloned();
            }
            _ => return None,
        }
    }
}

/// Marks that every class-like needs a row.
/// Whether the first operand of a member reflector construction is a literal naming a class-like
/// the program compiled.
fn literal_first_operand_names_a_compiled_class_like(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> bool {
    let Some(operand) = inst.operands.first() else { return false };
    let Ok(reflected) = const_string_or_class_operand(ctx, *operand, "ReflectionClass") else {
        return false;
    };
    resolve_reflection_class(ctx, &reflected).is_some()
        || resolve_reflection_interface(ctx, &reflected).is_some()
        || resolve_reflection_trait(ctx, &reflected).is_some()
}

/// The function names `new ReflectionFunction('literal')` sites spell, for the builtins among them.
fn literal_function_names(ctx: &FunctionContext<'_>) -> Vec<String> {
    let module = ctx.module;
    let mut names = BTreeSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if inst.op != Op::ObjectNew || inst.operands.len() != 1 {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate.as_ref() else { continue };
            let Some(class) = module.data.class_names.get(data.as_raw() as usize) else { continue };
            if class.trim_start_matches('\\') != "ReflectionFunction" {
                continue;
            }
            if let Some(text) = literal_text(module, function, inst.operands[0]) {
                names.insert(text);
            }
        }
    }
    // A closed world reflects builtins named by strings that reach `new ReflectionFunction($f)`
    // through data, with no interpreter to answer the rest: twig's `new TwigFunction('max', 'max')`
    // is reflected by `debug:twig`, which died with "Function max() does not exist". Every string
    // constant naming a builtin that has reflection metadata gets its row; a string naming nothing
    // costs nothing (`collect_function_members` skips names without a signature).
    if module.closed_world {
        names.extend(
            module
                .data
                .strings
                .iter()
                .filter(|text| !text.is_empty() && text.len() <= 64 && !text.contains(' '))
                .cloned(),
        );
    }
    names.into_iter().collect()
}

/// Whether the operand of `new ReflectionFunction(...)` is a literal naming a compiled function
/// or a builtin the reflection tables know.
fn literal_function_operand_is_known(ctx: &FunctionContext<'_>, inst: &Instruction) -> bool {
    let Some(operand) = inst.operands.first() else { return false };
    let Some(text) = literal_text(ctx.module, ctx.function, *operand) else { return false };
    ctx.function_by_name(&text).is_some() || reflection_builtin_function_signature(&text).is_some()
}

const ALL_NAMES: &str = "*";

/// Whether `class` is `ReflectionClass` / `ReflectionEnum` or a user class deriving from one.
fn extends_table_reflector(module: &Module, class: &str) -> bool {
    let mut current = class.trim_start_matches('\\').to_string();
    for _ in 0..64 {
        if matches!(current.as_str(), "ReflectionClass" | "ReflectionEnum") || is_member_reflector(&current) {
            return true;
        }
        match module.class_infos.get(&current).and_then(|info| info.parent.clone()) {
            Some(parent) => current = parent.trim_start_matches('\\').to_string(),
            None => return false,
        }
    }
    false
}

/// The class-likes a default build's `new ReflectionClass('Literal')` / `ReflectionEnum` sites name.
fn literal_site_names(ctx: &FunctionContext<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let module = ctx.module;
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if inst.op != Op::ObjectNew || inst.operands.is_empty() {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate.as_ref() else { continue };
            let Some(class) = module.data.class_names.get(data.as_raw() as usize) else { continue };
            let bare = class.trim_start_matches('\\');
            if bare == "ReflectionObject" || !extends_table_reflector(module, bare) {
                continue;
            }
            let builtin_member = is_member_reflector(bare);
            let builtin_class = matches!(bare, "ReflectionClass" | "ReflectionEnum");
            if builtin_class && inst.operands.len() != 1 {
                continue;
            }
            if let Some(text) = literal_text(module, function, inst.operands[0]) {
                names.insert(text);
            } else if !builtin_member && !builtin_class {
                // A user subclass built from a runtime name: no interpreter answers it, so every
                // class-like gets a row.
                names.insert(ALL_NAMES.to_string());
            }
        }
    }
    names
}

/// Every class-like of a closed-world program: classes (enums included), interfaces, traits, and
/// `Closure`, which every program has although no declaration lists it (Symfony's container
/// reflects its `['Closure', 'fromCallable']` service factories).
fn all_class_like_names(module: &Module) -> BTreeSet<String> {
    module
        .class_infos
        .keys()
        .chain(module.interface_infos.keys())
        .chain(module.enum_infos.keys())
        .chain(module.trait_table.names.iter())
        .cloned()
        .chain(std::iter::once("Closure".to_string()))
        .collect()
}

/// Interns one string pool (`{pointer, length}` pairs) and returns the row word naming it.
pub(super) fn pool(data: &mut DataSection, strings: &[String]) -> DataWord {
    if strings.is_empty() {
        return DataWord::U64(0);
    }
    let mut words = Vec::with_capacity(strings.len() * 2);
    for text in strings {
        let (label, length) = data.add_string(text.as_bytes());
        words.push(DataWord::Symbol(label));
        words.push(DataWord::U64(length as u64));
    }
    DataWord::Symbol(data.add_words(words))
}

/// A value node: `[tag, a, b]`.
pub(super) fn node(tag: i64, a: DataWord, b: DataWord) -> Vec<DataWord> {
    vec![DataWord::U64(tag as u64), a, b]
}

fn int_node(value: i64) -> Vec<DataWord> {
    node(value::INT, DataWord::U64(value as u64), DataWord::U64(0))
}

fn float_node(data: &mut DataSection, number: f64) -> Vec<DataWord> {
    if number.is_nan() {
        return node(value::NAN, DataWord::U64(0), DataWord::U64(0));
    }
    if number.is_infinite() {
        let tag = if number > 0.0 { value::INFINITY } else { value::NEGATIVE_INFINITY };
        return node(tag, DataWord::U64(0), DataWord::U64(0));
    }
    node(value::FLOAT, pool(data, &[format!("{number:?}")]), DataWord::U64(0))
}

/// Encodes one class-constant value as a value node; `None` for a shape the decoder lacks.
pub(super) fn encode_constant(data: &mut DataSection, constant: &ReflectionConstantValue) -> Option<Vec<DataWord>> {
    Some(match constant {
        // The table decoder has no raising node: the extras filler materializes it.
        ReflectionConstantValue::Undefined(_) => return None,
        ReflectionConstantValue::Null => node(value::NULL, DataWord::U64(0), DataWord::U64(0)),
        ReflectionConstantValue::Bool(flag) => {
            node(value::BOOL, DataWord::U64(u64::from(*flag)), DataWord::U64(0))
        }
        ReflectionConstantValue::Int(number) => int_node(*number),
        ReflectionConstantValue::Float(number) => float_node(data, *number),
        ReflectionConstantValue::Str(text) => {
            node(value::STRING, pool(data, std::slice::from_ref(text)), DataWord::U64(0))
        }
        ReflectionConstantValue::Array(elements) => {
            let mut entries = Vec::new();
            for (index, element) in elements.iter().enumerate() {
                entries.extend(int_node(index as i64));
                entries.extend(encode_constant(data, element)?);
            }
            array_node(data, entries, elements.len())
        }
        ReflectionConstantValue::AssocArray(items) => {
            let mut entries = Vec::new();
            for item in items {
                match &item.key {
                    ReflectionDefaultArrayKey::Int(number) => entries.extend(int_node(*number)),
                    ReflectionDefaultArrayKey::Str(text) => entries.extend(node(
                        value::STRING,
                        pool(data, std::slice::from_ref(text)),
                        DataWord::U64(0),
                    )),
                }
                entries.extend(encode_constant(data, &item.value)?);
            }
            array_node(data, entries, items.len())
        }
        ReflectionConstantValue::EnumCase { enum_name, case_name } => {
            encode_enum_case(data, enum_name, case_name)
        }
    })
}

/// Shares the enum name-pair descriptor between constants and nested default values.
fn encode_enum_case(data: &mut DataSection, enum_name: &str, case_name: &str) -> Vec<DataWord> {
    node(
        value::ENUM_CASE,
        pool(data, &[enum_name.to_string(), case_name.to_string()]),
        DataWord::U64(0),
    )
}

/// Encodes a default value (property or array element) as a value node.
pub(super) fn encode_default(
    data: &mut DataSection,
    default: &ReflectionParameterDefaultValue,
) -> Option<Vec<DataWord>> {
    Some(match default {
        ReflectionParameterDefaultValue::Null => node(value::NULL, DataWord::U64(0), DataWord::U64(0)),
        ReflectionParameterDefaultValue::Bool(flag) => {
            node(value::BOOL, DataWord::U64(u64::from(*flag)), DataWord::U64(0))
        }
        ReflectionParameterDefaultValue::Int(number) => int_node(*number),
        ReflectionParameterDefaultValue::Float(number) => float_node(data, *number),
        ReflectionParameterDefaultValue::Str(text) => {
            node(value::STRING, pool(data, std::slice::from_ref(text)), DataWord::U64(0))
        }
        ReflectionParameterDefaultValue::EnumCase { enum_name, case_name } => {
            encode_enum_case(data, enum_name, case_name)
        }
        ReflectionParameterDefaultValue::Array(elements) => {
            let mut entries = Vec::new();
            for (index, element) in elements.iter().enumerate() {
                entries.extend(int_node(index as i64));
                entries.extend(encode_default(data, element)?);
            }
            array_node(data, entries, elements.len())
        }
        ReflectionParameterDefaultValue::AssocArray(items) => {
            let mut entries = Vec::new();
            for item in items {
                match &item.key {
                    ReflectionDefaultArrayKey::Int(number) => entries.extend(int_node(*number)),
                    ReflectionDefaultArrayKey::Str(text) => entries.extend(node(
                        value::STRING,
                        pool(data, std::slice::from_ref(text)),
                        DataWord::U64(0),
                    )),
                }
                entries.extend(encode_default(data, &item.value)?);
            }
            array_node(data, entries, items.len())
        }
        // A lazily failing default and a default produced by a separately stored factory have
        // no data form; the row stays with the generic reflector that knows them.
        ReflectionParameterDefaultValue::DeferredError(_)
        | ReflectionParameterDefaultValue::EirHelper(_) => return None,
        ReflectionParameterDefaultValue::Object { class_name, args, .. } => {
            let mut entries = Vec::new();
            for (index, argument) in args.iter().enumerate() {
                entries.extend(int_node(index as i64));
                entries.extend(encode_default(data, argument)?);
            }
            let arguments = array_node(data, entries, args.len());
            let arguments = DataWord::Symbol(data.add_words(arguments));
            node(value::OBJECT, pool(data, std::slice::from_ref(class_name)), arguments)
        }
    })
}

pub(super) fn array_node(data: &mut DataSection, entries: Vec<DataWord>, count: usize) -> Vec<DataWord> {
    let table = if entries.is_empty() {
        DataWord::U64(0)
    } else {
        DataWord::Symbol(data.add_words(entries))
    };
    node(value::ARRAY, table, DataWord::U64(count as u64))
}

/// The node of `getConstants()`: a string-keyed array of every constant's value.
fn encode_constants(
    data: &mut DataSection,
    members: &[ReflectionConstantMember],
) -> Option<DataWord> {
    if members.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    for member in members {
        entries.extend(node(
            value::STRING,
            pool(data, std::slice::from_ref(&member.name)),
            DataWord::U64(0),
        ));
        entries.extend(encode_constant(data, &member.value)?);
    }
    let node = array_node(data, entries, members.len());
    Some(DataWord::Symbol(data.add_words(node)))
}

/// The node of `getDefaultProperties()`: a string-keyed array of every default.
fn encode_defaults(
    data: &mut DataSection,
    members: &[ReflectionDefaultPropertyMember],
) -> Option<DataWord> {
    if members.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    for member in members {
        entries.extend(node(
            value::STRING,
            pool(data, std::slice::from_ref(&member.name)),
            DataWord::U64(0),
        ));
        entries.extend(encode_default(data, &member.value)?);
    }
    let node = array_node(data, entries, members.len());
    Some(DataWord::Symbol(data.add_words(node)))
}

/// One finished row and the lowercase name it is found by.
struct TableRow {
    key: Vec<u8>,
    words: Vec<DataWord>,
    /// Class-likes this row's reflector can ask for later (ancestors, the classes declaring its
    /// members): they need rows of their own.
    referenced: Vec<String>,
}

/// Builds the row of one class-like, emitting the extras filler it needs.
fn build_row(
    ctx: &mut FunctionContext<'_>,
    members: &mut MemberTables,
    name: &str,
    closed: bool,
) -> Result<Option<TableRow>> {
    // A default build already computes the full metadata for the literal sites it serves (the
    // member fillers are built from it); a closed world has no fillers and skips the members.
    let metadata = reflection_class_metadata_for_name(ctx, name)?;
    let Some(reflected) = metadata.reflected_name.clone() else { return Ok(None) };
    collect_constant_members(ctx, members, &reflected, &metadata)?;
    let enum_full = if metadata.is_enum {
        Some(reflection_enum_metadata_for_name(ctx, &reflected)?)
    } else {
        None
    };
    // The metadata an enum reflector's filler reads: the full enum answer in a default build, the
    // table metadata (which already carries the backing type and the cases) in a closed world.
    let enum_metadata: Option<&ReflectionOwnerMetadata> = if metadata.is_enum {
        Some(enum_full.as_ref().unwrap_or(&metadata))
    } else {
        None
    };
    let mut words = vec![DataWord::U64(0); CLASS_ROW_WORDS];
    words[word::NAME] = pool(ctx.data, std::slice::from_ref(&reflected));
    let mut flags = 0i64;
    let mut set = |condition: bool, bit: i64| {
        if condition {
            flags |= bit;
        }
    };
    set(metadata.is_final, flag::FINAL);
    // php reports an interface that declares methods as abstract.
    set(
        metadata.is_abstract || (metadata.is_interface && !metadata.method_names.is_empty()),
        flag::ABSTRACT,
    );
    set(metadata.is_interface, flag::INTERFACE);
    set(metadata.is_trait, flag::TRAIT);
    set(metadata.is_enum, flag::ENUM);
    set(metadata.is_readonly, flag::READONLY);
    set(metadata.is_anonymous, flag::ANONYMOUS);
    set(metadata.is_instantiable, flag::INSTANTIABLE);
    set(metadata.is_cloneable, flag::CLONEABLE);
    set(metadata.is_iterable, flag::ITERABLE);
    let internal = reflection_class_like_is_internal(&reflected);
    set(internal, flag::INTERNAL);
    set(!internal, flag::USER_DEFINED);
    set(metadata.doc_comment.is_some(), flag::HAS_DOC);
    set(
        enum_metadata.is_some_and(|enum_metadata| enum_metadata.type_metadata.is_some()),
        flag::BACKED,
    );
    set(closed, flag::GENERIC_MEMBERS);
    if let Some(doc) = metadata.doc_comment.as_ref() {
        words[word::DOC] = pool(ctx.data, std::slice::from_ref(doc));
    }
    if let Some(path) = reflection_source_file(ctx, "ReflectionClass", &metadata) {
        let (start, end) = reflection_source_lines(ctx, "ReflectionClass", &metadata);
        words[word::FILE] = pool(ctx.data, std::slice::from_ref(&path));
        words[word::START_LINE] = DataWord::U64(u64::from(start));
        words[word::END_LINE] = DataWord::U64(u64::from(end));
        flags |= flag::HAS_FILE;
    }
    words[word::FLAGS] = DataWord::U64(flags as u64);
    words[word::MODIFIERS] = DataWord::U64(metadata.modifiers as u64);
    let mut list = |data: &mut DataSection, at: usize, strings: &[String]| {
        words[at] = pool(data, strings);
        words[at + 1] = DataWord::U64(strings.len() as u64);
    };
    list(ctx.data, word::INTERFACES, &metadata.interface_names);
    list(ctx.data, word::TRAITS, &metadata.trait_names);
    list(ctx.data, word::PARENTS, &metadata.parent_names);
    list(ctx.data, word::METHODS, &metadata.method_names);
    list(ctx.data, word::PROPERTIES, &metadata.property_names);
    list(ctx.data, word::CONSTANTS, &metadata.constant_names);
    let listed_methods = metadata
        .method_members
        .iter()
        .map(|member| member.name.clone())
        .collect::<Vec<_>>();
    list(ctx.data, word::METHOD_MEMBERS, &listed_methods);
    let listed_properties = metadata
        .property_members
        .iter()
        .map(|member| member.name.clone())
        .collect::<Vec<_>>();
    list(ctx.data, word::PROPERTY_MEMBERS, &listed_properties);
    if let Some(enum_metadata) = enum_metadata {
        let cases = enum_metadata
            .enum_case_members
            .iter()
            .map(|member| member.name.clone())
            .collect::<Vec<_>>();
        list(ctx.data, word::CASES, &cases);
    }
    // One modifier word per constant, for `getConstants($filter)` and `getReflectionConstants`.
    if !metadata.constant_names.is_empty() {
        let modifiers = metadata
            .constant_names
            .iter()
            .map(|constant| {
                metadata
                    .constant_reflection_members
                    .iter()
                    .find(|member| &member.name == constant)
                    .map_or(1, |member| member.modifiers)
            })
            .map(|modifiers| DataWord::U64(modifiers as u64))
            .collect::<Vec<_>>();
        words[word::CONSTANT_MODIFIERS] = DataWord::Symbol(ctx.data.add_words(modifiers));
    }
    // Constant and default values the decoder can express live in the row; the extras filler
    // stores only what it cannot.
    let mut in_table = TableValues::default();
    if let Some(values) = encode_constants(ctx.data, &metadata.constant_members) {
        words[word::CONSTANT_VALUES] = values;
        in_table.constants = true;
    }
    if let Some(values) = encode_defaults(ctx.data, &metadata.default_property_members) {
        words[word::DEFAULT_VALUES] = values;
        in_table.defaults = true;
    }
    // The extras filler: only for the owners this class-like can be reflected through.
    let lazy = !closed;
    if reflection_class_extras_are_needed("ReflectionClass", &metadata, lazy, in_table) {
        let label = reflection_class_extras_label(
            ctx,
            ReflectionOwnerKind::Class,
            "ReflectionClass",
            &metadata,
            lazy,
            in_table,
        )?;
        words[word::EXTRAS] = DataWord::Symbol(label);
    }
    if closed && reflection_class_extras_are_needed("ReflectionObject", &metadata, lazy, in_table) {
        let label = reflection_class_extras_label(
            ctx,
            ReflectionOwnerKind::Object,
            "ReflectionObject",
            &metadata,
            lazy,
            in_table,
        )?;
        words[word::OBJECT_EXTRAS] = DataWord::Symbol(label);
    }
    if let Some(enum_metadata) = enum_metadata {
        if reflection_class_extras_are_needed("ReflectionEnum", enum_metadata, lazy, in_table) {
            let label = reflection_class_extras_label(
                ctx,
                ReflectionOwnerKind::Enum,
                "ReflectionEnum",
                enum_metadata,
                lazy,
                in_table,
            )?;
            words[word::ENUM_EXTRAS] = DataWord::Symbol(label);
        }
    }
    let mut referenced = metadata.parent_names.clone();
    referenced.extend(metadata.interface_names.iter().cloned());
    referenced.extend(metadata.trait_names.iter().cloned());
    for listed in metadata
        .constant_reflection_members
        .iter()
        .chain(metadata.enum_case_members.iter())
    {
        if let Some(declaring) = &listed.declaring_class_name {
            referenced.push(declaring.clone());
        }
    }
    Ok(Some(TableRow { key: php_symbol_key(&reflected).into_bytes(), words, referenced }))
}

/// Emits the reflection table, its name index and its lookup routine.
pub(in crate::codegen) fn emit_reflection_class_table(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_reads_table(module) {
        return Ok(());
    }
    let function = crate::codegen::shared_helper::helper_function("_eir_refl_table", PhpType::Void);
    let layout = crate::codegen::frame::layout_for_function(
        &function,
        emitter.target,
        regalloc_linear,
        false,
        false,
    );
    let closed = module.closed_world;
    let (keys, records) = {
        let mut ctx = FunctionContext::new(
            module, &function, emitter, data, shared, layout, false, false, false, None,
        );
        // A default build whose reflector subclasses take runtime names gives every class-like a
        // row, but must not fail on one whose metadata cannot be computed statically: a name
        // that would have been answered by the interpreter before simply has no row.
        let mut tolerant = false;
        let names = if closed {
            all_class_like_names(module)
        } else {
            let literal = literal_site_names(&ctx);
            if literal.contains(ALL_NAMES) {
                tolerant = true;
                all_class_like_names(module)
            } else {
                literal
            }
        };
        let mut members = MemberTables::default();
        let mut rows: Vec<TableRow> = Vec::new();
        let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
        let mut queue: std::collections::VecDeque<String> = names.iter().cloned().collect();
        let mut queued: BTreeSet<String> = names.iter().map(|name| php_symbol_key(name)).collect();
        while let Some(name) = queue.pop_front() {
            let built = match build_row(&mut ctx, &mut members, &name, closed) {
                Ok(row) => row,
                Err(_) if tolerant => None,
                Err(error) => return Err(error),
            };
            if let Some(row) = built {
                for referenced in &row.referenced {
                    if queued.insert(php_symbol_key(referenced)) {
                        queue.push_back(referenced.clone());
                    }
                }
                if seen.insert(row.key.clone()) {
                    rows.push(row);
                }
            }
        }
        // The function and closure rows, plus the builtin functions literal sites name.
        let builtin_names = literal_function_names(&ctx);
        collect_function_members(&mut ctx, &mut members, &builtin_names)?;
        // Slot 0 is a blank: a real row's index is never 0, so 0 can mean "no such class-like" and a
        // row index can double as the "still to read" mark of a lazily filled reflector. A class
        // row spans two slots; the member rows follow the class rows.
        let class_slots = rows.len() * (CLASS_ROW_WORDS / ROW_WORDS);
        let mut all_words: Vec<DataWord> = vec![DataWord::U64(0); ROW_WORDS];
        all_words.reserve((class_slots + members.rows.len()) * ROW_WORDS);
        let mut keys: Vec<Vec<u8>> = Vec::with_capacity(rows.len() + members.aliases.len());
        let mut records: Vec<DataWord> = Vec::with_capacity((rows.len() + members.aliases.len()) * 4);
        let add_record = |data: &mut DataSection, keys: &mut Vec<Vec<u8>>, records: &mut Vec<DataWord>, key: Vec<u8>, slot: usize| {
            let (label, length) = data.add_string(&key);
            records.extend([
                DataWord::Symbol(label),
                DataWord::U64(length as u64),
                DataWord::U64(slot as u64),
                DataWord::U64(0),
            ]);
            keys.push(key);
        };
        for (index, row) in rows.into_iter().enumerate() {
            let slot = 1 + index * (CLASS_ROW_WORDS / ROW_WORDS);
            add_record(ctx.data, &mut keys, &mut records, row.key, slot);
            all_words.extend(row.words);
        }
        // A method's parameter slot was recorded relative to the member rows; the absolute slot
        // follows the blank and the class rows.
        let mut member_rows = members.rows;
        for position in &members.method_positions {
            let parameters = crate::reflection_table::member::PARAMETERS;
            if let DataWord::U64(relative) = member_rows[*position][parameters] {
                if relative != 0 {
                    member_rows[*position][parameters] = DataWord::U64(relative + class_slots as u64);
                }
            }
        }
        for row in member_rows {
            all_words.extend(row);
        }
        for (key, position) in members.aliases {
            add_record(ctx.data, &mut keys, &mut records, key, 1 + class_slots + position);
        }
        ctx.data.add_named_words(ROWS_LABEL, all_words);
        (keys, records)
    };
    let index = data.add_words(if records.is_empty() { vec![DataWord::U64(0); 4] } else { records });
    crate::codegen::source_units::emit_lookup(
        emitter,
        data,
        LOOKUP_SYMBOL,
        "refl_row",
        &index,
        &keys,
        crate::codegen::source_units::OnMatch::Return,
    );
    Ok(())
}

/// Lowers `ReflRowLookup`: the table row of the lowercase class-like name in operand 0, or 0.
pub(in crate::codegen::lower_inst) fn lower_refl_row_lookup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let name = *inst
        .operands
        .first()
        .ok_or_else(|| CodegenIrError::invalid_module("refl_row_lookup missing name"))?;
    ctx.load_value_to_result(name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // lookup(pointer, length)
            ctx.emitter.instruction("mov x1, x2");                              // pass the name length as the lookup's second argument
            abi::emit_call_label(ctx.emitter, LOOKUP_SYMBOL);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // lookup(pointer, length)
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the name length as the lookup's second argument
            abi::emit_call_label(ctx.emitter, LOOKUP_SYMBOL);
        }
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("refl_row_lookup missing result"))?;
    ctx.store_result_value(result)
}

/// Lowers `ReflWord`: word operand 1 of the table row in operand 0.
pub(in crate::codegen::lower_inst) fn lower_refl_word(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (row, word_index) = match inst.operands.as_slice() {
        [row, word_index] => (*row, *word_index),
        _ => return Err(CodegenIrError::invalid_module("refl_word needs a row and a word")),
    };
    ctx.load_value_to_result(row)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(word_index)?;
    let rows = abi::symbol_scratch_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, scratch);
    let row_shift = (ROW_WORDS * 8).trailing_zeros();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("lsl {scratch}, {scratch}, #{row_shift}")); // row * row size
            abi::emit_symbol_address(ctx.emitter, rows, ROWS_LABEL);
            ctx.emitter.instruction(&format!("add {rows}, {rows}, {scratch}")); // the row
            ctx.emitter.instruction(&format!("ldr {result}, [{rows}, {result}, lsl #3]")); // its word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("shl {scratch}, {row_shift}"));    // row * row size
            abi::emit_symbol_address(ctx.emitter, rows, ROWS_LABEL);
            ctx.emitter.instruction(&format!("add {rows}, {scratch}"));         // the row
            ctx.emitter.instruction(&format!("mov {result}, QWORD PTR [{rows} + {result} * 8]")); // its word
        }
    }
    let destination = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("refl_word missing result"))?;
    ctx.store_result_value(destination)
}

/// Lowers `ReflAt`: word number operand 1 of the table at the address in operand 0.
pub(in crate::codegen::lower_inst) fn lower_refl_at(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (address, index) = match inst.operands.as_slice() {
        [address, index] => (*address, *index),
        _ => return Err(CodegenIrError::invalid_module("refl_at needs an address and an index")),
    };
    ctx.load_value_to_result(address)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(index)?;
    let base = abi::symbol_scratch_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, base);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {result}, [{base}, {result}, lsl #3]")); // the word
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {result}, QWORD PTR [{base} + {result} * 8]")); // the word
        }
    }
    let destination = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("refl_at missing result"))?;
    ctx.store_result_value(destination)
}

/// Lowers `ReflPoolStr`: string number operand 1 of the string pool at address operand 0.
pub(in crate::codegen::lower_inst) fn lower_refl_pool_str(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (pool_address, index) = match inst.operands.as_slice() {
        [pool_address, index] => (*pool_address, *index),
        _ => return Err(CodegenIrError::invalid_module("refl_pool_str needs a pool and an index")),
    };
    ctx.load_value_to_result(pool_address)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(index)?;
    let base = abi::symbol_scratch_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, base);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("lsl {result}, {result}, #4"));    // 16 bytes per string
            ctx.emitter.instruction(&format!("add {base}, {base}, {result}"));  // address the selected string-pool entry
            ctx.emitter.instruction(&format!("ldr x1, [{base}]"));              // string bytes
            ctx.emitter.instruction(&format!("ldr x2, [{base}, #8]"));          // string length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("shl {result}, 4"));               // 16 bytes per string
            ctx.emitter.instruction(&format!("add {base}, {result}"));          // address the selected string-pool entry
            ctx.emitter.instruction(&format!("mov rax, QWORD PTR [{base}]"));   // string bytes
            ctx.emitter.instruction(&format!("mov rdx, QWORD PTR [{base} + 8]")); // string length
        }
    }
    let destination = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("refl_pool_str missing result"))?;
    ctx.store_result_value(destination)
}

/// Lowers `ReflCallableName`: the PHP name of the callable descriptor boxed in operand 0.
///
/// A boxed callable is tag 10 with the descriptor pointer as payload; the descriptor holds the
/// name's pointer and length at fixed offsets. Anything else answers the empty string.
pub(in crate::codegen::lower_inst) fn lower_refl_callable_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    use crate::codegen_support::callable_descriptor::{
        CALLABLE_DESC_NAME_LEN_OFFSET, CALLABLE_DESC_NAME_OFFSET,
    };
    let operand = *inst
        .operands
        .first()
        .ok_or_else(|| CodegenIrError::invalid_module("refl_callable_name missing operand"))?;
    let operand_type = ctx.value_php_type(operand)?;
    ctx.load_value_to_result(operand)?;
    let none = ctx.next_label("refl_callable_none");
    let done = ctx.next_label("refl_callable_done");
    let result = abi::int_result_reg(ctx.emitter);
    let (empty_label, _) = ctx.data.add_string(b"");
    if !matches!(operand_type.codegen_repr(), PhpType::Callable) {
        // A boxed value: only tag 10 is a callable; the payload comes back in the second register.
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #10");                         // a callable descriptor?
                ctx.emitter.instruction(&format!("b.ne {none}"));               // non-callable boxed values have no descriptor name
                ctx.emitter.instruction("mov x0, x1");                          // the descriptor
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 10");                         // a callable descriptor?
                ctx.emitter.instruction(&format!("jne {none}"));                // non-callable boxed values have no descriptor name
                ctx.emitter.instruction("mov rax, rdi");                        // the descriptor
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {result}, {none}"));          // no descriptor
            ctx.emitter.instruction(&format!("ldr x2, [{result}, #{CALLABLE_DESC_NAME_LEN_OFFSET}]")); // name length
            ctx.emitter.instruction(&format!("ldr x1, [{result}, #{CALLABLE_DESC_NAME_OFFSET}]")); // name bytes
            ctx.emitter.instruction(&format!("cbz x1, {none}"));                // an unnamed descriptor has no bytes
            ctx.emitter.instruction(&format!("b {done}"));                      // return the descriptor name without the empty fallback
            ctx.emitter.label(&none);
            abi::emit_symbol_address(ctx.emitter, "x1", &empty_label);
            ctx.emitter.instruction("mov x2, xzr");                             // empty string
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {result}, {result}"));       // no descriptor
            ctx.emitter.instruction(&format!("jz {none}"));                     // a null descriptor has no name
            ctx.emitter.instruction(&format!("mov rdx, QWORD PTR [{result} + {CALLABLE_DESC_NAME_LEN_OFFSET}]")); // name length
            ctx.emitter.instruction(&format!("mov rax, QWORD PTR [{result} + {CALLABLE_DESC_NAME_OFFSET}]")); // name bytes
            ctx.emitter.instruction("test rax, rax");                           // an unnamed descriptor has no bytes
            ctx.emitter.instruction(&format!("jz {none}"));                     // an unnamed descriptor uses the empty fallback
            ctx.emitter.instruction(&format!("jmp {done}"));                    // return the descriptor name without the empty fallback
            ctx.emitter.label(&none);
            abi::emit_symbol_address(ctx.emitter, "rax", &empty_label);
            ctx.emitter.instruction("xor edx, edx");                            // empty string
            ctx.emitter.label(&done);
        }
    }
    let destination = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("refl_callable_name missing result"))?;
    ctx.store_result_value(destination)
}
