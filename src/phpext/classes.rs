//! Purpose:
//! Collects the checked class surface exposed to a real PHP extension host.
//!
//! Called from:
//! - The export scan before declaration pruning and SDK compilation.
//!
//! Key details:
//! - Source classes stay explicitly final until host subclass dispatch is supported.
//! - Physical method signatures include the receiver or late-static class ID.

use std::collections::{HashMap, HashSet};

use crate::{errors::CompileError, exports::ExportedFunction, parser::ast::{Program, StmtKind, TypeExpr, Visibility},
    span::Span, types::{ClassInfo, FunctionSig, PhpType}, codegen_support::platform::Target};

/// A real Zend class and its compiler-owned physical invocation entries.
#[derive(Clone, Debug)]
pub struct ExportedClass {
    pub name: String,
    pub info: ClassInfo,
    pub methods: Vec<ExportedMethod>,
    pub helpers: Vec<super::class_helpers::ObjectHelper>,
    pub property_defaults: Vec<Option<super::constants::ConstantValue>>,
    pub property_types: Vec<TypeExpr>,
    pub constants: Vec<ExportedClassConstant>,
}

/// A public class constant with its source type and resolved persistent value.
#[derive(Clone, Debug)]
pub struct ExportedClassConstant {
    pub name: String,
    pub declared_type: Option<TypeExpr>,
    pub is_final: bool,
    pub value: super::constants::ConstantValue,
}

/// Source-visible method metadata with a separate physical native signature.
#[derive(Clone, Debug)]
pub struct ExportedMethod {
    pub name: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub declared_return: Option<TypeExpr>,
    pub declared_params: Vec<Option<TypeExpr>>,
    pub export: ExportedFunction,
}

/// Collects explicitly exported classes and rejects declarations with no proven host ABI.
pub fn collect(program: &Program, classes: &HashMap<String, ClassInfo>, target: Target) -> Result<Vec<ExportedClass>, CompileError> {
    let names = program.iter().filter(|stmt| crate::exports::has_export_attribute(stmt))
        .filter_map(|stmt| match &stmt.kind { StmtKind::ClassDecl { name, .. } => Some(name.clone()), _ => None })
        .collect::<HashSet<_>>();
    let constant_context = if names.is_empty() { HashMap::new() } else { super::constants::context(program, target) };
    if !names.is_empty() {
        for stmt in program {
            if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
                if name.starts_with("__elephc_phpext_object_") {
                    return Err(CompileError::new(stmt.span, "PHP class exports reserve the __elephc_phpext_object_ function prefix"));
                }
            }
        }
    }
    let mut result = Vec::new();
    for stmt in program.iter().filter(|stmt| crate::exports::has_export_attribute(stmt)) {
        let (name, properties, declarations) = match &stmt.kind {
            StmtKind::ClassDecl { name, properties, methods, .. } => (name, properties, methods),
            StmtKind::EnumDecl { .. } | StmtKind::InterfaceDecl { .. } | StmtKind::TraitDecl { .. } =>
                return Err(CompileError::new(stmt.span, "PHP class exports do not yet support interfaces, traits or enums")),
            _ => continue,
        };
        let fail = |detail: &str| CompileError::new(stmt.span, &format!("PHP class export '{name}': {detail}"));
        let info = classes.get(name).ok_or_else(|| fail("missing checked class metadata"))?;
        if !info.is_final { return Err(fail("requires an explicitly final class until host subclassing is supported")); }
        if info.parent.is_some() || !info.interfaces.is_empty() || !info.used_traits.is_empty() || info.is_abstract {
            return Err(fail("inheritance, interfaces, traits and abstract classes are not yet supported"));
        }
        if info.is_readonly_class || info.allow_dynamic_properties || !info.static_properties.is_empty() {
            return Err(fail("readonly classes, dynamic properties and static properties are not yet supported"));
        }
        if !info.reference_properties.is_empty() || !info.readonly_properties.is_empty()
            || !info.property_set_visibilities.is_empty()
            || properties.iter().any(|property| property.hooks.any() || property.is_abstract) {
            return Err(fail("property hooks, reference properties, readonly and asymmetric properties are not yet supported"));
        }
        if properties.iter().any(|property| property.type_expr.is_none()) {
            return Err(fail("properties require a declared PHP type; use mixed for dynamic values"));
        }
        for (property, ty) in &info.properties {
            if !super::signature::supported(ty) || !super::signature::input_storage_supported(ty) {
                return Err(fail(&format!("property {property} has unsupported boundary type {ty:?}")));
            }
            validate_object_types(name, ty, &names, stmt.span)?;
            super::class_registration::type_initializer(ty, false).map_err(|detail| fail(&detail))?;
        }
        let mut methods = Vec::new();
        for method in &info.method_decls {
            if method.name.eq_ignore_ascii_case("__construct") && method.visibility != Visibility::Public {
                return Err(fail("non-public constructors are not yet supported"));
            }
            if method.name.starts_with("__") && !method.name.eq_ignore_ascii_case("__construct") {
                return Err(CompileError::new(method.span, &format!(
                    "PHP class export '{name}': magic method {} has no audited host object policy", method.name)));
            }
            // Implementation-only methods stay on the native side; Zend exposes the public surface.
            if method.visibility != Visibility::Public { continue; }
            let signatures = if method.is_static { &info.static_methods } else { &info.methods };
            let internal = signatures.get(&crate::names::php_symbol_key(&method.name)).ok_or_else(|| fail("missing checked method signature"))?;
            let full_name = format!("{name}::{}", method.name);
            let mut source = crate::exports::source_signature(&full_name, internal, method.span)?;
            let constructor = method.name.eq_ignore_ascii_case("__construct");
            if constructor { source.declared_return = true; }
            super::signature::validate(&full_name, &source, method.span)?;
            validate_signature_classes(&full_name, &source, &names, method.span)?;
            let receiver = if method.is_static { PhpType::Int } else { PhpType::Object(name.clone()) };
            let mut physical = internal.clone();
            prepend_receiver(&mut source, receiver.clone());
            prepend_receiver(&mut physical, receiver);
            methods.push(ExportedMethod {
                name: method.name.clone(), visibility: method.visibility.clone(),
                is_static: method.is_static, is_final: method.is_final,
                declared_return: declarations.iter().find(|decl| decl.name.eq_ignore_ascii_case(&method.name))
                    .and_then(|decl| decl.return_type.clone()),
                declared_params: declarations.iter().find(|decl| decl.name.eq_ignore_ascii_case(&method.name))
                    .map(|decl| decl.params.iter().map(|(_, ty, _, _)| ty.clone()).collect()).unwrap_or_default(),
                export: ExportedFunction {
                    name: full_name, c_name: format!("elephc_php_class_{}_{}", result.len(), methods.len()),
                    source_sig: source, internal_sig: physical, span: method.span,
                    php_void_return: constructor || matches!(method.return_type, Some(TypeExpr::Void)),
                },
            });
        }
        let property_defaults = info.defaults.iter().map(|value| value.as_ref().map(|value|
            super::constants::evaluate_in_class(value, name, &constant_context)).transpose()).collect::<Result<Vec<_>, _>>()?;
        let mut constants = Vec::new();
        for constant in &info.constant_order {
            if info.constant_visibilities.get(constant).is_some_and(|visibility| *visibility != Visibility::Public) { continue; }
            constants.push(ExportedClassConstant {
                name: constant.clone(), declared_type: info.constant_types.get(constant).cloned(),
                is_final: info.final_constants.contains(constant),
                value: super::constants::evaluate_in_class(&info.constants[constant], name, &constant_context)?,
            });
        }
        let property_types = info.properties.iter().map(|(name, _)| properties.iter()
            .find(|property| &property.name == name).and_then(|property| property.type_expr.clone())
            .expect("exported properties have checked explicit source types")).collect();
        let mut class = ExportedClass { name: name.clone(), info: info.clone(), methods, helpers: Vec::new(), property_defaults, property_types, constants };
        class.helpers = super::class_helpers::describe(&class);
        result.push(class);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the checked declaration path without requiring a PHP development SDK.
    fn checked(source: &str) -> (Program, crate::types::CheckResult) {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let result = crate::types::check(&program).unwrap();
        (program, result)
    }

    /// Keeps source-visible method arity separate from hidden receiver and static-class arguments.
    #[test]
    fn phpext_class_metadata_preserves_physical_method_arguments() {
        let (program, checked) = checked(r#"<?php
#[Export] final class Pippo {
    public int $value = 7;
    public function __construct(int $value = 7) { $this->value = $value; }
    public function addValue(int $amount): int { return $this->value + $amount; }
    public static function doubled(int $value): int { return $value * 2; }
}
"#);
        let classes = collect(&program, &checked.classes, Target::detect_host()).unwrap();
        let class = &classes[0];
        assert_eq!(class.name, "Pippo");
        assert_eq!(class.methods.len(), 3);
        assert_eq!(class.methods[1].name, "addValue");
        assert_eq!(class.methods[1].export.source_sig.params[0].1, PhpType::Object("Pippo".into()));
        assert_eq!(class.methods[2].export.source_sig.params[0].1, PhpType::Int);
        assert_eq!(class.helpers.len(), 8);
    }

    /// Refuses PHP subclasses explicitly rather than silently changing a source declaration.
    #[test]
    fn phpext_class_metadata_requires_explicit_final() {
        let (program, checked) = checked("<?php #[Export] class Pippo {}");
        let error = collect(&program, &checked.classes, Target::detect_host()).unwrap_err();
        assert!(error.message.contains("explicitly final"));
    }

    /// Rejects an unrelated object's declared ABI even when its storage is a native object pointer.
    #[test]
    fn phpext_class_metadata_rejects_foreign_object_types() {
        let error = validate_object_types("Pippo::read", &PhpType::Object("Unexported".into()),
            &HashSet::from(["Pippo".into()]), Span::dummy()).unwrap_err();
        assert!(error.message.contains("not an exported class"));
    }

    /// Traverses private implementation calls instead of treating every final-class call as safe.
    #[test]
    fn phpext_class_safety_checks_transitive_private_methods() {
        for source in [
            "<?php #[Export] final class Pippo { public function run(): int { return $this->hidden(); } private function hidden(): int { exit(1); } }",
            "<?php #[Export] final class Pippo { public static function run(): int { return self::hidden(); } private static function hidden(): int { exit(1); } }",
        ] {
            let (program, checked) = checked(source);
            let target = Target::detect_host();
            let classes = collect(&program, &checked.classes, target).unwrap();
            let exports = classes.iter().flat_map(|class| class.methods.iter().map(|method| &method.export)
                .chain(class.helpers.iter().map(|helper| &helper.export)))
                .map(|export| (export.name.clone(), export.clone())).collect();
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            super::super::class_helpers::lower(&mut module, &classes);
            let error = crate::exports::validate_phpext_call_graph(&module, &exports).unwrap_err();
            assert!(error.message.contains("Pippo::hidden"), "{}", error.message);
        }
    }
}

/// Preserves the private physical receiver independently of source-visible parameter metadata.
fn prepend_receiver(signature: &mut FunctionSig, ty: PhpType) {
    signature.params.insert(0, ("this".into(), ty));
    signature.param_type_exprs.insert(0, None);
    signature.param_attributes.insert(0, Vec::new());
    signature.defaults.insert(0, None);
    signature.ref_params.insert(0, false);
    signature.declared_params.insert(0, true);
}

/// Requires every declared object type at the boundary to belong to this module.
pub fn validate_signature_classes(name: &str, signature: &FunctionSig, names: &HashSet<String>, span: Span) -> Result<(), CompileError> {
    for (_, ty) in &signature.params { validate_object_types(name, ty, names, span)?; }
    validate_object_types(name, &signature.return_type, names, span)
}

/// Rejects object layouts that the receiving Zend module cannot identify or retain.
fn validate_object_types(name: &str, ty: &PhpType, names: &HashSet<String>, span: Span) -> Result<(), CompileError> {
    match ty {
        PhpType::Object(class) if !names.iter().any(|name| name.eq_ignore_ascii_case(class)) =>
            Err(CompileError::new(span, &format!("PHP export '{name}': object class '{class}' is not an exported class"))),
        PhpType::Union(members) => {
            for member in members { validate_object_types(name, member, names, span)?; }
            Ok(())
        }
        PhpType::Array(value) => validate_object_types(name, value, names, span),
        PhpType::AssocArray { key, value } => {
            validate_object_types(name, key, names, span)?;
            validate_object_types(name, value, names, span)
        }
        _ => Ok(()),
    }
}
