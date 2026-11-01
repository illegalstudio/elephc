//! Purpose:
//! Carries the compilation's thread-local facts from the compiling thread onto the threads a
//! parallel phase starts, so code running there answers the same questions the same way.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_bodies_in_parallel()` around each codegen worker.
//!
//! Key details:
//! - Several facts about the compile in progress live in `thread_local!`s because they cannot be
//!   threaded through every signature that needs them: the PHP profile and SAPI, the null
//!   representation, the declaration order, the linked extensions, `--strict-php`, the eager
//!   globals. A new thread starts with their DEFAULTS, and nothing says so: a codegen worker
//!   lowering an eval context told the interpreter it ran under the CLI SAPI, and Symfony's 404
//!   page came back as a raw exception dump.
//! - A fact that one pass sets and restores around itself (the lowering guards, the strict-PHP
//!   source mode, the parser's mode) is back at its default before code generation starts on the
//!   compiling thread too, so it is not captured.
//! - A new compilation-wide thread-local belongs here as well, or a parallel phase reads its
//!   default.

use std::collections::HashSet;

/// The compilation-wide thread-local facts of the compiling thread; see the module docs.
pub(crate) struct CompileThreadContext {
    php_version: crate::web_prelude::PhpVersion,
    web_sapi_profile: bool,
    linked_extensions: Vec<String>,
    autoload_rule_count: usize,
    declared_classes: Vec<String>,
    declared_interfaces: Vec<String>,
    declared_traits: Vec<String>,
    null_repr: crate::codegen_support::sentinels::NullRepr,
    eager_global_functions: HashSet<String>,
    front_end_web: bool,
    strict_php: bool,
    compiling_for_web: bool,
    entry_script: String,
}

impl CompileThreadContext {
    /// Reads every fact from the current thread.
    pub(crate) fn capture() -> Self {
        use crate::codegen_support::{compilation_context, declaration_order, sentinels};
        Self {
            php_version: compilation_context::compile_php_version(),
            web_sapi_profile: compilation_context::compile_is_web_sapi(),
            linked_extensions: compilation_context::linked_extensions(),
            autoload_rule_count: compilation_context::autoload_rule_count(),
            declared_classes: declaration_order::declared_class_names(),
            declared_interfaces: declaration_order::declared_interface_names(),
            declared_traits: declaration_order::declared_trait_names(),
            null_repr: sentinels::null_repr(),
            eager_global_functions: crate::eager_globals::snapshot(),
            front_end_web: crate::sapi::is_web(),
            strict_php: crate::strict_php::is_requested(),
            compiling_for_web: crate::superglobals::compiling_for_web(),
            entry_script: crate::superglobals::entry_script(),
        }
    }

    /// Makes the current thread answer every fact as the captured thread did.
    pub(crate) fn install(&self) {
        use crate::codegen_support::{compilation_context, declaration_order, sentinels};
        compilation_context::set_compile_profile(self.php_version, self.web_sapi_profile);
        compilation_context::set_linked_extensions(self.linked_extensions.clone());
        compilation_context::set_autoload_rule_count(self.autoload_rule_count);
        declaration_order::set_declared_name_order(
            self.declared_classes.clone(),
            self.declared_interfaces.clone(),
            self.declared_traits.clone(),
        );
        sentinels::set_null_repr(self.null_repr);
        crate::eager_globals::set(self.eager_global_functions.clone());
        crate::sapi::set_web(self.front_end_web);
        crate::strict_php::set_enabled(self.strict_php);
        crate::superglobals::set_compiling_for_web(self.compiling_for_web);
        crate::superglobals::set_entry_script(&self.entry_script);
    }
}

#[cfg(test)]
mod tests {
    use super::CompileThreadContext;
    use crate::codegen_support::{compilation_context, sentinels};

    /// A thread that installs the context answers like the compiling thread, not like a default.
    #[test]
    fn a_new_thread_sees_the_compiling_threads_facts() {
        compilation_context::set_compile_profile(crate::web_prelude::PhpVersion::Php84, true);
        sentinels::set_null_repr(sentinels::NullRepr::Sentinel);
        crate::superglobals::set_compiling_for_web(true);
        crate::strict_php::set_enabled(true);
        let context = CompileThreadContext::capture();
        let seen = std::thread::spawn(move || {
            let before = compilation_context::compile_is_web_sapi();
            context.install();
            (
                before,
                compilation_context::compile_is_web_sapi(),
                compilation_context::compile_php_version(),
                sentinels::null_repr(),
                crate::superglobals::compiling_for_web(),
                crate::strict_php::is_requested(),
            )
        })
        .join()
        .expect("thread");
        assert!(!seen.0, "a fresh thread starts from the CLI default");
        assert!(seen.1);
        assert_eq!(seen.2, crate::web_prelude::PhpVersion::Php84);
        assert_eq!(seen.3, sentinels::NullRepr::Sentinel);
        assert!(seen.4);
        assert!(seen.5);
    }
}
