//! Purpose:
//! Indexes immutable PHP function, extern, include-variant and emitted method names.
//!
//! Called from:
//! - FunctionContext name resolution while lowering direct and callable calls.
//!
//! Key details:
//! - Preserves the original first-match function/closure and dispatch-group order.
//! - Wrapper snapshots extend literals but retain the source declaration tables.
//! - One SharedCodegenState owns one index; separate emissions never share names.

use std::collections::{HashMap, HashSet};

use super::SharedCodegenState;
use crate::ir::{function_variants, Function, Module};
use crate::names::php_symbol_key;

/// Position in the two tables searched by the original direct-function lookup.
#[derive(Clone, Copy)]
enum FunctionSlot {
    Function(usize),
    Closure(usize),
}

/// Immutable callable names, include representatives and emitted method membership.
pub(in crate::codegen) struct ModuleCallableIndex {
    functions: HashMap<String, FunctionSlot>,
    externs: HashSet<String>,
    groups: HashMap<String, (String, usize)>,
    /// Exact emitted class spelling, normalized method key and static/instance kind.
    emitted_methods: HashMap<String, [HashSet<String>; 2]>,
}

/// Applies the same leading-separator and PHP case normalization as the scanner.
fn key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\'))
}

impl ModuleCallableIndex {
    /// Builds the name tables once from finalized source declarations and metadata.
    fn new(module: &Module) -> Self {
        let mut functions = HashMap::new();
        for (index, function) in module.functions.iter().enumerate() {
            functions.entry(key(&function.name)).or_insert(FunctionSlot::Function(index));
        }
        let mut groups = HashMap::new();
        for group in function_variants::collect_dispatch_groups(module) {
            let representative = group.variants.iter().find_map(|variant| {
                match functions.get(&key(variant)) {
                    Some(FunctionSlot::Function(index)) => Some(*index),
                    _ => None,
                }
            });
            if let Some(index) = representative {
                groups.entry(key(&group.name)).or_insert((group.name, index));
            }
        }
        for (index, closure) in module.closures.iter().enumerate() {
            functions.entry(key(&closure.name)).or_insert(FunctionSlot::Closure(index));
        }
        let mut emitted_methods: HashMap<String, [HashSet<String>; 2]> = HashMap::new();
        for function in &module.class_methods {
            if let Some((class_name, method_name)) = function.name.rsplit_once("::") {
                emitted_methods.entry(class_name.to_string()).or_default()
                    [usize::from(function.flags.is_static)].insert(php_symbol_key(method_name));
            }
        }
        Self {
            functions,
            externs: module.extern_decls.iter().map(|entry| key(&entry.name)).collect(),
            groups,
            emitted_methods,
        }
    }

    /// Returns the first concrete function or closure with the requested PHP name.
    pub(in crate::codegen) fn function<'a>(&self, module: &'a Module, name: &str) -> Option<&'a Function> {
        match self.functions.get(&key(name))? {
            FunctionSlot::Function(index) => module.functions.get(*index),
            FunctionSlot::Closure(index) => module.closures.get(*index),
        }
    }

    /// Matches the original emitted-body scanner without broadening class or request keys.
    pub(in crate::codegen) fn has_emitted_method(&self, class_name: &str, method_key: &str, is_static: bool) -> bool {
        self.emitted_methods.get(class_name)
            .is_some_and(|kinds| kinds[usize::from(is_static)].contains(method_key))
    }

    /// Returns whether the module declares an extern with the requested PHP name.
    pub(in crate::codegen) fn has_extern(&self, name: &str) -> bool {
        self.externs.contains(&key(name))
    }

    /// Preserves the public group's spelling while matching PHP names without case.
    pub(in crate::codegen) fn group_name(&self, name: &str) -> Option<String> {
        self.groups.get(&key(name)).map(|(public, _)| public.clone())
    }

    /// Resolves the direct body first, then the first surviving public include variant.
    pub(in crate::codegen) fn callable<'a>(&self, module: &'a Module, name: &str) -> Option<&'a Function> {
        self.function(module, name).or_else(|| {
            self.groups.get(&key(name)).and_then(|(_, index)| module.functions.get(*index))
        })
    }
}

impl SharedCodegenState {
    /// Reuses declarations across every context belonging to this source-body graph.
    pub(in crate::codegen) fn module_callable_index(&self, module: &Module) -> &ModuleCallableIndex {
        self.module_callable_index.get_or_init(|| ModuleCallableIndex::new(module))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::ir::IrType;
    use crate::types::PhpType;

    /// Creates a declaration-only function for metadata resolution tests.
    fn body(name: &str) -> Function {
        Function::new(name.to_string(), IrType::Void, PhpType::Void)
    }

    /// Reproduces the pre-index function and closure search as an independent oracle.
    fn scanned<'a>(module: &'a Module, name: &str) -> Option<&'a Function> {
        let requested = key(name);
        module.functions.iter().chain(module.closures.iter())
            .find(|function| key(&function.name) == requested)
    }

    /// Direct names keep case, leading separators, table priority and duplicate order.
    #[test]
    fn callable_index_matches_function_and_closure_scanner_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = Module::new(Target::parse(target).unwrap());
            module.functions.extend([body("Ns\\Pick"), body("ns\\PICK"), body("Generic<array<string, int>>")]);
            module.closures.extend([body("NS\\PICK"), body("closure_name")]);
            let index = ModuleCallableIndex::new(&module);
            for name in ["ns\\pick", "\\NS\\PICK", "\\\\Ns\\Pick", "generic<array<string, int>>", "closure_name", "CLOSURE_NAME", "missing"] {
                assert_eq!(index.function(&module, name).map(|function| &function.name), scanned(&module, name).map(|function| &function.name), "{target}: {name}");
            }
            assert!(!index.has_extern("missing"));
        }
    }

    /// Canonical variant metadata retains first-group and first-surviving-variant precedence.
    #[test]
    fn callable_index_matches_variant_scanner_and_public_spelling() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        module.functions.extend([body("first"), body("second"), body("shadow")]);
        for label in ["Public:missing,second,first", "PUBLIC:first", "shadow:first", "Generic<array<string, int>>:first,second", "missing:absent", "broken<:first", "ordinary literal"] {
            module.data.intern_string(label);
        }
        let index = ModuleCallableIndex::new(&module);
        for name in ["Public", "\\PUBLIC", "shadow", "Generic<array<string, int>>", "missing", "unknown"] {
            let expected = scanned(&module, name).or_else(|| function_variants::variant_callee_for_group(&module, name));
            assert_eq!(index.callable(&module, name).map(|function| &function.name), expected.map(|function| &function.name));
            let expected_group = function_variants::collect_dispatch_groups(&module).into_iter()
                .find(|group| key(&group.name) == key(name)).map(|group| group.name);
            assert_eq!(index.group_name(name), expected_group);
        }
        assert_eq!(index.group_name("public").as_deref(), Some("Public"));
    }

    /// A closure with a public group's name retains direct lookup priority.
    #[test]
    fn callable_index_keeps_closures_ahead_of_variant_fallback() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        module.functions.push(body("concrete"));
        module.closures.push(body("PUBLIC"));
        module.data.intern_string("Public:concrete");
        let index = ModuleCallableIndex::new(&module);
        assert_eq!(index.callable(&module, "public").unwrap().name, "PUBLIC");
        assert_eq!(index.group_name("public").as_deref(), Some("Public"));
    }

    /// Literal-only wrapper snapshots reuse the same declaration index without rescanning.
    #[test]
    fn callable_index_reuses_source_declarations_in_wrapper_snapshots() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        module.functions.push(body("source"));
        let shared = SharedCodegenState::default();
        let original = shared.module_callable_index(&module);
        let mut wrapper = module.clone();
        wrapper.data.intern_string("wrapper-only literal");
        let reused = shared.module_callable_index(&wrapper);
        assert!(std::ptr::eq(original, reused));
        assert_eq!(reused.function(&wrapper, "SOURCE").unwrap().name, "source");
    }

    /// A new emission cannot inherit another module's source or variant declarations.
    #[test]
    fn callable_index_is_scoped_to_one_emission() {
        let mut first = Module::new(Target::parse("linux-x86_64").unwrap());
        first.functions.push(body("first"));
        first.data.intern_string("Public:first");
        let second = Module::new(Target::parse("linux-x86_64").unwrap());
        let first_shared = SharedCodegenState::default();
        let second_shared = SharedCodegenState::default();
        assert!(first_shared.module_callable_index(&first).callable(&first, "public").is_some());
        assert!(second_shared.module_callable_index(&second).callable(&second, "public").is_none());
    }

    /// Large declaration tables stay equivalent while repeated lookups reuse one index.
    #[test]
    fn callable_index_large_graph_preserves_scanned_resolution() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        for number in 0..4096 { module.functions.push(body(&format!("function_{number}"))); }
        for number in 0..128 { module.data.intern_string(&format!("Public_{number}:missing,function_{}", number * 31)); }
        let shared = SharedCodegenState::default();
        let index = shared.module_callable_index(&module);
        assert_eq!(index.functions.len(), 4096);
        assert_eq!(index.groups.len(), 128);
        for number in 0..128 {
            let name = format!("\\PUBLIC_{number}");
            let expected = function_variants::variant_callee_for_group(&module, &name).unwrap();
            for _ in 0..16 {
                assert_eq!(index.callable(&module, &name).unwrap().name, expected.name);
                assert!(std::ptr::eq(index, shared.module_callable_index(&module)));
            }
        }
    }
    /// Keeps emitted-body membership identical to the pre-index scan on all targets.
    #[test]
    fn emitted_method_index_preserves_class_case_method_keys_and_static_kind() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = Module::new(Target::parse(target).unwrap());
            for (name, is_static) in [("Ns\\Owner::MiXeD", false), ("Ns\\Owner::Other", true), ("ns\\owner::MiXeD", true), ("Parent::Inherited", false), ("Alias#1::Method", false), ("Malformed", true), ("Nested::Owner::Tail", false)] {
                let mut function = body(name);
                function.flags.is_static = is_static;
                module.class_methods.push(function);
            }
            let index = ModuleCallableIndex::new(&module);
            for class_name in ["Ns\\Owner", "ns\\owner", "NS\\OWNER", "\\Ns\\Owner", "Parent", "Child", "Alias#1", "Alias", "Nested::Owner", "Malformed", "missing"] {
                for method_key in ["mixed", "MiXeD", "other", "inherited", "method", "tail", "missing"] {
                    for is_static in [false, true] {
                        let expected = module.class_methods.iter().any(|function| function.flags.is_static == is_static
                            && function.name.rsplit_once("::").is_some_and(|(owner, name)| owner == class_name && php_symbol_key(name) == method_key));
                        assert_eq!(index.has_emitted_method(class_name, method_key, is_static), expected, "{target}: {class_name}::{method_key} static={is_static}");
                    }
                }
            }
        }
    }

    /// Repeated large-graph membership checks reuse one source index and stay emission-local.
    #[test]
    fn emitted_method_index_reuses_finalized_tables_without_cross_module_names() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        for number in 0..4096 {
            let mut function = body(&format!("Owner{number}::Run"));
            function.flags.is_static = number % 2 == 0;
            module.class_methods.push(function);
        }
        let shared = SharedCodegenState::default();
        let index = shared.module_callable_index(&module);
        let mut snapshot = module.clone();
        snapshot.data.intern_string("new wrapper literal");
        for number in 0..4096 {
            let owner = format!("Owner{number}");
            assert!(index.has_emitted_method(&owner, "run", number % 2 == 0));
            assert!(!index.has_emitted_method(&owner, "run", number % 2 != 0));
            assert!(std::ptr::eq(index, shared.module_callable_index(&snapshot)));
        }
        let empty = Module::new(Target::parse("linux-x86_64").unwrap());
        let other = SharedCodegenState::default();
        assert!(!other.module_callable_index(&empty).has_emitted_method("Owner0", "run", true));
    }

}
