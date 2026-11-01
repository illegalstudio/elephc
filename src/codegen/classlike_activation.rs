//! Collects compiled class-like activation overlays for native runtime metadata.

use std::collections::BTreeSet;

use crate::ir::{Immediate, Module};
use crate::names::php_symbol_key;
use crate::parser::ast::ClassLikeKind;
use crate::span::Span;

/// Sorted PHP lookup keys whose interface metadata requires a runtime binding.
#[derive(Debug, Default)]
pub(super) struct ActivationRegistryNames {
    pub(super) interfaces: Vec<String>,
}

pub(super) fn collect_activation_registry_names(module: &Module) -> ActivationRegistryNames {
    let mut interfaces = BTreeSet::new();
    // Source spans are retained after AST/EIR dead-code elimination. A
    // declaration after `throw` still needs an inactive cell; otherwise an
    // existence query falls back to immutable metadata and mistakes discovery
    // for PHP activation. Compiler-injected interfaces deliberately use the
    // dummy span and retain their always-active runtime behavior.
    for (name, info) in &module.interface_infos {
        if info.declaration_span == Span::dummy() {
            continue;
        }
        let key = php_symbol_key(name.trim_start_matches('\\'));
        interfaces.insert(key);
    }
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for instruction in &function.instructions {
            let Some(Immediate::ClassLikeActivation(activation)) = &instruction.immediate else {
                continue;
            };
            let (kind, name) = (&activation.kind, &activation.name);
            if *kind == ClassLikeKind::Interface {
                if let Some(name) = module.data.strings.get(name.as_raw() as usize) {
                    interfaces.insert(php_symbol_key(name));
                }
            }
        }
    }
    ActivationRegistryNames {
        interfaces: interfaces.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Function, IrType, Op, Ownership, SourceCatalog, SourceId, Terminator};
    use crate::span::Span;
    use crate::types::PhpType;

    #[test]
    fn activation_overlay_names_are_folded_deduplicated_and_kind_filtered() {
        let catalog = SourceCatalog::from_units([crate::resolver::SourceUnit {
            canonical_path: "/unit.php".into(), mode: crate::source::SourceMode::Php, source: "<?php".into(),
        }]).unwrap();
        let mut module = Module::with_source_catalog(crate::codegen::platform::Target::detect_host(), catalog);
        let interface = module.data.intern_string("Scope\\Probe");
        let class = module.data.intern_string("Scope\\ProbeClass");
        let mut function = Function::new("main".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        for (kind, name) in [(ClassLikeKind::Interface, interface), (ClassLikeKind::Interface, interface), (ClassLikeKind::Class, class)] {
            builder.emit(Op::ClassLikeActivate, vec![], Some(Immediate::class_like_activation(
                SourceId::from_raw(0), Span::dummy(), kind, name,
            )), IrType::Void, PhpType::Void, Ownership::NonHeap);
        }
        builder.terminate(Terminator::Return { value: None });
        module.add_function(function);
        assert_eq!(collect_activation_registry_names(&module).interfaces, ["scope\\probe"]);
    }
}
