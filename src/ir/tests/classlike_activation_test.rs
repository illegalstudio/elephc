//! Typed declaration activation identity and fail-closed metadata validation.

use crate::ir::*;
use crate::parser::ast::ClassLikeKind;
use crate::span::Span;
use crate::types::PhpType;

fn module() -> Module {
    let catalog = SourceCatalog::from_units([crate::resolver::SourceUnit {
        canonical_path: "/unit.php".into(),
        mode: crate::source::SourceMode::Php,
        source: "<?php interface Probe {}".into(),
    }]).unwrap();
    let mut module = Module::with_source_catalog(crate::codegen::platform::Target::detect_host(), catalog);
    let name = module.data.intern_string("Probe");
    let mut function = Function::new("main".into(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    builder.emit(Op::ClassLikeActivate, vec![], Some(Immediate::class_like_activation(
        SourceId::from_raw(0), Span::new(1, 7), ClassLikeKind::Interface, name,
    )), IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: None });
    module.add_function(function);
    module
}

#[test]
fn classlike_activation_validation_preserves_typed_source_and_name() {
    let module = module();
    assert_eq!(validate_module(&module), Ok(()));
    let text = print_module(&module);
    assert!(text.contains("class_like_activate"));
    assert!(text.contains("declaration[0:1:7:Interface:data[0]]"));
    assert!(!Op::ClassLikeActivate.default_effects().is_pure());
    assert!(Op::ClassLikeActivate.default_effects().contains(Effects::MAY_THROW | Effects::WRITES_GLOBAL));
}

#[test]
fn classlike_activation_validation_rejects_unknown_source() {
    let mut module = module();
    let Some(Immediate::ClassLikeActivation(activation)) =
        &mut module.functions[0].instructions[0].immediate else { panic!("activation expected") };
    activation.source = SourceId::from_raw(1);
    assert_eq!(validate_module(&module), Err(ValidationError::UnknownSource(SourceId::from_raw(1))));
}

#[test]
fn classlike_activation_validation_rejects_invalid_name() {
    let mut module = module();
    module.data.strings[0].clear();
    assert_eq!(validate_module(&module), Err(ValidationError::InvalidDeclarationName(DataId::from_raw(0))));
}

#[test]
fn classlike_activation_validation_rejects_untyped_identity() {
    let mut module = module();
    module.functions[0].instructions[0].immediate = Some(Immediate::Source(SourceId::from_raw(0)));
    assert!(matches!(validate_module(&module), Err(ValidationError::MissingImmediate {
        expected: "declaration site", ..
    })));
}

#[test]
fn classlike_activation_ast_preserves_names_and_source_without_reserving_symbol() {
    use crate::parser::ast::{Stmt, StmtKind};
    let site = Span::new(4, 9);
    let program = vec![Stmt::new(StmtKind::NamespaceBlock {
        name: Some(crate::names::Name::unqualified("Scope")),
        body: vec![Stmt::new(StmtKind::ClassLikeActivate {
            name: "Probe".into(), kind: ClassLikeKind::Interface, source_path: "/unit.php".into(),
        }, site)],
    }, site)];
    let resolved = crate::name_resolver::resolve(program).unwrap();
    let StmtKind::ClassLikeActivate { name, kind, source_path } = &resolved[0].kind
        else { panic!("activation must survive namespace resolution") };
    assert_eq!(name, "Scope\\Probe");
    assert_eq!(*kind, ClassLikeKind::Interface);
    assert_eq!(source_path, std::path::Path::new("/unit.php"));
    assert_eq!(resolved[0].span, site);
    let checked = crate::types::check(&resolved).unwrap();
    assert!(!checked.interfaces.contains_key("Scope\\Probe"));
    let usage = crate::optimize::reachability::usage::scan_stmt(&resolved[0]);
    assert!(usage.classes.contains("scope\\probe"));
    let catalog = module().source_catalog().unwrap().clone();
    let lowered = crate::ir_lower::lower_program_with_source_catalog(
        &resolved, &checked, crate::codegen::platform::Target::detect_host(),
        std::path::Path::new("/unit.php"), false, catalog,
    ).unwrap();
    assert!(lowered.functions.iter().flat_map(|function| &function.instructions)
        .any(|inst| matches!(&inst.immediate, Some(Immediate::ClassLikeActivation(activation)) if activation.site == site)));
}

#[test]
fn non_interface_activation_backend_fails_explicitly_on_all_targets_until_implemented() {
    use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
        Target { apple_variant: AppleVariant::IOS, ..Target::new(Platform::MacOS, Arch::AArch64) },
        Target { apple_variant: AppleVariant::IOSSimulator, ..Target::new(Platform::MacOS, Arch::AArch64) },
    ] {
        let mut module = module();
        module.target = target;
        let Some(Immediate::ClassLikeActivation(activation)) =
            &mut module.functions[0].instructions[0].immediate else { panic!("activation expected") };
        activation.kind = ClassLikeKind::Trait;
        let error = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .expect_err("activation must not silently disappear");
        assert!(error.to_string().contains("class-like activation for Trait"), "{target:?}: {error}");
    }
}
