//! Purpose:
//! Renders the PHP dump `ReflectionMethod`, `ReflectionFunction` and `ReflectionParameter`
//! answer from `__toString()`, at compile time, for the `__string` slot each one carries.
//!
//! Called from:
//! - `owner_emission` when a Reflection owner object is materialized.
//! - `member_object_emit` for the entries `ReflectionClass::getMethods()` hands back.
//! - `parameter_property_emit` for each `ReflectionParameter`.
//!
//! Key details:
//! - The rendering follows PHP 8.5 except for the `@@ <file> <line> - <line>` header line, which a
//!   compiled binary cannot honestly answer: the source it was built from need not exist where it
//!   runs, and baking the build machine's path in would be worse than leaving it out. The eval
//!   bridge omits the same line and shares scalar and union formatting with this renderer.
//! - Union members use the shared PHP type-mask order, including the iterable expansion.
//! - Dumps use the compiler's literal-byte representation until property emission, so raw
//!   non-UTF-8 bytes in AST exports survive alongside real Unicode names without collisions.
//! - Whether a callable is internal is decided by the caller (the builtin signature registry for a
//!   function, the class catalog for a method), the same answer `isInternal()` gives. Only the
//!   MODULE comes from here: the shared builtin contract for a function, the declaring class's
//!   contract for a method, printed under the module's registered name (`SPL`).

use super::{
    ReflectionDefaultArrayKey, ReflectionListedMember, ReflectionMemberFlags,
    ReflectionParameterDefaultValue, ReflectionParameterMember, ReflectionParameterTypeMetadata,
};
use crate::names::is_generated_local_name;
use elephc_builtin_contract::PhpModule;

use super::class_traits::reflection_class_like_is_internal;
use super::property_members::{reflection_property_visibility_label, reflection_type_metadata_to_string};

/// Renders `ReflectionMethod::__toString()` for one reflected method.
pub(super) fn reflection_method_to_string(
    name: &str,
    declaring_class_name: Option<&str>,
    flags: ReflectionMemberFlags,
    prototype_class_name: Option<&str>,
    inherited_class_name: Option<&str>,
    overwritten_class_name: Option<&str>,
    is_internal: bool,
    is_deprecated: bool,
    returns_reference: bool,
    parameters: &[ReflectionParameterMember],
    return_type: Option<&ReflectionParameterTypeMetadata>,
) -> String {
    let mut parts = Vec::new();
    if flags.is_abstract {
        parts.push("abstract");
    }
    if flags.is_final {
        parts.push("final");
    }
    if flags.is_static {
        parts.push("static");
    }
    parts.push(reflection_property_visibility_label(flags));
    parts.push("method");

    let module = inherited_class_name
        .and_then(elephc_builtin_contract::lookup_class)
        .or_else(|| declaring_class_name.and_then(elephc_builtin_contract::lookup_class))
        .map(|class| class.module);
    let mut origin_parts = vec![reflection_origin_label(is_internal, is_deprecated, module)];
    if let Some(class_name) = overwritten_class_name {
        origin_parts.push(format!("overwrites {}", reflection_dump_text(class_name)));
    }
    if let Some(class_name) = inherited_class_name {
        origin_parts.push(format!("inherits {}", reflection_dump_text(class_name)));
    }
    if let Some(class_name) = prototype_class_name {
        origin_parts.push(format!("prototype {}", reflection_dump_text(class_name)));
    }
    // PHP marks constructors specially in ReflectionMethod dumps; destructors have no dtor tag.
    if name.eq_ignore_ascii_case("__construct") {
        origin_parts.push("ctor".to_string());
    }
    let origin = format!("<{}>", origin_parts.join(", "));
    let display_name = if returns_reference {
        format!("&{}", reflection_dump_text(name))
    } else {
        reflection_dump_text(name)
    };
    let header = format!("Method [ {} {} {} ]", origin, parts.join(" "), display_name);
    let tentative_return = is_internal && declaring_class_name.is_some_and(|class|
        elephc_builtin_contract::reflection_methods::has_tentative_return(class, name));
    reflection_callable_body(&header, parameters, return_type, tentative_return)
}

/// Renders ReflectionFunction::__toString() for one reflected function.
pub(super) fn reflection_function_to_string(
    name: &str,
    is_internal: bool,
    is_deprecated: bool,
    returns_reference: bool,
    parameters: &[ReflectionParameterMember],
    return_type: Option<&ReflectionParameterTypeMetadata>,
) -> String {
    let normalized_name = name.trim_start_matches(char::from(92));
    let display_name = if returns_reference {
        format!("&{}", reflection_dump_text(normalized_name))
    } else {
        reflection_dump_text(normalized_name)
    };
    let module = elephc_builtin_contract::lookup(name).map(|contract| contract.module);
    let header = format!(
        "Function [ <{}> function {} ]",
        reflection_origin_label(is_internal, is_deprecated, module),
        display_name
    );
    reflection_callable_body(&header, parameters, return_type, false)
}

/// Returns PHP's callable origin label, including the owning module when it is known.
fn reflection_origin_label(is_internal: bool, is_deprecated: bool, module: Option<PhpModule>) -> String {
    let mut label = if is_internal { "internal" } else { "user" }.to_string();
    if is_deprecated { label.push_str(", deprecated"); }
    if let Some(module) = module.filter(|_| is_internal) {
        label.push(':');
        label.push_str(module.registered_name());
    }
    label
}

/// Appends PHP's parameter/return block to a rendered function or method header.
///
/// A callable with no parameters AND no declared return type prints an EMPTY body: PHP omits the
/// `- Parameters [0]` block entirely there, and prints it as soon as either half has something to
/// say. Measured on 8.5.10 with `function f() {}` against `function f(): void {}`.
///
/// The blank line before `- Parameters` belongs to that block, not to the omitted `@@` line: an
/// internal callable has no `@@` line and PHP still prints it (`ReflectionFunction('pi')`).
fn reflection_callable_body(
    header: &str,
    parameters: &[ReflectionParameterMember],
    return_type: Option<&ReflectionParameterTypeMetadata>,
    tentative_return: bool,
) -> String {
    // A scope that calls `func_get_args()` carries a synthesized `mixed ...$__elephc_func_args`
    // so it can read surplus positional arguments. It is an ABI slot, not a PHP parameter, and
    // `reflection_parameter_members_with_declaring_function` already drops it, so neither the dump
    // nor `getParameters()` sees it. This filter only keeps the dump honest should a member list
    // ever be built another way. `is_generated_local_name` is the predicate the rest of the
    // compiler uses to keep these out of `get_defined_vars()` and eval scopes.
    let parameters: Vec<&ReflectionParameterMember> = parameters
        .iter()
        .filter(|parameter| !is_generated_local_name(&parameter.name))
        .collect();
    if parameters.is_empty() && return_type.is_none() {
        return format!("{header} {{\n}}\n");
    }
    let mut rendered = format!("{header} {{\n\n  - Parameters [{}] {{\n", parameters.len());
    for (visible_position, parameter) in parameters.into_iter().enumerate() {
        rendered.push_str("    ");
        rendered.push_str(&reflection_parameter_to_string_at_position(
            parameter,
            visible_position as i64,
        ));
        rendered.push('\n');
    }
    rendered.push_str("  }\n");
    if let Some(return_type) = return_type {
        rendered.push_str(if tentative_return { "  - Tentative return [ " } else { "  - Return [ " });
        rendered.push_str(&reflection_dump_text(&reflection_type_metadata_to_string(return_type)));
        rendered.push_str(" ]\n");
    }
    rendered.push_str("}\n");
    rendered
}

/// Renders `ReflectionParameter::__toString()` for one reflected parameter.
///
/// The `__toString` slot used to hold the parameter's bare NAME, so `(string) $parameter` answered
/// `a` where PHP answers `Parameter #0 [ <required> int $a ]`.
pub(super) fn reflection_parameter_to_string(parameter: &ReflectionParameterMember) -> String {
    reflection_parameter_to_string_at_position(parameter, parameter.position)
}

/// Renders a parameter at its visible position after hidden compiler-only formals are removed.
fn reflection_parameter_to_string_at_position(
    parameter: &ReflectionParameterMember,
    position: i64,
) -> String {
    let mut rendered = format!(
        "Parameter #{} [ <{}> ",
        position,
        if parameter.is_optional {
            "optional"
        } else {
            "required"
        }
    );
    if let Some(type_metadata) = parameter.type_metadata.as_ref() {
        rendered.push_str(&reflection_dump_text(&reflection_type_metadata_to_string(type_metadata)));
        rendered.push(' ');
    }
    if parameter.is_passed_by_reference {
        rendered.push('&');
    }
    if parameter.is_variadic {
        rendered.push_str("...");
    }
    rendered.push('$');
    rendered.push_str(&reflection_dump_text(&parameter.name));
    if !parameter.is_variadic {
        if let Some(default) = reflection_parameter_default_to_string(parameter) {
            rendered.push_str(" = ");
            rendered.push_str(&default);
        }
    }
    rendered.push_str(" ]");
    rendered
}

/// Renders one parameter's default the way PHP's dump does.
///
/// A default written as a constant prints the CONSTANT's name, not its value, `= LIMIT` and
/// `= self::CEIL`, measured on 8.5.10, which is why the constant name is consulted first.
/// The exported AST comes before both: an object default keeps its written arguments
/// (`new \Foo(Foo::VALUE)`), and a named class constant prints fully qualified
/// (`= \N\Foo::VALUE`) although `getDefaultValueConstantName()` answers `N\Foo::VALUE`.
fn reflection_parameter_default_to_string(parameter: &ReflectionParameterMember) -> Option<String> {
    if let Some(exported) = parameter.default_value_export.as_ref() {
        return Some(exported.clone());
    }
    if let Some(constant_name) = parameter.default_value_constant_name.as_ref() {
        return Some(reflection_dump_text(constant_name));
    }
    parameter.default_value.as_ref().and_then(|value| match value {
        ReflectionParameterDefaultValue::DeferredError(_)
        | ReflectionParameterDefaultValue::EirHelper(_) => None,
        _ => Some(reflection_dump_default_value(value)),
    })
}

/// Renders one default value, including the array and object forms PHP prints in a dump.
pub(super) fn reflection_dump_default_value(default: &ReflectionParameterDefaultValue) -> String {
    match default {
        ReflectionParameterDefaultValue::Int(value) => value.to_string(),
        ReflectionParameterDefaultValue::Bool(value) => value.to_string(),
        ReflectionParameterDefaultValue::Float(value) => reflection_dump_float(*value),
        // Quote marks remain literal; control and non-ASCII bytes use PHP's printable escapes.
        ReflectionParameterDefaultValue::Str(value) => elephc_builtin_contract::reflection_format::string(
            &elephc_builtin_contract::string_literal::literal_bytes(value),
        ),
        ReflectionParameterDefaultValue::Null => String::from("NULL"),
        ReflectionParameterDefaultValue::DeferredError(_)
        | ReflectionParameterDefaultValue::EirHelper(_) => String::new(),
        // Written arguments retain their source order and names independently of the
        // constructor slots used by getDefaultValue, including omitted defaults.
        ReflectionParameterDefaultValue::Object {
            class_name,
            written_args,
            ..
        } => {
            let rendered = written_args
                .iter()
                .map(|arg| {
                    let value = reflection_dump_default_value(&arg.value);
                    match &arg.name {
                        Some(name) => format!("{}: {value}", reflection_dump_text(name)),
                        None => value,
                    }
                })
                .collect::<Vec<_>>();
            format!(
                "new \\{}({})",
                reflection_dump_text(class_name.trim_start_matches('\\')),
                rendered.join(", ")
            )
        }
        ReflectionParameterDefaultValue::Array(values) => {
            let rendered = values
                .iter()
                .map(reflection_dump_default_value)
                .collect::<Vec<_>>();
            format!("[{}]", rendered.join(", "))
        }
        ReflectionParameterDefaultValue::AssocArray(entries) => {
            let rendered = entries
                .iter()
                .map(|entry| {
                    let key = match &entry.key {
                        ReflectionDefaultArrayKey::Int(value) => value.to_string(),
                        ReflectionDefaultArrayKey::Str(value) => {
                            elephc_builtin_contract::reflection_format::string(
                                &elephc_builtin_contract::string_literal::literal_bytes(value),
                            )
                        }
                    };
                    format!("{key} => {}", reflection_dump_default_value(&entry.value))
                })
                .collect::<Vec<_>>();
            format!("[{}]", rendered.join(", "))
        }
    }
}

/// Renders a float default using PHP's 14 significant digits and exponent spelling.
pub(super) fn reflection_dump_float(value: f64) -> String {
    elephc_builtin_contract::reflection_format::float(value)
}

/// Encodes real UTF-8 text without confusing private-use names with escaped PHP bytes.
pub(super) fn reflection_dump_text(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for ch in value.chars() {
        elephc_builtin_contract::string_literal::push_literal_char(ch, &mut encoded);
    }
    encoded
}

/// Renders the `ReflectionMethod::__toString()` answer for one listed member.
pub(super) fn reflection_listed_method_to_string(
    ctx: &super::FunctionContext<'_>, member: &ReflectionListedMember,
) -> String {
    let inherited_class_name = member.declaring_class_name.as_deref().and_then(|class| {
        let php_owner = reflection_class_like_is_internal(class).then(||
            elephc_builtin_contract::reflection_inheritance::inherited_declaring_class(
                class, &member.name,
            )).flatten();
        php_owner.or_else(|| member.flags.is_inherited.then_some(class))
    });
    let overwritten_class_name = if inherited_class_name.is_none() {
        member.declaring_class_name.as_deref()
            .and_then(|class| ctx.module.class_infos.get(class))
            .and_then(|info| super::reflection_parent_class_name(ctx, info))
            .and_then(|parent| super::resolve_reflection_class(ctx, &parent))
            .and_then(|(parent, info)| {
                let key = super::php_symbol_key(&member.name);
                let flags = super::reflection_method_member_flags(info, &key)?;
                (!flags.is_private).then(||
                    super::reflection_method_declaring_class_name(info, parent, &key)).flatten()
            })
    } else { None };
    let mut prototype = member.prototype_member.as_deref();
    while let Some(parent) = prototype.and_then(|member| member.prototype_member.as_deref()) {
        prototype = Some(parent);
    }
    reflection_method_to_string(
        &member.name,
        member.declaring_class_name.as_deref(),
        member.flags,
        prototype.and_then(|prototype| prototype.declaring_class_name.as_deref()),
        inherited_class_name,
        overwritten_class_name.as_deref(),
        member
            .declaring_class_name
            .as_deref()
            .is_some_and(reflection_class_like_is_internal),
        member.is_deprecated,
        member.returns_reference,
        &member.parameters,
        member.type_metadata.as_ref(),
    )
}

/// Internal deprecation precedes the module suffix in both callable headers.
#[test]
fn test_reflection_dump_internal_deprecated_headers() {
    let function = reflection_function_to_string("xml_set_object", true, true, false, &[], None);
    assert!(function.starts_with("Function [ <internal, deprecated:xml> function xml_set_object ]"));
    let method = reflection_method_to_string(
        "__wakeup", Some("DateTime"), ReflectionMemberFlags::default(),
        Some("DateTimeInterface"), None, None, true, true, false, &[], None,
    );
    assert!(method.starts_with("Method [ <internal, deprecated:date, prototype DateTimeInterface>"));
    assert_eq!(reflection_origin_label(false, true, None), "user, deprecated");
    assert_eq!(reflection_origin_label(true, true, None), "internal, deprecated");
}
