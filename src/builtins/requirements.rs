//! Purpose:
//! Resolves fixed and source-dependent runtime/linker requirements for builtin calls.
//!
//! Called from:
//! - `crate::builtins::semantics::BuiltinSemantics` and the builtin checker dispatcher.
//!
//! Key details:
//! - Resolvers inspect normalized source expressions without mutating checker state.
//! - Platform-specific libraries remain tagged until the checker applies the target policy.

use crate::parser::ast::{Expr, ExprKind};

/// Explicit runtime or linker requirement declared by builtin semantics.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinRequirement {
    /// Link a bridge/static library on demand.
    Bridge(&'static str),
    /// Link a bridge only so its PHP extension is reported as loaded.
    ///
    /// The call itself may lower natively, so this alone never enables the bridge's
    /// runtime lifecycle; EIR feature discovery adds that when a bridge call survives.
    BridgeIdentity(&'static str),
    /// Link a target-neutral system library on demand.
    SystemLibrary(&'static str),
    /// Link a macOS-only system library while Linux resolves the API from libc.
    MacOsLibrary(&'static str),
    /// Enable a named runtime feature collected from the final EIR module.
    RuntimeFeature(&'static str),
}

/// Source-level inputs used to resolve conditional runtime and linker requirements.
pub struct BuiltinRequirementInput<'a> {
    /// Source-order argument expressions after common call normalization.
    pub args: &'a [Expr],
}

/// Resolver for argument-dependent runtime and linker requirements.
pub type RequirementsFn = for<'a> fn(&BuiltinRequirementInput<'a>) -> Vec<BuiltinRequirement>;

/// Declares fixed or source-dependent runtime and linker requirements.
#[derive(Clone, Copy)]
pub enum BuiltinRequirements {
    /// Requirements that apply to every call of the builtin.
    Static(&'static [BuiltinRequirement]),
    /// Requirements selected from normalized source arguments.
    Shared(RequirementsFn),
}

/// Resolves libraries needed by `file_get_contents()` from its filename expression.
pub fn file_get_contents_requirements(
    input: &BuiltinRequirementInput<'_>,
) -> Vec<BuiltinRequirement> {
    match input.args.first().map(|arg| &arg.kind) {
        Some(ExprKind::StringLiteral(url))
            if url.starts_with("https://") || url.starts_with("ftps://") =>
        {
            vec![BuiltinRequirement::Bridge("elephc_tls")]
        }
        Some(ExprKind::StringLiteral(_)) => Vec::new(),
        _ => vec![
            BuiltinRequirement::Bridge("elephc_tls"),
            BuiltinRequirement::Bridge("elephc_phar"),
            BuiltinRequirement::SystemLibrary("z"),
            BuiltinRequirement::SystemLibrary("bz2"),
        ],
    }
}

/// Resolves PHAR libraries needed by `file_put_contents()` from its filename expression.
pub fn file_put_contents_requirements(
    input: &BuiltinRequirementInput<'_>,
) -> Vec<BuiltinRequirement> {
    match input.args.first().map(|arg| &arg.kind) {
        Some(ExprKind::StringLiteral(url)) if url.starts_with("phar://") => vec![
            BuiltinRequirement::Bridge("elephc_phar"),
            BuiltinRequirement::Bridge("elephc_crypto"),
        ],
        Some(ExprKind::StringLiteral(_)) => Vec::new(),
        _ => vec![BuiltinRequirement::Bridge("elephc_phar")],
    }
}

/// Resolves transport, compression, and PHAR libraries needed by `fopen()`.
pub fn fopen_requirements(input: &BuiltinRequirementInput<'_>) -> Vec<BuiltinRequirement> {
    let Some(ExprKind::StringLiteral(filename)) = input.args.first().map(|arg| &arg.kind) else {
        return vec![
            BuiltinRequirement::Bridge("elephc_phar"),
            BuiltinRequirement::SystemLibrary("z"),
            BuiltinRequirement::SystemLibrary("bz2"),
        ];
    };
    let mut requirements = Vec::new();
    if filename.starts_with("https://") || filename.starts_with("ftps://") {
        requirements.push(BuiltinRequirement::Bridge("elephc_tls"));
    }
    if filename.starts_with("compress.zlib://") {
        requirements.push(BuiltinRequirement::SystemLibrary("z"));
    }
    if filename.starts_with("compress.bzip2://") {
        requirements.push(BuiltinRequirement::SystemLibrary("bz2"));
    }
    let write_mode = matches!(
        input.args.get(1).map(|arg| &arg.kind),
        Some(ExprKind::StringLiteral(mode))
            if matches!(mode.as_bytes().first(), Some(b'w') | Some(b'a') | Some(b'c') | Some(b'x'))
    );
    if filename.starts_with("phar://") && write_mode {
        requirements.push(BuiltinRequirement::Bridge("elephc_phar"));
        requirements.push(BuiltinRequirement::Bridge("elephc_crypto"));
    }
    requirements
}

/// Resolves the PHAR bridge needed by `unlink()` from its filename expression.
pub fn unlink_requirements(input: &BuiltinRequirementInput<'_>) -> Vec<BuiltinRequirement> {
    match input.args.first().map(|arg| &arg.kind) {
        Some(ExprKind::StringLiteral(filename)) if filename.starts_with("phar://") => {
            vec![BuiltinRequirement::Bridge("elephc_phar")]
        }
        Some(ExprKind::StringLiteral(_)) => Vec::new(),
        _ => vec![BuiltinRequirement::Bridge("elephc_phar")],
    }
}

/// Resolves how `mb_check_encoding()` needs the mbstring bridge from its encoding literal.
///
/// A positional subject checked against a literal `UTF-8`/`UTF8` can lower to the native
/// validator, so the bridge is required only for extension identity. Whether the call really
/// stays native also depends on the subject's type, which EIR feature discovery decides.
pub fn mb_check_encoding_requirements(
    input: &BuiltinRequirementInput<'_>,
) -> Vec<BuiltinRequirement> {
    let native_shape = matches!(input.args, [subject, encoding]
        if !matches!(subject.kind, ExprKind::Spread(_) | ExprKind::NamedArg { .. })
            && matches!(&encoding.kind, ExprKind::StringLiteral(name)
                if name.eq_ignore_ascii_case("UTF-8") || name.eq_ignore_ascii_case("UTF8")));
    if native_shape {
        vec![BuiltinRequirement::BridgeIdentity("elephc_mbstring")]
    } else {
        vec![BuiltinRequirement::Bridge("elephc_mbstring")]
    }
}

/// Resolves libraries needed by a literal stream filter name.
pub fn stream_filter_requirements(
    input: &BuiltinRequirementInput<'_>,
) -> Vec<BuiltinRequirement> {
    let Some(ExprKind::StringLiteral(filter)) = input.args.get(1).map(|arg| &arg.kind) else {
        return Vec::new();
    };
    if matches!(filter.as_str(), "zlib.deflate" | "zlib.inflate") {
        vec![BuiltinRequirement::SystemLibrary("z")]
    } else if filter.starts_with("convert.iconv.") {
        vec![BuiltinRequirement::MacOsLibrary("iconv")]
    } else if matches!(filter.as_str(), "bzip2.compress" | "bzip2.decompress") {
        vec![BuiltinRequirement::SystemLibrary("bz2")]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Builds one requirement resolver input with a dummy call-site span.
    fn input(args: &[Expr]) -> BuiltinRequirementInput<'_> {
        BuiltinRequirementInput {
            args,
        }
    }

    /// Keeps the mbstring runtime only for check shapes that cannot lower natively.
    #[test]
    fn mb_check_encoding_requirements_separate_identity_from_runtime() {
        let string = |value: &str| Expr::new(ExprKind::StringLiteral(value.to_string()), Span::dummy());
        let variable = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), Span::dummy());
        let identity = vec![BuiltinRequirement::BridgeIdentity("elephc_mbstring")];
        let runtime = vec![BuiltinRequirement::Bridge("elephc_mbstring")];
        assert_eq!(mb_check_encoding_requirements(&input(&[variable("s"), string("UTF-8")])), identity);
        assert_eq!(mb_check_encoding_requirements(&input(&[variable("s"), string("utf8")])), identity);
        assert_eq!(mb_check_encoding_requirements(&input(&[variable("s"), string("ISO-8859-1")])), runtime);
        assert_eq!(mb_check_encoding_requirements(&input(&[variable("s"), variable("e")])), runtime);
        assert_eq!(mb_check_encoding_requirements(&input(&[variable("s")])), runtime);
        assert_eq!(mb_check_encoding_requirements(&input(&[])), runtime);
    }

    /// Verifies literal and dynamic file reads select only the libraries they can reach.
    #[test]
    fn file_read_requirements_preserve_literal_precision() {
        let https = [Expr::new(
            ExprKind::StringLiteral("https://example.test".to_string()),
            Span::dummy(),
        )];
        assert_eq!(
            file_get_contents_requirements(&input(&https)),
            vec![BuiltinRequirement::Bridge("elephc_tls")]
        );
        let local = [Expr::new(
            ExprKind::StringLiteral("local.txt".to_string()),
            Span::dummy(),
        )];
        assert!(file_get_contents_requirements(&input(&local)).is_empty());
        let dynamic = [Expr::new(
            ExprKind::Variable("path".to_string()),
            Span::dummy(),
        )];
        assert_eq!(file_get_contents_requirements(&input(&dynamic)).len(), 4);
    }

    /// Verifies PHAR writes and known stream filters select their exact bridge libraries.
    #[test]
    fn phar_write_and_filter_requirements_are_argument_driven() {
        let phar_write = [
            Expr::new(
                ExprKind::StringLiteral("phar://archive.phar/file".to_string()),
                Span::dummy(),
            ),
            Expr::new(ExprKind::StringLiteral("wb".to_string()), Span::dummy()),
        ];
        assert_eq!(
            fopen_requirements(&input(&phar_write)),
            vec![
                BuiltinRequirement::Bridge("elephc_phar"),
                BuiltinRequirement::Bridge("elephc_crypto"),
            ]
        );
        let zlib_filter = [
            Expr::new(ExprKind::Null, Span::dummy()),
            Expr::new(
                ExprKind::StringLiteral("zlib.inflate".to_string()),
                Span::dummy(),
            ),
        ];
        assert_eq!(
            stream_filter_requirements(&input(&zlib_filter)),
            vec![BuiltinRequirement::SystemLibrary("z")]
        );
    }
}
