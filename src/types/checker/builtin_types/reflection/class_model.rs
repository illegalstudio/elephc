//! Purpose:
//! Builds ReflectionClass, ReflectionObject, and ReflectionEnum shells.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Flattened inheritance exposes only methods supported by each synthetic owner.

use super::*;

/// Builds the `ReflectionClass` shell with retained eval metadata accessors.
pub(super) fn builtin_reflection_class() -> FlattenedClass {
    let mut class = builtin_reflection_class_shell();
    // The address of each member slot's compiled filler, until a getter that reads the slot runs
    // it; 0 once filled or when the slot was filled up front.
    class
        .properties
        .extend(REFLECTION_CLASS_LAZY_SLOTS.iter().map(|slot| member_filler_property(slot)));
    add_member_fill_preambles(&mut class, REFLECTION_CLASS_MEMBER_READERS);
    if !crate::reflection_table::enabled() {
        return class;
    }
    // The constructor reads the module reflection table (`class_table_ctor`); `__generic_members`
    // records that the row asks for its member reflectors to be constructed from the row's lists.
    class.properties.push(builtin_property(
        "__generic_members",
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    // True for a reflector built from the module reflection table; `__constant_modifiers` holds
    // one modifier int per constant name, for `getConstants($filter)`.
    class.properties.push(builtin_property(
        "__table_built",
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    class.properties.push(builtin_property(
        "__constant_modifiers",
        Visibility::Private,
        Some(TypeExpr::Array(Box::new(TypeExpr::Int))),
        empty_array(),
    ));
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = class_table_ctor::table_constructor(class_table_ctor::TableConstructor::Class);
    }
    for name in ["__method_member_names", "__property_member_names"] {
        class.properties.push(builtin_property(
            name,
            Visibility::Private,
            Some(string_array_type()),
            empty_array(),
        ));
    }
    for marker in class_table_ctor::LAZY_MARKERS.iter().chain(["__values_row"].iter()) {
        class.properties.push(builtin_property(
            marker,
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
    }
    class.methods.push(class_table_ctor::fill_from_row_method());
    class.methods.push(class_table_ctor::fill_lazy_method());
    class.methods.push(class_table_ctor::fill_relations_method());
    class.methods.push(class_table_ctor::fill_member_names_method());
    class.methods.push(class_table_ctor::fill_constant_names_method(false));
    class.methods.push(class_table_ctor::fill_values_method());
    class.methods.push(class_table_ctor::decode_method());
    class.methods.extend(class_table_ctor::generic_fill_methods(false));
    add_lazy_preambles(&mut class);
    class
}

/// The slot groups a table-built reflector fills on first use: the marker property, the private
/// method that fills the group, and the properties it fills.
const REFLECTION_CLASS_LAZY_GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "__lazy_row",
        "__fill_lazy",
        &[
            "__is_final",
            "__is_abstract",
            "__is_interface",
            "__is_trait",
            "__is_enum",
            "__is_readonly",
            "__is_anonymous",
            "__is_instantiable",
            "__is_cloneable",
            "__is_iterable",
            "__is_internal",
            "__is_user_defined",
            "__generic_members",
            "__modifiers",
            "__short_name",
            "__namespace_name",
            "__in_namespace",
            "__doc_comment",
            "__has_doc_comment",
            "__file_name",
            "__start_line",
            "__end_line",
        ],
    ),
    (
        "__relations_row",
        "__fill_relations",
        &["__interface_names", "__trait_names", "__parent_names"],
    ),
    (
        "__member_names_row",
        "__fill_member_names",
        &[
            "__method_names",
            "__property_names",
            "__method_member_names",
            "__property_member_names",
        ],
    ),
    (
        "__constant_names_row",
        "__fill_constant_names",
        &["__constant_names", "__constant_modifiers", "__case_names"],
    ),
    (
        "__values_row",
        "__fill_values",
        &["__constants", "__default_properties"],
    ),
];

/// `if ($this-><marker> !== 0) { $row = $this-><marker>; $this-><marker> = 0; $this-><fill>($row); }`
fn lazy_row_preamble(marker: &str, fill: &str) -> Stmt {
    use crate::synthetic_class::{e_binop, e_int, e_method_call, e_this, e_this_prop, e_var, s_assign, s_expr, s_if, s_prop_assign};
    s_if(
        e_binop(e_this_prop(marker), BinOp::StrictNotEq, e_int(0)),
        vec![
            s_assign("lazy_row", e_this_prop(marker)),
            s_prop_assign(e_this(), marker, e_int(0)),
            s_expr(e_method_call(e_this(), fill, vec![e_var("lazy_row")])),
        ],
        vec![],
        None,
    )
}

/// Prepends to every method that reads a lazily filled slot the statement that fills it first.
///
/// A table-built reflector stores only its name and flags when constructed; the strings, name
/// lists and decoded values are read from the table the first time a getter needs them. Idempotent:
/// a method that already starts with the preamble is left alone. Run it last, so the fill comes
/// before a member filler that reads the lists.
pub(super) fn add_lazy_preambles(class: &mut FlattenedClass) {
    const OWN: &[&str] = &[
        "__construct",
        "__fill_from_row",
        "__fill_lazy",
        "__fill_relations",
        "__fill_member_names",
        "__fill_constant_names",
        "__fill_values",
        "__decode",
    ];
    for method in &mut class.methods {
        if OWN.contains(&method.name.as_str()) {
            continue;
        }
        let body = format!("{:?}", method.body);
        for (marker, fill, properties) in REFLECTION_CLASS_LAZY_GROUPS {
            let already = method
                .body
                .iter()
                .take(REFLECTION_CLASS_LAZY_GROUPS.len())
                .any(|stmt| format!("{:?}", stmt).contains(&format!("\"{}\"", marker)));
            let reads = properties
                .iter()
                .any(|property| body.contains(&format!("property: \"{}\"", property)));
            if reads && !already {
                method.body.insert(0, lazy_row_preamble(marker, fill));
            }
        }
    }
}

/// The `ReflectionClass` getters whose bodies read a member-reflector slot, with that slot.
///
/// `test_every_member_slot_reader_fills_the_members_first` keeps this list complete.
const REFLECTION_CLASS_MEMBER_READERS: &[(&str, &str)] = &[
    ("getReflectionConstants", "__reflection_constants"),
    ("getReflectionConstant", "__reflection_constants"),
    ("getMethod", "__methods"),
    ("getMethods", "__methods"),
    ("getConstructor", "__constructor"),
    ("getParentClass", "__parent_class"),
    ("isSubclassOf", "__parent_class"),
    ("getProperties", "__properties"),
    ("getProperty", "__properties"),
];

/// The `ReflectionEnum` getters whose bodies read its `__cases` member-reflector slot.
const REFLECTION_ENUM_MEMBER_READERS: &[(&str, &str)] = &[
    ("getCases", "__cases"),
    ("getCase", "__cases"),
];

/// The member-reflector slots a compiled class reflector fills on first use, each through its
/// own `<slot>_filler` (codegen's `REFLECTION_LAZY_MEMBER_SLOTS`).
const REFLECTION_CLASS_LAZY_SLOTS: &[&str] = &[
    "__reflection_constants",
    "__methods",
    "__constructor",
    "__parent_class",
    "__properties",
];

/// Prepends the fill preamble of its slot to each listed method of `class`.
fn add_member_fill_preambles(class: &mut FlattenedClass, readers: &[(&str, &str)]) {
    for method in &mut class.methods {
        if let Some((_, slot)) = readers.iter().find(|(name, _)| *name == method.name) {
            method.body.insert(0, reflection_member_fill_preamble(slot));
        }
    }
}

/// The int property holding the address of `slot`'s compiled filler, or 0.
pub(super) fn member_filler_property(slot: &str) -> ClassProperty {
    builtin_property(
        &format!("{}_filler", slot),
        Visibility::Private,
        Some(TypeExpr::Int),
        int_lit(0),
    )
}

/// `if ($this->S_filler !== 0) { __elephc_reflection_fill_members($this->S_filler, $this); }`
///
/// A compiled class reflector records one filler per member slot instead of building its member
/// reflectors (`emit_reflection_owner_object_with_members`); the filler clears the slot first,
/// so it runs once. A reflector the eval bridge built, or one filled up front, has 0 there.
pub(super) fn reflection_member_fill_preamble(slot: &str) -> Stmt {
    member_fill_preamble(slot, false)
}

/// [`reflection_member_fill_preamble`] for a slot the owner builds from the table in a default
/// build too (`ReflectionMethod::__parameters`): a `-1` mark runs the generic fill.
pub(super) fn reflection_member_fill_preamble_table(slot: &str) -> Stmt {
    member_fill_preamble(slot, crate::reflection_table::enabled())
}

fn member_fill_preamble(slot: &str, force_generic: bool) -> Stmt {
    let span = crate::span::Span::dummy();
    let filler = format!("{}_filler", slot);
    // A table-built reflector has no compiled filler for a generic slot: its `<slot>_filler` holds
    // -1 (the table constructor stores it), meaning the slot's members are constructed from the
    // row's lists now. Any other non-zero value is the address of a compiled filler (a reflector
    // built per class), which a default build still produces for the slots that are not generic.
    let generic_fill = vec![
        Stmt::new(
            StmtKind::PropertyAssign {
                object: Box::new(Expr::new(ExprKind::This, span)),
                property: filler.clone(),
                value: Expr::new(ExprKind::IntLiteral(0), span),
            },
            span,
        ),
        Stmt::new(
            StmtKind::ExprStmt(method_call_expr(
                Expr::new(ExprKind::This, span),
                &class_table_ctor::generic_fill_method_name(slot),
                Vec::new(),
                span,
            )),
            span,
        ),
    ];
    let compiled_fill = vec![Stmt::new(
        StmtKind::ExprStmt(function_call(
            "__elephc_reflection_fill_members",
            vec![
                reflection_this_property(&filler, span),
                Expr::new(ExprKind::This, span),
            ],
            span,
        )),
        span,
    )];
    let is_generic = force_generic || crate::reflection_table::generic_slots().contains(&slot);
    let fill = if crate::closed_world::is_enabled() {
        generic_fill
    } else if is_generic {
        vec![Stmt::new(
            StmtKind::If {
                condition: binary_expr(
                    reflection_this_property(&filler, span),
                    BinOp::StrictEq,
                    Expr::new(ExprKind::IntLiteral(-1), span),
                    span,
                ),
                then_body: generic_fill,
                elseif_clauses: Vec::new(),
                else_body: Some(compiled_fill),
            },
            span,
        )]
    } else {
        compiled_fill
    };
    Stmt::new(
        StmtKind::If {
            condition: binary_expr(
                reflection_this_property(&filler, span),
                BinOp::StrictNotEq,
                Expr::new(ExprKind::IntLiteral(0), span),
                span,
            ),
            then_body: fill,
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds the `ReflectionClass` members and slots, before the member-fill preambles.
fn builtin_reflection_class_shell() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionClass".to_string(),
        span: dummy(),
        extends: None,
        implements: vec!["Reflector".to_string()],
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        properties: vec![
            builtin_property("name", Visibility::Public, Some(TypeExpr::Str), empty_string()),
            builtin_property(
                "__name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__doc_comment",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__has_doc_comment",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__string",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__attrs",
                Visibility::Private,
                Some(array_type()),
                empty_array(),
            ),
            builtin_property(
                "__is_final",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_abstract",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_interface",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_trait",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_enum",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_readonly",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_anonymous",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_instantiable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_cloneable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_iterable",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_internal",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__is_user_defined",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__modifiers",
                Visibility::Private,
                Some(TypeExpr::Int),
                int_lit(0),
            ),
            builtin_property(
                "__short_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__namespace_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__in_namespace",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__interface_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__interfaces",
                Visibility::Private,
                Some(object_array_type("ReflectionClass")),
                empty_array(),
            ),
            builtin_property(
                "__trait_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__traits",
                Visibility::Private,
                Some(object_array_type("ReflectionClass")),
                empty_array(),
            ),
            builtin_property(
                "__trait_aliases",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__parent_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__method_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            // Set only by the full compiled materializer: every slot the native bodies read is
            // filled, so calls on this reflector need not go through the eval bridge.
            builtin_property(
                "__complete",
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ),
            builtin_property(
                "__property_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__constant_names",
                Visibility::Private,
                Some(string_array_type()),
                empty_array(),
            ),
            builtin_property(
                "__constants",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__default_properties",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__default_error",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__static_properties",
                Visibility::Private,
                Some(mixed_type()),
                empty_array(),
            ),
            builtin_property(
                "__reflection_constants",
                Visibility::Private,
                Some(object_array_type("ReflectionClassConstant")),
                empty_array(),
            ),
            builtin_property(
                "__methods",
                Visibility::Private,
                Some(object_array_type("ReflectionMethod")),
                empty_array(),
            ),
            builtin_property(
                "__constructor",
                Visibility::Private,
                Some(nullable_object_type("ReflectionMethod")),
                null_expr(),
            ),
            builtin_property(
                "__parent_class",
                Visibility::Private,
                Some(mixed_type()),
                false_bool(),
            ),
            builtin_property(
                "__properties",
                Visibility::Private,
                Some(object_array_type("ReflectionProperty")),
                empty_array(),
            ),
            builtin_property(
                "__file_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__start_line",
                Visibility::Private,
                Some(TypeExpr::Int),
                int_lit(0),
            ),
            builtin_property(
                "__end_line",
                Visibility::Private,
                Some(TypeExpr::Int),
                int_lit(0),
            ),
        ],
        methods: vec![
            builtin_reflection_owner_constructor_method(vec![(
                "class_name",
                Some(class_string_or_object_type()),
                None,
                false,
            )]),
            builtin_reflection_class_string_method("getName", "__name"),
            builtin_reflection_class_string_method("__toString", "__string"),
            builtin_reflection_slot_getter(
                "getFileName",
                "__file_name",
                string_or_bool_type(),
            ),
            builtin_reflection_slot_getter(
                "getStartLine",
                "__start_line",
                int_or_bool_type(),
            ),
            builtin_reflection_slot_getter(
                "getEndLine",
                "__end_line",
                int_or_bool_type(),
            ),
            builtin_reflection_class_doc_comment_method(),
            builtin_reflection_constant_false_union_method("getExtensionName"),
            builtin_reflection_constant_null_mixed_method("getExtension"),
            builtin_reflection_class_string_method("getShortName", "__short_name"),
            builtin_reflection_class_string_method("getNamespaceName", "__namespace_name"),
            builtin_reflection_class_bool_method("inNamespace", "__in_namespace"),
            builtin_reflection_class_array_method(
                "getInterfaceNames",
                "__interface_names",
                string_array_type(),
            ),
            builtin_reflection_class_objects_from_names_method(
                "getInterfaces",
                "__interface_names",
                object_array_type("ReflectionClass"),
            ),
            builtin_reflection_class_array_method(
                "getTraitNames",
                "__trait_names",
                string_array_type(),
            ),
            builtin_reflection_class_objects_from_names_method(
                "getTraits",
                "__trait_names",
                object_array_type("ReflectionClass"),
            ),
            builtin_reflection_class_array_method(
                "getTraitAliases",
                "__trait_aliases",
                string_array_type(),
            ),
            builtin_reflection_class_bool_method("isFinal", "__is_final"),
            builtin_reflection_class_bool_method("isAbstract", "__is_abstract"),
            builtin_reflection_class_bool_method("isInterface", "__is_interface"),
            builtin_reflection_class_bool_method("isTrait", "__is_trait"),
            builtin_reflection_class_bool_method("isEnum", "__is_enum"),
            builtin_reflection_class_bool_method("isReadOnly", "__is_readonly"),
            builtin_reflection_class_bool_method("isAnonymous", "__is_anonymous"),
            builtin_reflection_class_bool_method("isInstantiable", "__is_instantiable"),
            builtin_reflection_class_bool_method("isCloneable", "__is_cloneable"),
            builtin_reflection_class_bool_method("isIterable", "__is_iterable"),
            builtin_reflection_class_bool_method("isIterateable", "__is_iterable"),
            builtin_reflection_class_bool_method("isInternal", "__is_internal"),
            builtin_reflection_class_bool_method("isUserDefined", "__is_user_defined"),
            builtin_reflection_class_int_method("getModifiers", "__modifiers"),
            builtin_reflection_class_has_name_method("hasMethod", "__method_names", true),
            builtin_reflection_class_has_name_method("hasProperty", "__property_names", false),
            builtin_reflection_class_has_name_method("hasConstant", "__constant_names", false),
            builtin_reflection_class_get_constant_method(),
            builtin_reflection_class_get_constants_method(),
            builtin_reflection_class_get_default_properties_method(),
            builtin_reflection_class_mixed_method("getStaticProperties", "__static_properties"),
            builtin_reflection_class_get_static_property_value_method(),
            builtin_reflection_class_set_static_property_value_method(),
            builtin_reflection_class_filtered_array_method(
                "getReflectionConstants",
                "__reflection_constants",
                object_array_type("ReflectionClassConstant"),
            ),
            builtin_reflection_class_get_reflection_constant_method(),
            builtin_reflection_class_implements_interface_method(),
            builtin_reflection_class_is_subclass_of_method(),
            builtin_reflection_class_is_instance_method(),
            builtin_reflection_class_get_member_method(
                "getMethod",
                "__methods",
                "ReflectionMethod",
                true,
            ),
            builtin_reflection_class_nullable_object_method(
                "getConstructor",
                "__constructor",
                "ReflectionMethod",
            ),
            builtin_reflection_class_mixed_method("getParentClass", "__parent_class"),
            builtin_reflection_class_filtered_array_method(
                "getMethods",
                "__methods",
                object_array_type("ReflectionMethod"),
            ),
            builtin_reflection_class_filtered_array_method(
                "getProperties",
                "__properties",
                object_array_type("ReflectionProperty"),
            ),
            builtin_reflection_class_get_member_method(
                "getProperty",
                "__properties",
                "ReflectionProperty",
                false,
            ),
            builtin_reflection_class_new_instance_method(),
            builtin_reflection_class_new_instance_args_method(),
            builtin_reflection_class_new_instance_without_constructor_method(),
            builtin_reflection_class_new_lazy_ghost_method(),
            builtin_reflection_class_new_lazy_proxy_method(),
            builtin_reflection_class_is_uninitialized_lazy_object_method(),
            builtin_reflection_class_lazy_object_identity_method("initializeLazyObject"),
            builtin_reflection_class_lazy_object_identity_method("markLazyObjectAsInitialized"),
            builtin_reflection_class_reset_as_lazy_method(false),
            builtin_reflection_class_reset_as_lazy_method(true),
            builtin_reflection_owner_get_attributes_method("ReflectionClass"),
        ],
        attributes: Vec::new(),
        constants: reflection_class_constants(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds the synthetic `ReflectionObject` class with ReflectionClass metadata slots.
pub(super) fn builtin_reflection_object_class() -> FlattenedClass {
    let mut class = builtin_reflection_class();
    class.name = "ReflectionObject".to_string();
    class.extends = Some("ReflectionClass".to_string());
    class.properties.push(builtin_property(
        "__object",
        Visibility::Private,
        Some(mixed_type()),
        null_expr(),
    ));
    if let Some(constructor) = class
        .methods
        .iter_mut()
        .find(|method| method.name == "__construct")
    {
        if crate::reflection_table::enabled() {
            *constructor =
                class_table_ctor::table_constructor(class_table_ctor::TableConstructor::Object);
        }
    }
    class
}

/// Builds the synthetic `ReflectionEnum` class with flattened ReflectionClass members.
///
/// php's `ReflectionEnum` extends `ReflectionClass`, so it keeps every inherited method: the
/// materializer fills the same member slots for an enum as for a class. An allow-list of the
/// name and flag getters used to stand here, and `getMethods()` / `getConstants()` /
/// `implementsInterface()` on an enum were compile errors.
pub(super) fn builtin_reflection_enum_class() -> FlattenedClass {
    let mut class = builtin_reflection_class();
    class.name = "ReflectionEnum".to_string();
    class.properties.extend([
        builtin_property(
            "__case_names",
            Visibility::Private,
            Some(string_array_type()),
            empty_array(),
        ),
        // Typed like the sibling member slots (`__methods`, `__properties`): the filler stores a
        // raw object array, and an untyped `array` slot is a boxed Mixed cell, so `getCases()`
        // unboxed the raw array pointer as a box and answered "float given".
        builtin_property(
            "__cases",
            Visibility::Private,
            Some(object_array_type("ReflectionEnumUnitCase")),
            empty_array(),
        ),
        builtin_property(
            "__is_backed",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ),
        builtin_property(
            "__backing_type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ),
    ]);
    class.methods.extend([
        builtin_reflection_class_bool_method("isBacked", "__is_backed"),
        builtin_reflection_class_mixed_method("getBackingType", "__backing_type"),
        builtin_reflection_class_array_method("getCases", "__cases", array_type()),
        builtin_reflection_enum_get_case_method(),
        builtin_reflection_enum_has_case_method(),
    ]);
    class.properties.push(member_filler_property("__cases"));
    if crate::reflection_table::enabled() {
        // A backed enum's cases live in a slot typed for them: arrays are invariant.
        class.properties.push(builtin_property(
            "__backed_cases",
            Visibility::Private,
            Some(object_array_type("ReflectionEnumBackedCase")),
            empty_array(),
        ));
        for method in &mut class.methods {
            match method.name.as_str() {
                "getCases" => {
                    method.body = vec![
                        Stmt::new(
                            StmtKind::If {
                                condition: reflection_this_property("__is_backed", dummy()),
                                then_body: vec![Stmt::new(
                                    StmtKind::Return(Some(reflection_this_property("__backed_cases", dummy()))),
                                    dummy(),
                                )],
                                elseif_clauses: Vec::new(),
                                else_body: None,
                            },
                            dummy(),
                        ),
                        Stmt::new(
                            StmtKind::Return(Some(reflection_this_property("__cases", dummy()))),
                            dummy(),
                        ),
                    ];
                }
                "getCase" => {
                    if let Some(Stmt { kind: StmtKind::Foreach { array, .. }, .. }) = method.body.first_mut() {
                        *array = method_call_expr(
                            Expr::new(ExprKind::This, dummy()),
                            "getCases",
                            Vec::new(),
                            dummy(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    add_member_fill_preambles(&mut class, REFLECTION_ENUM_MEMBER_READERS);
    if !crate::reflection_table::enabled() {
        return class;
    }
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = class_table_ctor::table_constructor(class_table_ctor::TableConstructor::Enum);
    }
    if let Some(lazy) = class.methods.iter_mut().find(|method| method.name == "__fill_constant_names") {
        *lazy = class_table_ctor::fill_constant_names_method(true);
    }
    {
        // The enum copy also builds its case reflectors.
        class.methods.extend(
            class_table_ctor::generic_fill_methods(true)
                .into_iter()
                .filter(|method| method.name == class_table_ctor::generic_fill_method_name("__cases")),
        );
    }
    add_lazy_preambles(&mut class);
    class
}

/// Builds `ReflectionEnum::getCase(string $name)`: the case reflector of that name, or php's
/// `ReflectionException` -- "Case E::B does not exist", or "E::C is not a case" when the name is
/// one of the enum's plain constants.
fn builtin_reflection_enum_get_case_method() -> ClassMethod {
    let span = crate::span::Span::dummy();
    let name = variable_expr("name", span);
    let member = variable_expr("member", span);
    let found = binary_expr(
        method_call_expr(member.clone(), "getName", Vec::new(), span),
        BinOp::StrictEq,
        name.clone(),
        span,
    );
    let qualified = concat_expr(
        concat_expr(reflection_this_property("__name", span), string_lit("::", span), span),
        name.clone(),
        span,
    );
    let is_constant = function_call(
        "in_array",
        vec![
            name.clone(),
            reflection_this_property("__constant_names", span),
            Expr::new(ExprKind::BoolLiteral(true), span),
        ],
        span,
    );
    let body = vec![
        Stmt::new(
            StmtKind::Foreach {
                array: reflection_this_property("__cases", span),
                key_var: None,
                value_var: "member".to_string(),
                value_by_ref: false,
                body: vec![Stmt::new(
                    StmtKind::If {
                        condition: found,
                        then_body: vec![Stmt::new(StmtKind::Return(Some(member)), span)],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    span,
                )],
            },
            span,
        ),
        Stmt::new(
            StmtKind::If {
                condition: is_constant,
                then_body: vec![throw_new_reflection_exception(
                    concat_expr(qualified.clone(), string_lit(" is not a case", span), span),
                    span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ),
        throw_new_reflection_exception(
            concat_expr(
                concat_expr(string_lit("Case ", span), qualified, span),
                string_lit(" does not exist", span),
                span,
            ),
            span,
        ),
    ];
    ClassMethod {
        type_params: Vec::new(),
        name: "getCase".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body,
        span,
        attributes: Vec::new(),
    }
}

/// Builds `ReflectionEnum::hasCase(string $name)`: whether the enum declares a case of exactly
/// that name (case names are case-sensitive, like constants).
fn builtin_reflection_enum_has_case_method() -> ClassMethod {
    let span = crate::span::Span::dummy();
    let contains = function_call(
        "in_array",
        vec![
            variable_expr("name", span),
            reflection_this_property("__case_names", span),
            Expr::new(ExprKind::BoolLiteral(true), span),
        ],
        span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "hasCase".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(contains)), span)],
        span,
        attributes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every class-reflector method that reads a member-reflector slot runs the member
    /// filler first. A compiled reflector leaves those slots empty until then, so a reader
    /// missing from `REFLECTION_CLASS_MEMBER_READERS` / `REFLECTION_ENUM_MEMBER_READERS` would
    /// silently answer "no methods" for a class that has some.
    #[test]
    fn test_every_member_slot_reader_fills_the_members_first() {
        for class in [
            builtin_reflection_class(),
            builtin_reflection_object_class(),
            builtin_reflection_enum_class(),
        ] {
            for method in &class.methods {
                let body = format!("{:?}", method.body);
                // A lazy compiled reflector never builds these: nothing may read them.
                for unread in ["__interfaces", "__traits"] {
                    assert!(
                        !body.contains(&format!("property: \"{}\"", unread)),
                        "{}::{} reads {}, which lazy reflectors leave empty",
                        class.name,
                        method.name,
                        unread
                    );
                }
                for slot in REFLECTION_CLASS_LAZY_SLOTS.iter().chain(["__cases"].iter()) {
                    if !body.contains(&format!("property: \"{}\"", slot)) {
                        continue;
                    }
                    // The lazy-row preambles come first: the member fillers read the name lists.
                    let first = method
                        .body
                        .iter()
                        .find(|stmt| {
                            let text = format!("{:?}", stmt);
                            !REFLECTION_CLASS_LAZY_GROUPS
                                .iter()
                                .any(|(marker, _, _)| text.contains(&format!("\"{}\"", marker)))
                        })
                        .map(|stmt| format!("{:?}", stmt));
                    assert!(
                        first.is_some_and(|stmt| stmt
                            .contains("__elephc_reflection_fill_members")
                            && stmt.contains(&format!("\"{}_filler\"", slot))),
                        "{}::{} reads {} without running its filler first",
                        class.name,
                        method.name,
                        slot
                    );
                }
            }
        }
    }
}
