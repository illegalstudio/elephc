//! Purpose:
//! Renders the deterministic C header emitted beside an Elephc cdylib.
//!
//! Called from:
//! - `crate::pipeline::backend` after a cdylib links successfully.
//!
//! Key details:
//! - Scalar export prototypes preserve the v1 ABI exactly.
//! - The exact `string -> string` surface uses status plus owned output parameters.
//! - Export ordering and include-guard normalization are deterministic.

use std::fmt::Write as _;

use crate::types::PhpType;

use super::{is_string_roundtrip_signature, ExportedFunction};

/// Public ABI version returned by `elephc_abi_version()` and written to generated headers.
pub const ELEPHC_ABI_VERSION: u32 = 2;

/// Renders one complete C header for `library_stem` and the resolved exports.
pub fn render_c_header(library_stem: &str, exports: &[&ExportedFunction]) -> String {
    let guard = include_guard(library_stem);
    let mut sorted = exports.to_vec();
    sorted.sort_by(|left, right| left.c_name.cmp(&right.c_name));

    let mut out = String::new();
    writeln!(out, "#ifndef {guard}").unwrap();
    writeln!(out, "#define {guard}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#include <stddef.h>").unwrap();
    writeln!(out, "#include <stdint.h>").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#define ELEPHC_ABI_VERSION UINT32_C({ELEPHC_ABI_VERSION})").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#define ELEPHC_STATUS_OK INT32_C(0)").unwrap();
    writeln!(out, "#define ELEPHC_STATUS_INVALID_ARGUMENT INT32_C(1)").unwrap();
    writeln!(out, "#define ELEPHC_STATUS_PHP_EXCEPTION INT32_C(2)").unwrap();
    writeln!(out, "#define ELEPHC_STATUS_ALLOCATION_FAILURE INT32_C(3)").unwrap();
    writeln!(out, "#define ELEPHC_STATUS_RUNTIME_FAILURE INT32_C(4)").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#ifdef __cplusplus").unwrap();
    writeln!(out, "extern \"C\" {{").unwrap();
    writeln!(out, "#endif").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "uint32_t elephc_abi_version(void);").unwrap();
    writeln!(out, "int32_t elephc_init(void);").unwrap();
    writeln!(out, "void elephc_shutdown(void);").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/* Borrowed, NUL-terminated diagnostic; NULL means no recorded error.").unwrap();
    writeln!(out, " * A recorded empty message is a non-NULL pointer to an empty string.").unwrap();
    writeln!(out, " * Valid until the next exported call or lifecycle reset; never free. */").unwrap();
    writeln!(out, "const char *elephc_last_error(void);").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/* Releases an owned export result. Passing NULL is safe. */").unwrap();
    writeln!(out, "void elephc_free(void *ptr);").unwrap();

    for export in sorted {
        writeln!(out).unwrap();
        render_export(&mut out, export);
    }

    writeln!(out).unwrap();
    writeln!(out, "#ifdef __cplusplus").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out, "#endif").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#endif /* {guard} */").unwrap();
    out
}

/// Renders one public export prototype using its resolved PHP signature.
fn render_export(out: &mut String, export: &ExportedFunction) {
    if is_string_roundtrip_signature(&export.sig) {
        let param = c_identifier(&export.sig.params[0].0);
        writeln!(out, "/* On success, *output_ptr is caller-owned and must be released with").unwrap();
        writeln!(out, " * elephc_free(); output_len is authoritative and excludes the optional").unwrap();
        writeln!(out, " * trailing NUL byte. Failure leaves both outputs NULL/zero. */").unwrap();
        writeln!(
            out,
            "int32_t {}(const char *{}_ptr, size_t {}_len, char **output_ptr, size_t *output_len);",
            export.c_name, param, param
        )
        .unwrap();
        return;
    }

    let return_type = c_scalar_return_type(&export.sig.return_type);
    write!(out, "{return_type} {}(", export.c_name).unwrap();
    let mut parameters = Vec::new();
    for (name, php_type) in &export.sig.params {
        let name = c_identifier(name);
        match php_type {
            PhpType::Str => {
                parameters.push(format!("const char *{name}_ptr"));
                parameters.push(format!("size_t {name}_len"));
            }
            PhpType::Float => parameters.push(format!("double {name}")),
            PhpType::Int | PhpType::Bool => parameters.push(format!("int64_t {name}")),
            other => unreachable!("validated export parameter reached header rendering: {other:?}"),
        }
    }
    if parameters.is_empty() {
        write!(out, "void").unwrap();
    } else {
        write!(out, "{}", parameters.join(", ")).unwrap();
    }
    writeln!(out, ");").unwrap();
}

/// Maps a validated v1 scalar return type to its stable C spelling.
fn c_scalar_return_type(php_type: &PhpType) -> &'static str {
    match php_type {
        PhpType::Void => "void",
        PhpType::Float => "double",
        PhpType::Int | PhpType::Bool => "int64_t",
        other => unreachable!("validated export return reached scalar header rendering: {other:?}"),
    }
}

/// Builds a deterministic include guard from the conventional library stem.
fn include_guard(library_stem: &str) -> String {
    let normalized = library_stem
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_uppercase() } else { '_' })
        .collect::<String>();
    format!("ELEPHC_{normalized}_H")
}

/// Normalizes a PHP parameter name into a conservative C identifier.
fn c_identifier(name: &str) -> String {
    let mut normalized = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
        .collect::<String>();
    if normalized.is_empty() {
        normalized.push_str("arg");
    }
    if normalized.as_bytes()[0].is_ascii_digit() {
        normalized.insert(0, '_');
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use crate::types::FunctionSig;

    /// Builds a resolved export fixture for deterministic header tests.
    fn export(name: &str, params: Vec<(&str, PhpType)>, return_type: PhpType) -> ExportedFunction {
        let len = params.len();
        ExportedFunction {
            name: name.to_string(),
            c_name: super::super::public_c_name(name),
            sig: FunctionSig {
                params: params.into_iter().map(|(name, ty)| (name.to_string(), ty)).collect(),
                param_type_exprs: vec![None; len],
                param_attributes: vec![Vec::new(); len],
                defaults: vec![None; len],
                return_type,
                declared_return: true,
                by_ref_return: false,
                ref_params: vec![false; len],
                declared_params: vec![true; len],
                variadic: None,
                deprecation: None,
            },
            span: Span::dummy(),
        }
    }

    /// Renders every required ABI declaration and owned-string lifetime comment.
    #[test]
    fn renders_complete_binary_safe_header() {
        let roundtrip = export("roundtrip", vec![("input", PhpType::Str)], PhpType::Str);
        let add = export(
            "add_i64",
            vec![("a", PhpType::Int), ("b", PhpType::Int)],
            PhpType::Int,
        );
        let header = render_c_header("libroundtrip", &[&roundtrip, &add]);

        assert!(header.contains("#define ELEPHC_ABI_VERSION UINT32_C(2)"));
        assert!(header.contains("#define ELEPHC_STATUS_PHP_EXCEPTION INT32_C(2)"));
        assert!(header.contains("uint32_t elephc_abi_version(void);"));
        assert!(header.contains("int64_t add_i64(int64_t a, int64_t b);"));
        assert!(header.contains("int32_t roundtrip(const char *input_ptr, size_t input_len, char **output_ptr, size_t *output_len);"));
        assert!(header.contains("must be released with"));
        assert!(header.contains("A recorded empty message is a non-NULL pointer"));
    }

    /// Sorts export prototypes by public symbol regardless of map iteration order.
    #[test]
    fn output_is_deterministic_and_sorted() {
        let zed = export("zed", Vec::new(), PhpType::Void);
        let alpha = export("alpha", Vec::new(), PhpType::Int);
        let first = render_c_header("lib-test", &[&zed, &alpha]);
        let second = render_c_header("lib-test", &[&alpha, &zed]);
        assert_eq!(first, second);
        assert!(first.find("alpha(void)").unwrap() < first.find("zed(void)").unwrap());
        assert!(first.starts_with("#ifndef ELEPHC_LIB_TEST_H\n"));
    }

    /// Emits a valid, documented C prototype for a namespaced PHP export.
    #[test]
    fn renders_namespaced_export_with_c_safe_symbol() {
        let export = export(
            "Demo\\roundtrip",
            vec![("input", PhpType::Str)],
            PhpType::Str,
        );
        let header = render_c_header("namespaced", &[&export]);
        assert!(header.contains("int32_t Demo_roundtrip("));
        assert!(!header.contains("Demo\\roundtrip"));
    }
}
