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
//!   bridge omits the same line, but still differs in three places this renderer gets right: the
//!   prototype marker, the empty body, and the union order (#1117).
//! - Union members print in PHP's order, not the declared one — see `reflection_union_member_rank`,
//!   which the property dump shares.
//! - Internal function origins come from the shared builtin contract, and internal method origins
//!   come from the declaring class contract, printed under the module's registered name (`SPL`).

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
    is_internal: bool,
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

    let module = declaring_class_name
        .and_then(elephc_builtin_contract::lookup_class)
        .map(|class| class.module);
    let mut origin_parts = vec![reflection_origin_label(is_internal, module)];
    if let Some(class_name) = inherited_class_name {
        origin_parts.push(format!("inherits {class_name}"));
    }
    if let Some(class_name) = prototype_class_name {
        origin_parts.push(format!("prototype {class_name}"));
    }
    // PHP marks constructors specially in ReflectionMethod dumps; destructors have no dtor tag.
    if name.eq_ignore_ascii_case("__construct") {
        origin_parts.push("ctor".to_string());
    }
    let origin = format!("<{}>", origin_parts.join(", "));
    let display_name = if returns_reference {
        format!("&{name}")
    } else {
        name.to_string()
    };
    let header = format!("Method [ {} {} {} ]", origin, parts.join(" "), display_name);
    reflection_callable_body(&header, parameters, return_type)
}

/// Renders ReflectionFunction::__toString() for one reflected function.
pub(super) fn reflection_function_to_string(
    name: &str,
    is_internal: bool,
    returns_reference: bool,
    parameters: &[ReflectionParameterMember],
    return_type: Option<&ReflectionParameterTypeMetadata>,
) -> String {
    let normalized_name = name.trim_start_matches(char::from(92));
    let display_name = if returns_reference {
        format!("&{normalized_name}")
    } else {
        normalized_name.to_string()
    };
    let module = elephc_builtin_contract::lookup(name).map(|contract| contract.module);
    let header = format!(
        "Function [ <{}> function {} ]",
        reflection_origin_label(is_internal, module),
        display_name
    );
    reflection_callable_body(&header, parameters, return_type)
}

/// Returns PHP's callable origin label, including the owning module when it is known.
fn reflection_origin_label(is_internal: bool, module: Option<PhpModule>) -> String {
    if !is_internal {
        return "user".to_string();
    }
    match module {
        Some(module) => format!("internal:{}", module.registered_name()),
        None => "internal".to_string(),
    }
}

/// Appends PHP's parameter/return block to a rendered function or method header.
///
/// A callable with no parameters AND no declared return type prints an EMPTY body — PHP omits the
/// `- Parameters [0]` block entirely there, and prints it as soon as either half has something to
/// say. Measured on 8.5.10 with `function f() {}` against `function f(): void {}`.
///
/// The blank line before `- Parameters` belongs to that block, not to the omitted `@@` line: an
/// internal callable has no `@@` line and PHP still prints it (`ReflectionFunction('pi')`).
fn reflection_callable_body(
    header: &str,
    parameters: &[ReflectionParameterMember],
    return_type: Option<&ReflectionParameterTypeMetadata>,
) -> String {
    // Every callable carries a synthesized `mixed ...$__elephc_func_args` so `func_get_args()`
    // can read surplus positional arguments. It is an ABI slot, not a PHP parameter: PHP never
    // prints it, `getParameters()` already omits it, and printing it here made the compiled dump
    // disagree with the eval one for the same method. `is_generated_local_name` is the predicate
    // the rest of the compiler uses to keep these out of `get_defined_vars()` and eval scopes.
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
        rendered.push_str("  - Return [ ");
        rendered.push_str(&reflection_type_metadata_to_string(return_type));
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
        rendered.push_str(&reflection_type_metadata_to_string(type_metadata));
        rendered.push(' ');
    }
    if parameter.is_passed_by_reference {
        rendered.push('&');
    }
    if parameter.is_variadic {
        rendered.push_str("...");
    }
    rendered.push('$');
    rendered.push_str(&parameter.name);
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
/// A default written as a constant prints the CONSTANT's name, not its value — `= LIMIT` and
/// `= self::CEIL`, measured on 8.5.10 — which is why the constant name is consulted first.
fn reflection_parameter_default_to_string(parameter: &ReflectionParameterMember) -> Option<String> {
    if let Some(constant_name) = parameter.default_value_constant_name.as_ref() {
        return Some(constant_name.clone());
    }
    parameter
        .default_value
        .as_ref()
        .map(reflection_dump_default_value)
}

/// Renders one default value, including the array and object forms PHP prints in a dump.
fn reflection_dump_default_value(default: &ReflectionParameterDefaultValue) -> String {
    match default {
        ReflectionParameterDefaultValue::Int(value) => value.to_string(),
        ReflectionParameterDefaultValue::Bool(value) => value.to_string(),
        ReflectionParameterDefaultValue::Float(value) => reflection_dump_float(*value),
        // PHP does not escape the quote it wraps the value in, so `it's` prints as `'it's'`.
        ReflectionParameterDefaultValue::Str(value) => format!("'{value}'"),
        ReflectionParameterDefaultValue::Null => String::from("NULL"),
        // PHP exports the written AST here, so the argument list stays and the class name is
        // fully qualified: `new \\Foo(1, 'a')`, and `new \\Foo()` when nothing was written —
        // the constructor defaults `args` carries for `getDefaultValue()` are not part of the
        // source, so only the written ones are printed.
        ReflectionParameterDefaultValue::Object {
            class_name,
            args,
            written_args,
        } => {
            let rendered = args
                .iter()
                .take(*written_args)
                .map(reflection_dump_default_value)
                .collect::<Vec<_>>();
            format!(
                "new \\{}({})",
                class_name.trim_start_matches('\\'),
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
                        ReflectionDefaultArrayKey::Str(value) => format!("'{value}'"),
                    };
                    format!("{key} => {}", reflection_dump_default_value(&entry.value))
                })
                .collect::<Vec<_>>();
            format!("[{}]", rendered.join(", "))
        }
    }
}

/// Renders a float default the way PHP's dump does.
///
/// Three things Rust's own `to_string` does not do. PHP spells the non-finite values `INF`,
/// `-INF` and `NAN` where Rust writes `inf`, `-inf` and `NaN`; it keeps the decimal point on an
/// integral value — `1.0` stays `1.0` — and it switches to an exponent form outside a narrow
/// decimal range, where Rust expands every digit. Measured on 8.5.10: `1e100` prints `1.0E+100`,
/// `1e-5` prints `1.0E-5` and `1e15` prints `1.0E+15`, while `1.5` and `0.1` stay decimal, and
/// `function f(float $x = INF, float $y = -INF, float $z = NAN) {}` dumps those three names.
fn reflection_dump_float(value: f64) -> String {
    if value.is_nan() {
        return "NAN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-INF" } else { "INF" }.to_string();
    }
    let magnitude = value.abs();
    let exponential = magnitude != 0.0 && !(1e-4..1e15).contains(&magnitude);
    if !exponential {
        let rendered = value.to_string();
        return if rendered.contains('.') {
            rendered
        } else {
            format!("{rendered}.0")
        };
    }
    // `{:E}` gives `1E100`; PHP writes the mantissa with a decimal point and the exponent with a
    // sign, so `1E100` becomes `1.0E+100` and `1E-5` becomes `1.0E-5`.
    let rendered = format!("{:E}", value);
    let (mantissa, exponent) = match rendered.split_once('E') {
        Some(parts) => parts,
        None => return rendered,
    };
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    if exponent.starts_with('-') {
        format!("{mantissa}E{exponent}")
    } else {
        format!("{mantissa}E+{exponent}")
    }
}

/// Renders the `ReflectionMethod::__toString()` answer for one listed member.
pub(super) fn reflection_listed_method_to_string(member: &ReflectionListedMember) -> String {
    let inherited_class_name = if member.flags.is_inherited {
        member.declaring_class_name.as_deref()
    } else {
        None
    };
    reflection_method_to_string(
        &member.name,
        member.declaring_class_name.as_deref(),
        member.flags,
        member
            .prototype_member
            .as_deref()
            .and_then(|prototype| prototype.declaring_class_name.as_deref()),
        inherited_class_name,
        member
            .declaring_class_name
            .as_deref()
            .is_some_and(reflection_class_like_is_internal),
        member.returns_reference,
        &member.parameters,
        member.type_metadata.as_ref(),
    )
}
