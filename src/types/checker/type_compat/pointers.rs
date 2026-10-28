//! Purpose:
//! Checks type compatibility for pointers cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::{packed_type_size, width_spelling_hint, width_type_from_type_expr, PhpType};

use super::object_types::canonical_builtin_iterable_interface_name;
use super::super::builtin_enums::is_builtin_enum_name;
use super::super::Checker;

impl Checker {
    /// Returns true if `ty` is a `PhpType::Pointer`.
    pub(crate) fn is_pointer_type(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Pointer(_))
    }

    /// Returns true if both `left` and `right` are `PhpType::Pointer` (any target).
    /// Pointers of different target types are considered compatible at this level.
    pub(crate) fn pointer_types_compatible(left: &PhpType, right: &PhpType) -> bool {
        matches!((left, right), (PhpType::Pointer(_), PhpType::Pointer(_)))
    }

    /// Normalizes a pointer target type name string to its canonical form.
    /// Maps PHP aliases (int/integer, float/double/real, bool/boolean, ptr/pointer) to
    /// their canonical equivalents, validates class/packed/extern names, and returns
    /// `None` for unknown types.
    pub(crate) fn normalize_pointer_target_type(&self, target_type: &str) -> Option<String> {
        match target_type {
            "int" | "integer" => Some("int".to_string()),
            "float" | "double" | "real" => Some("float".to_string()),
            "bool" | "boolean" => Some("bool".to_string()),
            "string" => Some("string".to_string()),
            "ptr" | "pointer" => Some("ptr".to_string()),
            // `declared_classes` covers classes whose schema is not built yet: member signatures
            // and packed class fields resolve `ptr<T>` while the class schema pass is still
            // registering the classes they name. Builtin enums are injected after that pass and
            // after the packed and extern prescan, so they are recognized by name.
            class_name
                if self.classes.contains_key(class_name)
                    || self.declared_classes.contains(class_name)
                    || is_builtin_enum_name(self, class_name) =>
            {
                Some(class_name.to_string())
            }
            class_name if self.packed_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            class_name if self.extern_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            class_name => {
                let key = php_symbol_key(class_name);
                self.declared_native_records
                    .iter()
                    .find(|declared| php_symbol_key(declared) == key)
                    .cloned()
            }
        }
    }

    /// Resolves a declared pointer target, including nested pointers and C scalar widths.
    fn normalize_declared_pointer_target(&self, target: &str) -> Option<String> {
        if let Some(inner) = target.strip_prefix("ptr<").and_then(|s| s.strip_suffix('>')) {
            return self.normalize_declared_pointer_target(inner).map(|name| format!("ptr<{}>", name));
        }
        if crate::parser::ast::is_c_width_type_name(target) {
            return Some(crate::names::pointer_width_tag(target));
        }
        if let Some(rooted) = target.strip_prefix('\\') {
            return self.normalize_pointer_target_type(rooted);
        }
        self.normalize_pointer_target_type(target)
    }

    /// Resolves the element type of `buffer<T>` or `buffer_new<T>()`.
    ///
    /// A C width name (`buffer<i32>`, `buffer<u8>`, `buffer<f32>`) is the element's storage
    /// format and resolves to `PhpType::Narrow`, with `i64`, `u64` and `f64` the plain word
    /// types; any other element type resolves normally and must be POD-sized, otherwise
    /// `message` is reported.
    pub(crate) fn resolve_buffer_element_type(
        &self,
        inner: &crate::parser::ast::TypeExpr,
        span: crate::span::Span,
        message: &str,
    ) -> Result<PhpType, CompileError> {
        if let Some(width_type) = width_type_from_type_expr(inner) {
            return Ok(width_type);
        }
        let inner_ty = self
            .resolve_type_expr(inner, span)
            .map_err(|error| self.width_position_type_error(inner, span, error))?;
        if packed_type_size(&inner_ty, &self.packed_classes).is_none() {
            return Err(CompileError::new(span, message));
        }
        Ok(inner_ty)
    }

    /// Rewrites the diagnostic of a packed field or buffer element type that failed to resolve
    /// when its name looks like a misspelled width (`I32`, `uint8_t`, `double`): the message
    /// then lists the accepted spellings instead of pointing at a missing class. Any other
    /// failure keeps its original diagnostic.
    pub(crate) fn width_position_type_error(
        &self,
        type_expr: &crate::parser::ast::TypeExpr,
        span: crate::span::Span,
        error: CompileError,
    ) -> CompileError {
        let crate::parser::ast::TypeExpr::Named(name) = type_expr else {
            return error;
        };
        if self.normalize_pointer_target_type(name.as_str()).is_some() {
            return error;
        }
        match width_spelling_hint(name.as_str()) {
            Some(hint) => CompileError::new(
                span,
                &format!(
                    "Unknown type: {}; storage widths are spelled {} (did you mean {}?)",
                    name.as_str(),
                    crate::parser::ast::c_width_type_names().join(", "),
                    hint
                ),
            ),
            None => error,
        }
    }

    /// Resolves a `TypeExpr` AST node to a `PhpType`. Handles all primitive types, nullable,
    /// union, pointer, buffer, and named (class/interface/packed/extern) type expressions.
    /// Validates that buffer element types are POD-sized.
    pub(crate) fn resolve_type_expr(
        &self,
        type_expr: &crate::parser::ast::TypeExpr,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        match type_expr {
            // A declared signature resolves to an ORDINARY callable: the storage is one callable
            // descriptor either way, and the signature exists for inference, which reads it off
            // the declaration rather than off the resolved type. Giving it its own `PhpType`
            // would make every existing callable path have two shapes to handle for no gain.
            crate::parser::ast::TypeExpr::CallableSig { .. } => Ok(PhpType::Callable),
            // The instantiated class, named exactly as `generics::classes` names it.
            //
            // Almost every generic mention is rewritten to a `Named` before this checker runs.
            // The exception is the checker's OWN work: instantiating `unwrap<int>` from
            // `function unwrap<T>(Box<T> $b)` substitutes `T` and resolves the result in the
            // same pass, so the freshly built signature still says `Box<int>` structurally.
            // Naming it here is not a guess — the AST splice appends that same instantiation,
            // and the next monomorphization round emits the class under this very name.
            //
            // A mention whose class declares no type parameters never reaches here:
            // `generics::classes` rejects it while rewriting, with a message that names the
            // class rather than the type it failed to become.
            type_expr @ crate::parser::ast::TypeExpr::GenericClass { .. } => Ok(PhpType::Object(
                crate::generics::describe_type(type_expr),
            )),
            crate::parser::ast::TypeExpr::Int => Ok(PhpType::Int),
            crate::parser::ast::TypeExpr::Float => Ok(PhpType::Float),
            crate::parser::ast::TypeExpr::Bool => Ok(PhpType::Bool),
            crate::parser::ast::TypeExpr::False => Ok(PhpType::False),
            crate::parser::ast::TypeExpr::Str => Ok(PhpType::Str),
            crate::parser::ast::TypeExpr::Void => Ok(PhpType::Void),
            crate::parser::ast::TypeExpr::Never => Ok(PhpType::Never),
            crate::parser::ast::TypeExpr::Iterable => Ok(PhpType::Iterable),
            crate::parser::ast::TypeExpr::Array(inner) => {
                Ok(PhpType::Array(Box::new(self.resolve_type_expr(inner, span)?)))
            }
            crate::parser::ast::TypeExpr::AssocArray { key, value } => {
                let key_ty = self.resolve_type_expr(key, span)?;
                if !matches!(key_ty, PhpType::Int | PhpType::Str | PhpType::Mixed) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "array<K, V> key type must be int, string, or mixed, got {}",
                            key_ty
                        ),
                    ));
                }
                Ok(PhpType::AssocArray {
                    key: Box::new(key_ty),
                    value: Box::new(self.resolve_type_expr(value, span)?),
                })
            }
            crate::parser::ast::TypeExpr::Nullable(inner) => {
                let inner_ty = self.resolve_type_expr(inner, span)?;
                Ok(self.normalize_union_type(vec![inner_ty, PhpType::Void]))
            }
            crate::parser::ast::TypeExpr::Union(members) => {
                let resolved = members
                    .iter()
                    .map(|member| self.resolve_type_expr(member, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.normalize_union_type(resolved))
            }
            crate::parser::ast::TypeExpr::Intersection(members) => {
                // Resolve every member so unknown types are reported, then type the value as its
                // first member; call boundaries validate that arguments satisfy all members.
                let resolved = members
                    .iter()
                    .map(|member| self.resolve_type_expr(member, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(resolved.into_iter().next().unwrap_or(PhpType::Mixed))
            }
            crate::parser::ast::TypeExpr::Ptr(target) => {
                let normalized = match target {
                    Some(name) => (if name.is_unqualified() {
                        self.normalize_declared_pointer_target(name.as_str())
                    } else {
                        self.normalize_pointer_target_type(name.as_str())
                    })
                        .ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!("Unknown pointer target type: {}", name.as_str()),
                            )
                        })?,
                    None => return Ok(PhpType::Pointer(None)),
                };
                Ok(PhpType::Pointer(Some(normalized)))
            }
            crate::parser::ast::TypeExpr::Buffer(inner) => {
                let inner_ty = self.resolve_buffer_element_type(
                    inner,
                    span,
                    "buffer<T> requires a POD scalar, pointer, or packed class element type",
                )?;
                Ok(PhpType::Buffer(Box::new(inner_ty)))
            }
            crate::parser::ast::TypeExpr::Named(name) => {
                let name_str = name.as_str();
                if let Some(canonical) = canonical_builtin_iterable_interface_name(name_str) {
                    return Ok(PhpType::Object(canonical.to_string()));
                }
                match name_str.to_ascii_lowercase().as_str() {
                    "string" => Ok(PhpType::Str),
                    "mixed" => Ok(PhpType::Mixed),
                    "callable" => Ok(PhpType::Callable),
                    "closure" => Ok(PhpType::Callable),
                    "object" => Ok(PhpType::Object(String::new())),
                    "void" | "null" => Ok(PhpType::Void),
                    // User PHP declarations accept packed and hash arrays without fixing their
                    // storage from a default or the first caller. Compiler-synthesized methods
                    // have explicit internal array ABIs and keep their existing storage type.
                    "array" if span.line != 0 => Ok(PhpType::php_array()),
                    "array" => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
                    // Relative class types only survive to this point when used outside a class
                    // body; inside a class they are rewritten to the enclosing class beforehand.
                    relative @ ("self" | "static" | "parent") => Err(CompileError::new(
                        span,
                        &format!("Cannot use '{}' as a type outside of a class", relative),
                    )),
                    _ if self.classes.contains_key(name_str)
                        || self.declared_classes.contains(name_str)
                        || self.interfaces.contains_key(name_str)
                        || self.declared_interfaces.contains(name_str)
                        || self.extern_classes.contains_key(name_str) =>
                    {
                        Ok(PhpType::Object(name_str.to_string()))
                    }
                    _ if self.packed_classes.contains_key(name_str) => {
                        Ok(PhpType::Packed(name_str.to_string()))
                    }
                    _ => Err(CompileError::new(
                        span,
                        &format!("Unknown type: {}", name_str),
                    )),
                }
            },
        }
    }

    /// Looks up the `PhpType` of an `extern` class field by `class_name` and `field_name`.
    /// Returns `None` if the class or field does not exist.
    pub(crate) fn extern_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.extern_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    /// Looks up the `PhpType` of a `packed class` field by `class_name` and `field_name`.
    /// Returns `None` if the class or field does not exist.
    pub(crate) fn packed_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.packed_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    /// Validates that `ty` is a pointer type. Emits an error with `context` if not.
    pub(crate) fn ensure_pointer_type(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::is_pointer_type(ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} requires a pointer argument", context),
            ))
        }
    }

    /// Validates that `ty` is a valid word-pointer value for `ptr_set()`: must be `Int`,
    /// `Bool`, `Void`, `Pointer`, or boxed runtime `Mixed`.
    ///
    /// Checked integer arithmetic is typed as `Mixed` because it may overflow to a float,
    /// but pointer word writes still need to accept the non-overflowing int payload path.
    pub(crate) fn ensure_word_pointer_value(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(
            ty,
            PhpType::Int
                | PhpType::Bool
                | PhpType::Void
                | PhpType::Pointer(_)
                | PhpType::Mixed
                | PhpType::Union(_)
        ) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                "ptr_set() value must be int, bool, null, or pointer",
            ))
        }
    }
}
