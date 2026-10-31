//! Purpose:
//! Determines whether a web program can observe its process-environment superglobal.
//!
//! Called from:
//! - `super::inject_if_web_on_compiler_stack` and `super::program_observes_environment`.
//!
//! Key details:
//! - Scans all declarations, including callbacks and included code, without assuming reachability.
//! - Eval and variable-table inspection retain the complete per-request environment copy.
//! - The pipeline also checks the autoload probe, since class files arrive after web injection.
//! - Computed or escaped call targets retain population; simple literal and closure calls can omit it.

use crate::prelude_prune::usage::Usage;

/// Retains population for direct storage access and indirect variable-table observers.
/// `getenv()` reads the process environment independently and needs no PHP array copy.
pub(super) fn is_observable(usage: &Usage) -> bool {
    ["_ENV", "GLOBALS"].iter().any(|name| {
        usage.variables.contains(*name) || usage.storage_variables.contains(*name)
    }) || ["eval", "get_defined_vars", "compact"].iter().any(|name| usage.references(name))
        || usage.callable_names.has_unknown_target()
        || ["eval", "get_defined_vars"].iter().any(|name| usage.literals.contains(*name))
}

#[cfg(test)]
mod tests {
    use crate::parser::ast::Program;
    use crate::prelude_prune::usage;
    use crate::web_prelude::{inject_if_web, PhpVersion};

    /// Injects the actual web bootstrap and reports whether it enumerates the process environment.
    fn populates_environment(source: &str) -> bool {
        let tokens = crate::lexer::tokenize(source).expect("fixture tokenizes");
        let program: Program = crate::parser::parse(&tokens).expect("fixture parses");
        let injected = inject_if_web(
            program,
            true,
            PhpVersion::Php85,
            &[],
            &mut Default::default(),
            None,
        );
        usage::collect(&injected).references("elephc_web_env_count")
    }

    /// Environment-independent handlers avoid enumeration even with dynamic callbacks or probes.
    #[test]
    fn web_environment_population_skips_unobserved_values() {
        for source in [
            "<?php echo 'ok';",
            "<?php echo getenv('PATH');",
            "<?php echo $_SERVER['REQUEST_METHOD'];",
            "<?php $_env = []; echo count($_env);",
            "<?php $fn = 'strlen'; echo $fn('ok');",
            "<?php $fn = 'strlen'; echo $fn('compact');",
            "<?php $fn = fn() => 'ok'; echo $fn();",
            "<?php echo 'ok' |> strlen(...);",
            "<?php $fn = 'strlen'; echo function_exists($fn);",
        ] {
            assert!(!populates_environment(source), "{source}");
        }
    }

    /// Reads, named storage, callbacks, hooks, and indirect observers retain eager population.
    #[test]
    fn web_environment_population_keeps_observable_values() {
        for source in [
            "<?php echo $_ENV['PATH'];",
            "<?php function env_value() { return $_ENV['PATH']; } echo env_value();",
            "<?php $fn = fn() => $_ENV['PATH']; echo $fn();",
            "<?php class Config { public string $path { get => $_ENV['PATH']; } }",
            "<?php echo $GLOBALS['_ENV']['PATH'];",
            "<?php $_ENV['local'] = 'value';",
            "<?php $_ENV[] = 'value';",
            "<?php $values = []; $_ENV =& $values;",
            "<?php function inspect_env() { global $_ENV; }",
            "<?php $fn = function () use (&$_ENV) {};",
            "<?php eval('echo 1;');",
            "<?php eval($argv[1]);",
            "<?php $fn = 'eval'; $fn($code);",
            "<?php $fn = 'ev' . 'al'; $fn($code);",
            "<?php $prefix = 'ev'; $fn = $prefix . 'al'; $fn($code);",
            "<?php $fn = 'evaX'; ($fn[3] = 'l'); $fn($code);",
            "<?php $fn = 'evaX'; $ignored = $fn[3] = 'l'; $fn($code);",
            "<?php function invoke(string $fn, string $code) { $fn($code); }",
            "<?php $fn = 'strlen'; replace($fn); $fn($code);",
            "<?php $fn = 'strlen'; $fn |> replace(...); $fn($code);",
            "<?php $fn = 'get_defined_vars'; $fn();",
            "<?php $code |> 'eval';",
            "<?php echo count(get_defined_vars());",
            "<?php use function get_defined_vars as vars; echo count(vars());",
            "<?php echo count(compact('_ENV'));",
        ] {
            assert!(populates_environment(source), "{source}");
        }
    }
}
