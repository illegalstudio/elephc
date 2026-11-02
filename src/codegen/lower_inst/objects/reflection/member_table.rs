//! Purpose:
//! The member rows of the module reflection table: one data row per class constant, enum case,
//! property and method, read by the table-driven `ReflectionClassConstant`,
//! `ReflectionEnum*Case`, `ReflectionProperty` and `ReflectionMethod` constructors.
//!
//! Called from:
//! - `class_table::emit_reflection_class_table()`, which collects the rows while it builds the
//!   class rows and lays them out after them.
//!
//! Key details:
//! - A member row is one table slot ([`reflection_table::member`] words). It is shared by every
//!   class that lists the member: the index holds one key per `(kind, reflecting class, name)`
//!   pair, all naming the same slot, so an inherited member is data once.
//! - Plain data (names, flags, types, constant and default values) is in the row. What is not
//!   data (attributes with argument values, property hooks, a value the decoder lacks) hangs off
//!   the row as a code address, [`reflection_table::member::EXTRAS`], emitted only for the
//!   members that need it.

use std::collections::HashMap;

use super::class_table::{array_node, encode_constant, encode_default, node, pool};
use super::*;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::shared_reflection::ReflectionOwnerKind;
use crate::codegen::lower_inst::core_closures_php_source;
use crate::reflection_table::{member, member_flag, member_key, member_kind, type_node, ROW_WORDS};

/// The member rows collected while the class rows are built, and the keys that find them.
#[derive(Default)]
pub(super) struct MemberTables {
    /// The rows, in slot order (the first is slot `first_slot`).
    pub(super) rows: Vec<Vec<DataWord>>,
    /// `(kind, declaring class, name)` of every row built so far, for sharing.
    identities: HashMap<(u8, String, String), usize>,
    /// Every key that finds a row: the index key and the row's position in `rows`.
    pub(super) aliases: Vec<(Vec<u8>, usize)>,
    /// Positions of the method rows, whose parameter slot is relative until the layout is known.
    pub(super) method_positions: Vec<usize>,
}

impl MemberTables {
    /// Registers `reflecting_class`'s view of one member, building its row on first sight.
    fn add(
        &mut self,
        kind: u8,
        reflecting_class_lowercase: &str,
        key_name: &str,
        declaring: &str,
        build: impl FnOnce() -> Result<Vec<DataWord>>,
    ) -> Result<()> {
        let identity = (kind, declaring.to_string(), key_name.to_string());
        let position = match self.identities.get(&identity) {
            Some(position) => *position,
            None => {
                let row = build()?;
                debug_assert_eq!(row.len(), ROW_WORDS);
                self.rows.push(row);
                let position = self.rows.len() - 1;
                self.identities.insert(identity, position);
                position
            }
        };
        self.aliases
            .push((member_key(kind, reflecting_class_lowercase, key_name), position));
        Ok(())
    }
}

impl MemberTables {
    /// Registers `reflecting_class`'s view of one method: the parameter rows go first, so they
    /// are contiguous, then the method row naming the first of them.
    fn add_method(
        &mut self,
        ctx: &mut FunctionContext<'_>,
        reflecting_class_lowercase: &str,
        declaring: &str,
        listed: &ReflectionListedMember,
    ) -> Result<()> {
        let key_name = php_symbol_key(&listed.name);
        let identity = (member_kind::METHOD, declaring.to_string(), key_name.clone());
        let position = match self.identities.get(&identity) {
            Some(position) => *position,
            None => {
                let first = self.rows.len();
                for parameter in &listed.parameters {
                    let row = parameter_row(ctx, declaring, &listed.name, parameter)?;
                    self.rows.push(row);
                }
                let mut row = method_row(ctx, declaring, listed)?;
                if !listed.parameters.is_empty() {
                    // Relative until the layout is known: `position + 1`, 0 meaning none.
                    row[member::PARAMETERS] = DataWord::U64(first as u64 + 1);
                }
                row[member::PARAMETER_COUNT] = DataWord::U64(listed.parameters.len() as u64);
                self.rows.push(row);
                let position = self.rows.len() - 1;
                self.method_positions.push(position);
                self.identities.insert(identity, position);
                position
            }
        };
        self.aliases
            .push((member_key(member_kind::METHOD, reflecting_class_lowercase, &key_name), position));
        Ok(())
    }
}

/// Registers the constant and enum-case rows of one reflected class-like.
pub(super) fn collect_constant_members(
    ctx: &mut FunctionContext<'_>,
    tables: &mut MemberTables,
    reflected: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    let reflected_lowercase = php_symbol_key(reflected);
    for listed in &metadata.constant_reflection_members {
        let declaring = listed.declaring_class_name.clone().unwrap_or_else(|| reflected.to_string());
        let name = listed.name.clone();
        tables.add(member_kind::CONSTANT, &reflected_lowercase, &name, &declaring, || {
            constant_row(ctx, ReflectionOwnerKind::ClassConstant, "ReflectionClassConstant", &declaring, listed)
        })?;
    }
    for listed in metadata
        .method_members
        .iter()
        .chain(metadata.constructor_member.iter())
    {
        let declaring = listed.declaring_class_name.clone().unwrap_or_else(|| reflected.to_string());
        tables.add_method(ctx, &reflected_lowercase, &declaring, listed)?;
    }
    for listed in &metadata.property_members {
        let declaring = listed.declaring_class_name.clone().unwrap_or_else(|| reflected.to_string());
        let name = listed.name.clone();
        tables.add(member_kind::PROPERTY, &reflected_lowercase, &name, &declaring, || {
            property_row(ctx, &declaring, listed)
        })?;
    }
    for listed in &metadata.enum_case_members {
        let declaring = listed.declaring_class_name.clone().unwrap_or_else(|| reflected.to_string());
        let name = listed.name.clone();
        let (owner, class_name) = if listed.backing_value.is_some() {
            (ReflectionOwnerKind::EnumBackedCase, "ReflectionEnumBackedCase")
        } else {
            (ReflectionOwnerKind::EnumUnitCase, "ReflectionEnumUnitCase")
        };
        tables.add(member_kind::CASE, &reflected_lowercase, &name, &declaring, || {
            constant_row(ctx, owner, class_name, &declaring, listed)
        })?;
    }
    Ok(())
}

/// Splits the visibility and finality bits of a member's flags into row flag bits.
fn visibility_bits(flags: &ReflectionMemberFlags) -> i64 {
    let mut bits = 0;
    if flags.is_public {
        bits |= member_flag::PUBLIC;
    }
    if flags.is_protected {
        bits |= member_flag::PROTECTED;
    }
    if flags.is_private {
        bits |= member_flag::PRIVATE;
    }
    if flags.is_static {
        bits |= member_flag::STATIC;
    }
    if flags.is_final {
        bits |= member_flag::FINAL;
    }
    if flags.is_abstract {
        bits |= member_flag::ABSTRACT;
    }
    if flags.is_readonly {
        bits |= member_flag::READONLY;
    }
    if flags.is_promoted {
        bits |= member_flag::PROMOTED;
    }
    if flags.is_virtual {
        bits |= member_flag::VIRTUAL;
    }
    bits
}

/// A type node, or 0 for no type.
pub(super) fn encode_type(
    data: &mut DataSection,
    metadata: Option<&ReflectionParameterTypeMetadata>,
) -> DataWord {
    let Some(metadata) = metadata else { return DataWord::U64(0) };
    let named = |data: &mut DataSection, named: &ReflectionNamedTypeMetadata| -> DataWord {
        let flags = u64::from(named.allows_null) | (u64::from(named.is_builtin) << 1);
        let name = pool(data, std::slice::from_ref(&named.name));
        DataWord::Symbol(data.add_words(vec![
            DataWord::U64(type_node::NAMED as u64),
            DataWord::U64(flags),
            name,
            DataWord::U64(0),
        ]))
    };
    match metadata {
        ReflectionParameterTypeMetadata::Named(inner) => named(data, inner),
        ReflectionParameterTypeMetadata::Union(inner) => {
            let children = inner.types.iter().map(|child| named(data, child)).collect::<Vec<_>>();
            composite(data, type_node::UNION, inner.allows_null, children)
        }
        ReflectionParameterTypeMetadata::Intersection(inner) => {
            let children = inner.types.iter().map(|child| named(data, child)).collect::<Vec<_>>();
            composite(data, type_node::INTERSECTION, false, children)
        }
    }
}

fn composite(data: &mut DataSection, tag: i64, allows_null: bool, children: Vec<DataWord>) -> DataWord {
    let count = children.len() as u64;
    let list = if children.is_empty() {
        DataWord::U64(0)
    } else {
        DataWord::Symbol(data.add_words(children))
    };
    DataWord::Symbol(data.add_words(vec![
        DataWord::U64(tag as u64),
        DataWord::U64(u64::from(allows_null)),
        list,
        DataWord::U64(count),
    ]))
}

/// The address of a value node.
fn value_address(data: &mut DataSection, value: Vec<DataWord>) -> DataWord {
    DataWord::Symbol(data.add_words(value))
}

/// Builds the row of one class constant or enum case.
fn constant_row(
    ctx: &mut FunctionContext<'_>,
    owner: ReflectionOwnerKind,
    class_name: &'static str,
    declaring: &str,
    listed: &ReflectionListedMember,
) -> Result<Vec<DataWord>> {
    let mut words = vec![DataWord::U64(0); ROW_WORDS];
    words[member::NAME] = pool(ctx.data, std::slice::from_ref(&listed.name));
    words[member::CLASS] = pool(ctx.data, std::slice::from_ref(&declaring.to_string()));
    words[member::MODIFIERS] = DataWord::U64(listed.modifiers as u64);
    let mut flags = visibility_bits(&listed.flags);
    if listed.is_enum_case {
        flags |= member_flag::ENUM_CASE;
    }
    words[member::FLAGS] = DataWord::U64(flags as u64);
    words[member::TYPE] = encode_type(ctx.data, listed.type_metadata.as_ref());
    // A value the decoder cannot express is stored by the extras filler instead.
    let mut value_in_row = true;
    if let Some(value) = &listed.constant_value {
        match encode_constant(ctx.data, value) {
            Some(encoded) => words[member::VALUE] = value_address(ctx.data, encoded),
            None => value_in_row = false,
        }
    }
    let mut backing_in_row = true;
    if let Some(value) = &listed.backing_value {
        match encode_constant(ctx.data, value) {
            Some(encoded) => words[member::BACKING_VALUE] = value_address(ctx.data, encoded),
            None => backing_in_row = false,
        }
    }
    let needs_extras = !listed.attr_names.is_empty() || !value_in_row || !backing_in_row;
    if needs_extras {
        let identity = format!("{declaring}::{}", listed.name);
        let label = member_extras_label(ctx, owner, class_name, &identity, |helper_ctx| {
            if !listed.attr_names.is_empty() {
                emit_reflection_attrs_property(helper_ctx, class_name, &listed.attr_names, &listed.attr_args)?;
            }
            if !value_in_row {
                if let Some(value) = &listed.constant_value {
                    abi::emit_push_reg(helper_ctx.emitter, abi::int_result_reg(helper_ctx.emitter));
                    emit_reflection_constant_value_as_mixed(helper_ctx, value);
                    emit_reflection_owner_mixed_property_from_result(helper_ctx, class_name, "__value")?;
                }
            }
            if !backing_in_row {
                if let Some(value) = &listed.backing_value {
                    abi::emit_push_reg(helper_ctx.emitter, abi::int_result_reg(helper_ctx.emitter));
                    emit_reflection_constant_value_as_mixed(helper_ctx, value);
                    emit_reflection_owner_mixed_property_from_result(helper_ctx, class_name, "__backing_value")?;
                }
            }
            Ok(())
        })?;
        words[member::EXTRAS] = DataWord::Symbol(label);
    }
    Ok(words)
}

/// Builds the row of one property.
fn property_row(
    ctx: &mut FunctionContext<'_>,
    declaring: &str,
    listed: &ReflectionListedMember,
) -> Result<Vec<DataWord>> {
    let mut words = vec![DataWord::U64(0); ROW_WORDS];
    words[member::NAME] = pool(ctx.data, std::slice::from_ref(&listed.name));
    words[member::CLASS] = pool(ctx.data, std::slice::from_ref(&declaring.to_string()));
    words[member::MODIFIERS] = DataWord::U64(listed.modifiers as u64);
    let mut flags = visibility_bits(&listed.flags);
    if listed.default_value.is_some() {
        flags |= member_flag::HAS_DEFAULT;
    }
    if !listed.property_hook_members.is_empty() {
        flags |= member_flag::HAS_HOOKS;
    }
    words[member::FLAGS] = DataWord::U64(flags as u64);
    words[member::TYPE] = encode_type(ctx.data, listed.type_metadata.as_ref());
    let mut default_in_row = true;
    if let Some(default) = &listed.default_value {
        match encode_default(ctx.data, default) {
            Some(encoded) => words[member::VALUE] = value_address(ctx.data, encoded),
            None => default_in_row = false,
        }
    }
    let text = reflection_property_to_string(
        &listed.name,
        listed.flags,
        listed.type_metadata.as_ref(),
        listed.default_value.as_ref(),
    );
    words[member::STRING] = pool(ctx.data, std::slice::from_ref(&text));
    let needs_extras = !listed.attr_names.is_empty()
        || !listed.property_hook_members.is_empty()
        || !default_in_row;
    if needs_extras {
        let identity = format!("{declaring}::${}", listed.name);
        let label = member_extras_label(
            ctx,
            ReflectionOwnerKind::Property,
            "ReflectionProperty",
            &identity,
            |helper_ctx| {
                if !listed.attr_names.is_empty() {
                    emit_reflection_attrs_property(
                        helper_ctx,
                        "ReflectionProperty",
                        &listed.attr_names,
                        &listed.attr_args,
                    )?;
                }
                if !default_in_row {
                    emit_reflection_owner_default_value_property(
                        helper_ctx,
                        "ReflectionProperty",
                        listed.default_value.as_ref(),
                    )?;
                }
                if !listed.property_hook_members.is_empty() {
                    emit_reflection_property_hook_array_property_by_name(
                        helper_ctx,
                        "ReflectionProperty",
                        "__hooks",
                        &listed.property_hook_members,
                    )?;
                }
                Ok(())
            },
        )?;
        words[member::EXTRAS] = DataWord::Symbol(label);
    }
    Ok(words)
}

/// Builds the row of one method; the caller fills in the parameter slots.
fn method_row(
    ctx: &mut FunctionContext<'_>,
    declaring: &str,
    listed: &ReflectionListedMember,
) -> Result<Vec<DataWord>> {
    let mut words = vec![DataWord::U64(0); ROW_WORDS];
    words[member::NAME] = pool(ctx.data, std::slice::from_ref(&listed.name));
    words[member::CLASS] = pool(ctx.data, std::slice::from_ref(&declaring.to_string()));
    words[member::MODIFIERS] = DataWord::U64(listed.modifiers as u64);
    let mut flags = visibility_bits(&listed.flags);
    if listed.is_deprecated {
        flags |= member_flag::DEPRECATED;
    }
    if listed.is_generator {
        flags |= member_flag::GENERATOR;
    }
    if listed.returns_reference {
        flags |= member_flag::RETURNS_REFERENCE;
    }
    if let Some(prototype) = listed.prototype_member.as_deref() {
        flags |= member_flag::HAS_PROTOTYPE;
        let prototype_class = prototype.declaring_class_name.clone().unwrap_or_default();
        words[member::PROTOTYPE_CLASS] = pool(ctx.data, std::slice::from_ref(&prototype_class));
        words[member::PROTOTYPE_NAME] = pool(ctx.data, std::slice::from_ref(&prototype.name));
    }
    words[member::FLAGS] = DataWord::U64(flags as u64);
    words[member::TYPE] = encode_type(ctx.data, listed.type_metadata.as_ref());
    words[member::COUNT] = DataWord::U64(listed.required_parameter_count as u64);
    if !listed.attr_names.is_empty() {
        let identity = format!("{declaring}::{}()", listed.name);
        let label = member_extras_label(
            ctx,
            ReflectionOwnerKind::Method,
            "ReflectionMethod",
            &identity,
            |helper_ctx| {
                emit_reflection_attrs_property(
                    helper_ctx,
                    "ReflectionMethod",
                    &listed.attr_names,
                    &listed.attr_args,
                )
            },
        )?;
        words[member::EXTRAS] = DataWord::Symbol(label);
    }
    Ok(words)
}

/// Builds the row of one parameter of a method.
fn parameter_row(
    ctx: &mut FunctionContext<'_>,
    declaring: &str,
    function: &str,
    parameter: &ReflectionParameterMember,
) -> Result<Vec<DataWord>> {
    let mut words = vec![DataWord::U64(0); ROW_WORDS];
    words[member::NAME] = pool(ctx.data, std::slice::from_ref(&parameter.name));
    let class = parameter.declaring_class_name.clone().unwrap_or_else(|| declaring.to_string());
    words[member::CLASS] = pool(ctx.data, std::slice::from_ref(&class));
    words[member::FUNCTION] = pool(ctx.data, std::slice::from_ref(&function.to_string()));
    let mut flags = 0i64;
    for (condition, bit) in [
        (parameter.is_optional, member_flag::OPTIONAL),
        (parameter.is_variadic, member_flag::VARIADIC),
        (parameter.is_passed_by_reference, member_flag::BY_REFERENCE),
        (parameter.is_promoted, member_flag::PROMOTED),
        (parameter.has_type, member_flag::HAS_TYPE),
        (parameter.allows_null, member_flag::ALLOWS_NULL),
        (parameter.is_array_type, member_flag::ARRAY_TYPE),
        (parameter.is_callable_type, member_flag::CALLABLE_TYPE),
        // A variadic parameter never has a default in php (`max(mixed $value, mixed ...$values)`
        // reflects `values`, not `values = []`), whatever the signature models it with.
        (parameter.default_value.is_some() && !parameter.is_variadic, member_flag::HAS_DEFAULT),
        (parameter.default_value_constant_name.is_some(), member_flag::DEFAULT_CONSTANT),
    ] {
        if condition {
            flags |= bit;
        }
    }
    words[member::FLAGS] = DataWord::U64(flags as u64);
    words[member::TYPE] = encode_type(ctx.data, parameter.type_metadata.as_ref());
    words[member::COUNT] = DataWord::U64(parameter.position as u64);
    if let Some(constant) = &parameter.default_value_constant_name {
        words[member::STRING] = pool(ctx.data, std::slice::from_ref(constant));
    }
    let mut default_in_row = true;
    if let Some(default) = &parameter.default_value {
        match encode_default(ctx.data, default) {
            Some(encoded) => words[member::VALUE] = value_address(ctx.data, encoded),
            None => default_in_row = false,
        }
    }
    if !parameter.attr_names.is_empty() || !default_in_row {
        let identity = format!("{declaring}::{function}()#{}", parameter.position);
        let label = member_extras_label(
            ctx,
            ReflectionOwnerKind::Parameter,
            "ReflectionParameter",
            &identity,
            |helper_ctx| {
                if !parameter.attr_names.is_empty() {
                    emit_reflection_attrs_property(
                        helper_ctx,
                        "ReflectionParameter",
                        &parameter.attr_names,
                        &parameter.attr_args,
                    )?;
                }
                if !default_in_row {
                    emit_reflection_parameter_default_property(helper_ctx, parameter)?;
                }
                Ok(())
            },
        )?;
        words[member::EXTRAS] = DataWord::Symbol(label);
    }
    Ok(words)
}

// Silence unused warnings for the encoders the later member kinds use.
#[allow(dead_code)]
fn _encoders(data: &mut DataSection, default: &ReflectionParameterDefaultValue) {
    let _ = encode_default(data, default);
    let _ = array_node;
    let _ = node;
}

/// The function and closure rows: every compiled function, every closure body, and each builtin
/// function a `new ReflectionFunction('literal')` site names.
pub(super) fn collect_function_members(
    ctx: &mut FunctionContext<'_>,
    tables: &mut MemberTables,
    builtin_names: &[String],
) -> Result<()> {
    let module = ctx.module;
    for function in module.functions.iter().chain(module.closures.iter()) {
        if function.flags.is_main || function.flags.is_method || function.signature.is_none() {
            continue;
        }
        let is_closure = function.flags.is_closure;
        let source = if is_closure { core_closures_php_source(ctx, function) } else { None };
        let name = match &source {
            Some(source) => source.name.clone(),
            None => function.name.trim_start_matches('\\').to_string(),
        };
        let metadata = reflection_function_owner_metadata(ctx, function)?;
        let key_name = php_symbol_key(&name);
        let location = source.map(|source| (source.file, source.start_line, source.end_line));
        let listed = ReflectionListedMember {
            name,
            declaring_class_name: None,
            attr_names: metadata.attr_names.clone(),
            attr_args: metadata.attr_args.clone(),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            flags: ReflectionMemberFlags::default(),
            modifiers: 0,
            type_metadata: metadata.type_metadata.clone(),
            default_value: None,
            property_hook_members: Vec::new(),
            required_parameter_count: metadata.required_parameter_count,
            is_deprecated: metadata.is_deprecated,
            is_generator: metadata.is_generator,
            returns_reference: metadata.returns_reference,
            prototype_member: None,
            parameters: metadata.parameter_members.clone(),
        };
        tables.add_function(ctx, &key_name, &listed, is_closure, location)?;
    }
    for name in builtin_names {
        let key_name = php_symbol_key(name);
        if tables.has_function(&key_name) {
            continue;
        }
        let Some((builtin_name, signature)) = reflection_builtin_function_signature(name) else {
            continue;
        };
        let metadata = reflection_builtin_function_metadata(ctx, &builtin_name, &signature)?;
        let listed = ReflectionListedMember {
            name: builtin_name,
            declaring_class_name: None,
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            flags: ReflectionMemberFlags::default(),
            modifiers: 0,
            type_metadata: metadata.type_metadata.clone(),
            default_value: None,
            property_hook_members: Vec::new(),
            required_parameter_count: metadata.required_parameter_count,
            is_deprecated: false,
            is_generator: false,
            returns_reference: false,
            prototype_member: None,
            parameters: metadata.parameter_members.clone(),
        };
        tables.add_function(ctx, &key_name, &listed, false, None)?;
    }
    Ok(())
}

impl MemberTables {
    fn has_function(&self, key_name: &str) -> bool {
        self.identities
            .contains_key(&(member_kind::FUNCTION, String::new(), key_name.to_string()))
    }

    /// Registers one function (or closure) row with its parameter rows.
    fn add_function(
        &mut self,
        ctx: &mut FunctionContext<'_>,
        key_name: &str,
        listed: &ReflectionListedMember,
        is_closure: bool,
        location: Option<(String, u32, u32)>,
    ) -> Result<()> {
        let identity = (member_kind::FUNCTION, String::new(), key_name.to_string());
        if self.identities.contains_key(&identity) {
            return Ok(());
        }
        let first = self.rows.len();
        for parameter in &listed.parameters {
            let row = parameter_row(ctx, "", &listed.name, parameter)?;
            self.rows.push(row);
        }
        let mut row = function_row(ctx, listed, is_closure, location)?;
        if !listed.parameters.is_empty() {
            row[member::PARAMETERS] = DataWord::U64(first as u64 + 1);
        }
        row[member::PARAMETER_COUNT] = DataWord::U64(listed.parameters.len() as u64);
        self.rows.push(row);
        let position = self.rows.len() - 1;
        self.method_positions.push(position);
        self.identities.insert(identity, position);
        self.aliases.push((member_key(member_kind::FUNCTION, "", key_name), position));
        Ok(())
    }
}

/// Builds the row of one function or closure.
fn function_row(
    ctx: &mut FunctionContext<'_>,
    listed: &ReflectionListedMember,
    is_closure: bool,
    location: Option<(String, u32, u32)>,
) -> Result<Vec<DataWord>> {
    let mut words = vec![DataWord::U64(0); ROW_WORDS];
    words[member::NAME] = pool(ctx.data, std::slice::from_ref(&listed.name));
    let mut flags = 0i64;
    for (condition, bit) in [
        (listed.is_deprecated, member_flag::DEPRECATED),
        (listed.is_generator, member_flag::GENERATOR),
        (listed.returns_reference, member_flag::RETURNS_REFERENCE),
        (is_closure, member_flag::CLOSURE),
    ] {
        if condition {
            flags |= bit;
        }
    }
    if let Some((file, start, end)) = location {
        flags |= member_flag::HAS_FILE;
        words[member::DOC] = pool(ctx.data, std::slice::from_ref(&file));
        words[member::START_LINE] = DataWord::U64(u64::from(start));
        words[member::END_LINE] = DataWord::U64(u64::from(end));
    }
    words[member::FLAGS] = DataWord::U64(flags as u64);
    words[member::TYPE] = encode_type(ctx.data, listed.type_metadata.as_ref());
    words[member::COUNT] = DataWord::U64(listed.required_parameter_count as u64);
    if !listed.attr_names.is_empty() {
        let identity = format!("{}()", listed.name);
        let label = member_extras_label(
            ctx,
            ReflectionOwnerKind::Function,
            "ReflectionFunction",
            &identity,
            |helper_ctx| {
                emit_reflection_attrs_property(
                    helper_ctx,
                    "ReflectionFunction",
                    &listed.attr_names,
                    &listed.attr_args,
                )
            },
        )?;
        words[member::EXTRAS] = DataWord::Symbol(label);
    }
    Ok(words)
}
