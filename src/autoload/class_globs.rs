//! Purpose:
//! Finds the classes a dependency-injection container registers by globbing a directory, so a
//! closed world compiles them without rooting every class of every dependency.
//!
//! Called from:
//! - `crate::autoload` when it seeds the closed-world roots.
//!
//! Key details:
//! - Symfony's container loaders register a directory of classes from a namespace prefix and a
//!   glob: framework-bundle's `->load('Symfony\Component\Validator\Constraints\\',
//!   $validatorsDir.'/*Validator.php')`, an application's `->load('App\\', '../src/')`. A console
//!   command that rebuilds the container (`lint:container`, `debug:container`) lists that directory
//!   at run time, which no reference in the code shows.
//! - The directory part is often computed (`$validatorsDir`), but the namespace prefix and the
//!   glob's last segment are literals, and under PSR-4 the prefix already names the directory: a
//!   `*Validator.php` tail selects the classes directly under the prefix whose short name ends in
//!   `Validator`; a tail with no file pattern (`'../src/'`, `'../src/*'`) selects every class under
//!   the prefix, recursively. Exclusions are ignored: a superset only compiles a few more classes.

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{BinOp, Expr, ExprKind, Program};

/// One `load(prefix, glob)` registration: a namespace prefix and the glob's last segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassGlob {
    /// Namespace prefix, with its trailing `\` and without a leading one.
    pub prefix: String,
    /// The class short-name pattern (`*Validator`), or `None` for every class under the prefix.
    pub short_name: Option<String>,
}

impl ClassGlob {
    /// Whether the fully-qualified class `name` is one this registration lists.
    pub fn matches(&self, name: &str) -> bool {
        let name = name.trim_start_matches('\\');
        let Some(rest) = strip_prefix_ignore_case(name, &self.prefix) else {
            return false;
        };
        match &self.short_name {
            None => true,
            Some(pattern) => !rest.contains('\\') && glob_matches(pattern, rest),
        }
    }
}

/// The `load()` / `registerClasses()` registrations of `program` whose prefix and glob tail are
/// literals.
pub(crate) fn registrations(program: &Program) -> Vec<ClassGlob> {
    let mut found = Vec::new();
    walk_nodes(program, &mut |node| {
        let Node::Expr(expr) = node else { return };
        let ExprKind::MethodCall { method, args, .. } = &expr.kind else { return };
        let (prefix_arg, resource_arg) = match method.to_ascii_lowercase().as_str() {
            "load" => (args.first(), args.get(1)),
            // `FileLoader::registerClasses(Definition $prototype, string $namespace, string $resource)`.
            "registerclasses" => (args.get(1), args.get(2)),
            _ => return,
        };
        let (Some(prefix_arg), Some(resource_arg)) = (prefix_arg, resource_arg) else { return };
        let ExprKind::StringLiteral(prefix) = &positional(prefix_arg).kind else { return };
        if !prefix.ends_with('\\') || prefix.trim_matches('\\').is_empty() {
            return;
        }
        let Some(tail) = literal_tail(positional(resource_arg)) else { return };
        let segment = tail.rsplit('/').next().unwrap_or("");
        let short_name = segment
            .strip_suffix(".php")
            .filter(|pattern| !pattern.is_empty() && *pattern != "*")
            .map(str::to_string);
        let glob = ClassGlob { prefix: prefix.trim_start_matches('\\').to_string(), short_name };
        if !found.contains(&glob) {
            found.push(glob);
        }
    });
    found
}

/// The value of a named argument, or the argument itself.
fn positional(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}

/// The literal text that ends a resource expression: the string itself, or the right-most literal
/// of a concatenation (`$dir.'/*Validator.php'`). It must look like a glob or a directory.
fn literal_tail(expr: &Expr) -> Option<&str> {
    let text = match &expr.kind {
        ExprKind::StringLiteral(text) => text.as_str(),
        ExprKind::BinaryOp { op: BinOp::Concat, right, .. } => return literal_tail(right),
        _ => return None,
    };
    (text.contains('*') || text.ends_with('/')).then_some(text)
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// Whether `text` matches a glob whose only wildcard is `*`.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text[first.len()..].ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registrations_of(source: &str) -> Vec<ClassGlob> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        registrations(&program)
    }

    #[test]
    fn a_computed_directory_keeps_its_literal_prefix_and_pattern() {
        let globs = registrations_of(
            "<?php\n$services->load('Symfony\\\\Component\\\\Validator\\\\Constraints\\\\', $validatorsDir.'/*Validator.php')->abstract();\n",
        );
        assert_eq!(
            globs,
            vec![ClassGlob {
                prefix: "Symfony\\Component\\Validator\\Constraints\\".to_string(),
                short_name: Some("*Validator".to_string()),
            }]
        );
        let glob = &globs[0];
        assert!(glob.matches("Symfony\\Component\\Validator\\Constraints\\NotBlankValidator"));
        assert!(!glob.matches("Symfony\\Component\\Validator\\Constraints\\NotBlank"));
        assert!(!glob.matches("Symfony\\Component\\Validator\\Constraints\\Deep\\XValidator"));
        assert!(!glob.matches("Symfony\\Component\\Form\\FormValidator"));
    }

    #[test]
    fn a_directory_resource_lists_every_class_under_the_prefix() {
        let globs = registrations_of("<?php\n$loader->registerClasses($definition, 'App\\\\', '../src/*');\n");
        assert_eq!(globs, vec![ClassGlob { prefix: "App\\".to_string(), short_name: None }]);
        assert!(globs[0].matches("App\\Controller\\Deep\\Home"));
    }

    #[test]
    fn a_load_that_is_not_a_class_registration_is_ignored() {
        assert!(registrations_of("<?php\n$twig->load('page.html.twig');\n$x->load('App\\\\', $computed);\n").is_empty());
    }
}
