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

type ReflectionAttributeArgs = Vec<Option<Vec<crate::types::AttrArgEntry>>>;

impl Checker {
    /// Validates function-level attributes for `ReflectionFunction`.
    ///
    /// The reflected function must be a statically declared user function or a
    /// supported callable builtin; user-function attributes must have
    /// materializable `ReflectionAttribute::getArguments()` metadata.
    pub(super) fn validate_reflection_function_attrs(
        &self,
        function_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(canonical) =
            self.canonical_function_name_folded(function_name.trim_start_matches('\\'))
        else {
            let builtin_key = php_symbol_key(function_name.trim_start_matches('\\'));
            if crate::types::first_class_callable_builtin_sig(&builtin_key).is_some() {
                return Ok(());
            }
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
        // A NAME THIS WALK CANNOT SEE IS NOT THE SAME AS A NAME THAT DOES NOT EXIST. When the
        // program includes PHP at run time or registers an autoloader, the reflected class can be
        // declared by text that does not exist at compile time, and refusing here turns a program
        // `php -n` 8.5.6 runs to completion into a compile error. Measured: `main.php` doing
        // `include $computedPath;` then `new ReflectionClass("S1Plain")`, with `S1Plain` declared
        // by the included file, reported
        // `error[4:26]: ReflectionClass::__construct(): undefined class 'S1Plain'` where PHP
        // printed `loaded;S1Plain;done`. `include_reflection::
        // test_caller_reflects_on_a_class_a_runtime_include_declared` is the pinned case.
        //
        // Only the ATTRIBUTE validation is skipped by deferring, and that is the honest trade:
        // this function exists to reject attribute argument metadata the reflection surface
        // cannot materialize, and there is no metadata to inspect for a class the compiler has
        // never seen. The reflection itself resolves at run time through the bridge, and a name
        // nothing supplies still raises PHP's own `ReflectionException` there.
        if self.reflection_defers_unknown_classes() {
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

    /// Validates enum-level attributes for `ReflectionEnum`.
    ///
    /// The reflected symbol must be a declared enum; enum attributes are stored
    /// on the parallel class metadata produced for enum declarations.
    pub(super) fn validate_reflection_enum_attrs(
        &self,
        enum_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        if !self.enums.contains_key(enum_name) {
            return Err(CompileError::new(
                expr.span,
                &format!("ReflectionEnum::__construct(): {} is not an enum", enum_name),
            ));
        }
        if let Some(class_info) = self.classes.get(enum_name) {
            return self.validate_reflection_attribute_metadata(
                &class_info.attribute_names,
                &class_info.attribute_args,
                expr,
                "ReflectionEnum::getAttributes(): enum has attribute argument metadata that is not supported yet",
            );
        }
        Ok(())
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
        // Same reasoning as `validate_reflection_class_attrs`: a class a runtime include or an
        // autoloader supplies is not a class that does not exist, and its METHOD set is unknown
        // for the same reason its attributes are. Refusing here would reject a program PHP runs.
        if self.reflection_defers_unknown_classes() {
            return Ok(());
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
            // Same reasoning as the two validators above: a class a runtime include or an
            // autoloader supplies has no property set this walk can read, and refusing here would
            // reject a program PHP runs to completion.
            if self.reflection_defers_unknown_classes() {
                return Ok(());
            }
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionProperty::__construct(): undefined class '{}'",
                    class_name
                ),
            ));
        };
        let builtin_throwable_virtual_property = class_name == "Exception"
            && matches!(property_name, "file" | "line" | "trace");
        if !builtin_throwable_virtual_property
            && !class_info
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
            // php's `Constant P::X does not exist` is a run-time ReflectionException.
            return Ok(());
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
        // A name that is no enum, a pure enum's case asked for as backed, or a missing case is
        // php's catchable `ReflectionException` at run time (`Constant P::K is not a case`,
        // `Enum case E::A is not a backed case`), which the interpreter raises; refusing the
        // program here rejected a `try`/`catch` php runs.
        let _ = require_backed;
        if !self.enums.contains_key(enum_name) {
            return Ok(());
        }
        let Some((names, args)) =
            self.reflection_enum_case_attribute_metadata(enum_name, case_name)
        else {
            return Ok(());
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
        args: &[Option<Vec<crate::types::AttrArgEntry>>],
        expr: &Expr,
        message: &str,
    ) -> Result<(), CompileError> {
        if super::attributes_have_unsupported_args(names, args) {
            return Err(CompileError::new(expr.span, message));
        }
        Ok(())
    }
}
