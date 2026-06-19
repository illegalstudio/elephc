//! Purpose:
//! Validates builtin Reflection owner constructor metadata for object inference.
//! Keeps ReflectionClass/Function/Method/Property/Constant/EnumCase attribute checks out
//! of the general constructor inference driver.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::constructors::Checker::validate_reflection_constructor_args()`.
//!
//! Key details:
//! - Constructor validation checks that reflected members exist and that captured
//!   attribute arguments are materializable by the current ReflectionAttribute model.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::Expr;
use crate::types::checker::Checker;
use crate::types::{collect_attribute_args, collect_attribute_names};

type ReflectionAttributeArgs = Vec<Option<Vec<crate::types::AttrArgValue>>>;

impl Checker {
    /// Validates function-level attributes for `ReflectionFunction`.
    ///
    /// The reflected function must be a statically declared user function; when
    /// it has attributes, their captured argument metadata must be materializable
    /// by `ReflectionAttribute::getArguments()`.
    pub(super) fn validate_reflection_function_attrs(
        &self,
        function_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(canonical) =
            self.canonical_function_name_folded(function_name.trim_start_matches('\\'))
        else {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionFunction::__construct(): Function {}() does not exist",
                    function_name
                ),
            ));
        };
        let Some(decl) = self.fn_decls.get(&canonical) else {
            return Ok(());
        };
        let names = collect_attribute_names(&decl.attributes);
        let args = collect_attribute_args(&decl.attributes);
        self.validate_reflection_attribute_metadata(
            &names,
            &args,
            expr,
            "ReflectionFunction::getAttributes(): function has attribute argument metadata that is not supported yet",
        )
    }

    /// Validates class-level attributes for `ReflectionClass`.
    ///
    /// Returns `Ok` when the class exists and its captured attribute argument
    /// metadata can be materialized by `ReflectionAttribute::getArguments()`.
    pub(super) fn validate_reflection_class_attrs(
        &self,
        class_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        if let Some(class_info) = self.classes.get(class_name) {
            return self.validate_reflection_attribute_metadata(
                &class_info.attribute_names,
                &class_info.attribute_args,
                expr,
                "ReflectionClass::getAttributes(): class has attribute argument metadata that is not supported yet",
            );
        }
        if self.interfaces.contains_key(class_name) || self.declared_traits.contains(class_name) {
            return Ok(());
        }
        Err(CompileError::new(
            expr.span,
            &format!(
                "ReflectionClass::__construct(): undefined class '{}'",
                class_name
            ),
        ))
    }

    /// Validates method-level attributes for `ReflectionMethod`.
    ///
    /// Checks that the method exists on the reflected class before validating
    /// its captured attribute argument metadata.
    pub(super) fn validate_reflection_method_attrs(
        &self,
        class_name: &str,
        method_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let method_key = php_symbol_key(method_name);
        if let Some(class_info) = self.classes.get(class_name) {
            if !class_info.methods.contains_key(&method_key)
                && !class_info.static_methods.contains_key(&method_key)
            {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "ReflectionMethod::__construct(): undefined method '{}::{}'",
                        class_name, method_name
                    ),
                ));
            }
            let empty_names = Vec::new();
            let empty_args = Vec::new();
            let names = class_info
                .method_attribute_names
                .get(&method_key)
                .unwrap_or(&empty_names);
            let args = class_info
                .method_attribute_args
                .get(&method_key)
                .unwrap_or(&empty_args);
            return self.validate_reflection_attribute_metadata(
                names,
                args,
                expr,
                "ReflectionMethod::getAttributes(): method has attribute argument metadata that is not supported yet",
            );
        }
        if let Some(interface_info) = self.interfaces.get(class_name) {
            if interface_info.methods.contains_key(&method_key)
                || interface_info.static_methods.contains_key(&method_key)
            {
                return Ok(());
            }
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionMethod::__construct(): undefined method '{}::{}'",
                    class_name, method_name
                ),
            ));
        }
        if let Some(trait_methods) = self.declared_trait_methods.get(class_name) {
            if trait_methods.contains_key(&method_key) {
                return Ok(());
            }
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionMethod::__construct(): undefined method '{}::{}'",
                    class_name, method_name
                ),
            ));
        }
        Err(CompileError::new(
            expr.span,
            &format!(
                "ReflectionMethod::__construct(): undefined class '{}'",
                class_name
            ),
        ))
    }

    /// Validates property-level attributes for `ReflectionProperty`.
    ///
    /// Checks both instance and static properties because PHP reflection accepts
    /// either surface through the same constructor.
    pub(super) fn validate_reflection_property_attrs(
        &self,
        class_name: &str,
        property_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(class_info) = self.classes.get(class_name) else {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionProperty::__construct(): undefined class '{}'",
                    class_name
                ),
            ));
        };
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == property_name)
            && !class_info
                .static_properties
                .iter()
                .any(|(name, _)| name == property_name)
        {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionProperty::__construct(): undefined property '{}::${}'",
                    class_name, property_name
                ),
            ));
        }
        let empty_names = Vec::new();
        let empty_args = Vec::new();
        let names = class_info
            .property_attribute_names
            .get(property_name)
            .unwrap_or(&empty_names);
        let args = class_info
            .property_attribute_args
            .get(property_name)
            .unwrap_or(&empty_args);
        self.validate_reflection_attribute_metadata(
            names,
            args,
            expr,
            "ReflectionProperty::getAttributes(): property has attribute argument metadata that is not supported yet",
        )
    }

    /// Validates class-constant or enum-case attributes for `ReflectionClassConstant`.
    ///
    /// Enum cases are checked first because PHP exposes them through
    /// `ReflectionClassConstant` as well as the enum-case-specific reflectors.
    pub(super) fn validate_reflection_class_constant_attrs(
        &self,
        class_name: &str,
        constant_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        if let Some((names, args)) =
            self.reflection_enum_case_attribute_metadata(class_name, constant_name)
        {
            return self.validate_reflection_attribute_metadata(
                &names,
                &args,
                expr,
                "ReflectionClassConstant::getAttributes(): enum case has attribute argument metadata that is not supported yet",
            );
        }
        let Some((names, args)) = self
            .reflection_class_constant_attribute_metadata(class_name, constant_name)
            .or_else(|| self.reflection_interface_constant_metadata(class_name, constant_name))
            .or_else(|| self.reflection_trait_constant_metadata(class_name, constant_name))
        else {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionClassConstant::__construct(): undefined class constant '{}::{}'",
                    class_name, constant_name
                ),
            ));
        };
        self.validate_reflection_attribute_metadata(
            &names,
            &args,
            expr,
            "ReflectionClassConstant::getAttributes(): class constant has attribute argument metadata that is not supported yet",
        )
    }

    /// Validates enum-case attributes for `ReflectionEnumUnitCase` or `ReflectionEnumBackedCase`.
    ///
    /// `require_backed` is true for `ReflectionEnumBackedCase`; unit-case
    /// reflection accepts backed cases too, matching PHP.
    pub(super) fn validate_reflection_enum_case_attrs(
        &self,
        enum_name: &str,
        case_name: &str,
        require_backed: bool,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(enum_info) = self.enums.get(enum_name) else {
            return Err(CompileError::new(
                expr.span,
                &format!("{} is not an enum", enum_name),
            ));
        };
        if require_backed && enum_info.backing_type.is_none() {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "Enum case {}::{} is not a backed case",
                    enum_name, case_name
                ),
            ));
        }
        let Some((names, args)) =
            self.reflection_enum_case_attribute_metadata(enum_name, case_name)
        else {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionEnumUnitCase::__construct(): undefined enum case '{}::{}'",
                    enum_name, case_name
                ),
            ));
        };
        self.validate_reflection_attribute_metadata(
            &names,
            &args,
            expr,
            "ReflectionEnumUnitCase::getAttributes(): enum case has attribute argument metadata that is not supported yet",
        )
    }

    /// Returns cloned class-constant attribute metadata, walking parent classes.
    fn reflection_class_constant_attribute_metadata(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Option<(Vec<String>, ReflectionAttributeArgs)> {
        let class_info = self.classes.get(class_name)?;
        if class_info.constants.contains_key(constant_name) {
            return Some((
                class_info
                    .constant_attribute_names
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
                class_info
                    .constant_attribute_args
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
            ));
        }
        let parent = class_info.parent.as_deref()?;
        self.reflection_class_constant_attribute_metadata(parent, constant_name)
    }

    /// Returns empty attribute metadata when an interface constant exists.
    fn reflection_interface_constant_metadata(
        &self,
        interface_name: &str,
        constant_name: &str,
    ) -> Option<(Vec<String>, ReflectionAttributeArgs)> {
        if let Some(info) = self.interfaces.get(interface_name) {
            if info.constants.contains_key(constant_name) {
                return Some((Vec::new(), Vec::new()));
            }
        }
        let class_info = self.classes.get(interface_name)?;
        class_info.interfaces.iter().find_map(|implemented| {
            self.interfaces
                .get(implemented)
                .filter(|info| info.constants.contains_key(constant_name))
                .map(|_| (Vec::new(), Vec::new()))
        })
    }

    /// Returns empty attribute metadata when a trait constant exists.
    fn reflection_trait_constant_metadata(
        &self,
        trait_name: &str,
        constant_name: &str,
    ) -> Option<(Vec<String>, ReflectionAttributeArgs)> {
        self.declared_trait_constants
            .get(trait_name)
            .filter(|constants| constants.contains(constant_name))
            .map(|_| (Vec::new(), Vec::new()))
    }

    /// Returns cloned enum-case attribute metadata for one case.
    fn reflection_enum_case_attribute_metadata(
        &self,
        enum_name: &str,
        case_name: &str,
    ) -> Option<(Vec<String>, ReflectionAttributeArgs)> {
        self.enums
            .get(enum_name)?
            .cases
            .iter()
            .find(|case| case.name == case_name)
            .map(|case| (case.attribute_names.clone(), case.attribute_args.clone()))
    }

    /// Validates one pair of reflection attribute metadata slices.
    fn validate_reflection_attribute_metadata(
        &self,
        names: &[String],
        args: &[Option<Vec<crate::types::AttrArgValue>>],
        expr: &Expr,
        message: &str,
    ) -> Result<(), CompileError> {
        if super::attributes_have_unsupported_args(names, args) {
            return Err(CompileError::new(expr.span, message));
        }
        Ok(())
    }
}
