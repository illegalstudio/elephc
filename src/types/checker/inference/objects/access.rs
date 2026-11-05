//! Purpose:
//! Infers object access expression types.
//! Validates class, method, constructor, property, and magic-access contracts against schema metadata.
//!
//! Called from:
//! - `crate::types::checker::inference::objects`
//!
//! Key details:
//! - Object inference depends on flattened class metadata, visibility, inheritance, and declared property types.

use crate::errors::{CompileError, RuntimeFault};
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Infers the type of a property access expression (`$obj->prop`).
    ///
    /// Returns the declared property type on class/object, handles `Mixed`
    /// receivers (returning `Mixed`), and emits an error for non-object types.
    /// For nullable unions resolved to a single class, returns a nullable type.
    pub(crate) fn infer_property_access_type(
        &mut self,
        object: &Expr,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Flow-narrowing: a ternary like `$var->prop instanceof X ? ... : ...` records the
        // narrowed type of a simple property access under a synthetic env key (see
        // `branch_guard_narrowing`). Consult it first so a guarded branch sees the narrowed type.
        if let Some(key) = Self::narrowed_property_env_key(object, property) {
            if let Some(narrowed) = env.get(&key) {
                if let Ok(obj_ty) = self.infer_type(object, env) {
                    self.record_member_receiver_type(expr.span, &obj_ty);
                }
                return Ok(narrowed.clone());
            }
        }
        let receiver_is_flow_narrowed = self
            .flow_guard_env_key(object)
            .is_some_and(|key| env.contains_key(&key));
        let obj_ty = self.infer_type(object, env)?;
        self.record_member_receiver_type(expr.span, &obj_ty);
        // A `?ptr<T>` reaches its fields like `ptr<T>`: the value is the bare address, and a
        // null one faults with the null-pointer dereference fatal a null `ptr<T>` raises.
        let nullable_pointer = match &obj_ty {
            PhpType::Union(members) => crate::types::nullable_pointer_union_member(members),
            _ => None,
        };
        let obj_ty = nullable_pointer.unwrap_or(obj_ty);
        if let PhpType::Object(class_name) = &obj_ty {
            let inferred =
                self.infer_property_on_class_type(class_name, property, object, expr);
            if inferred.is_err()
                && self.closure_depth > 0
                && matches!(object.kind, ExprKind::This)
            {
                // A method-nested closure initially captures the enclosing receiver, but PHP
                // can later rebind it to an unrelated class. Keep precise types for properties
                // that exist on the enclosing class; an otherwise-invalid `$this` member is a
                // runtime-shaped access whose validity and type depend on the bound receiver.
                return Ok(PhpType::Mixed);
            }
            let property_ty = inferred?;
            if receiver_is_flow_narrowed {
                self.flow_typed_property_accesses.insert(
                    (self.current_loop_storage_scope.clone(), expr.span),
                    property_ty.clone(),
                );
            }
            return Ok(property_ty);
        }
        // Non-nullsafe property access on a nullable / union object type is
        // allowed when the union resolves to a single object class.
        //   - `?Foo` / `Foo|null`: a null receiver emits a PHP-style warning and
        //     evaluates to null, so the inferred type stays nullable.
        //   - `Foo|false` (and other object-plus-scalar unions): dispatch on the
        //     single object class; a non-object runtime value faults like PHP, so
        //     the inferred type is the plain property type.
        if let PhpType::Union(_) = &obj_ty {
            if let Ok(Some((class_name, nullable))) =
                self.nullsafe_object_receiver(&obj_ty, expr, "property access")
            {
                let property_ty =
                    self.infer_property_on_class_type(&class_name, property, object, expr)?;
                return if nullable {
                    Ok(self.normalize_union_type(vec![property_ty, PhpType::Void]))
                } else {
                    Ok(property_ty)
                };
            }
            if let Some(class_name) = self.union_single_object_class(&obj_ty) {
                return self.infer_property_on_class_type(&class_name, property, object, expr);
            }
            // Union of two or more distinct object classes (`A|B`): the property
            // must exist on every object member; codegen dispatches on the runtime
            // class id and the result is the union of each member's property type.
            let object_classes = self.union_object_classes(&obj_ty);
            if object_classes.len() >= 2 {
                let mut property_types = Vec::with_capacity(object_classes.len());
                for class_name in &object_classes {
                    // One member lacking the property keeps the error's run-time-fault tag: an
                    // autoloaded file compiles the read through `runtime_faults`' by-name helper,
                    // where the other members answer it and the lacking one raises php's Warning
                    // (`isset($mapping->enumType)` over `JoinColumnMapping|FieldMapping|...` in
                    // Doctrine's `SchemaTool`, and `$AST->selectClause` over
                    // `SelectStatement|DeleteStatement`).
                    let property_ty =
                        self.infer_property_on_class_type(class_name, property, object, expr)?;
                    property_types.push(property_ty);
                }
                return Ok(self.normalize_union_type(property_types));
            }
            // No object class at all. A plain `->` read of a scalar union is a runtime-shaped
            // access, not a type error: php warns and reads null, and the union can be a
            // transient of the checker's own narrowing -- `$d instanceof BackedEnum &&
            // !is_string($d = $d->value)` re-reads `$d->value` after `$d` took the enum's
            // `int|string` value, and refusing it kept Route, RouteRegistrar and UrlGenerator
            // out of the compiled program. Only the nullsafe form keeps the strict diagnostic.
            if matches!(expr.kind, ExprKind::PropertyAccess { .. }) {
                return Ok(PhpType::Mixed);
            }
            self.nullsafe_object_receiver(&obj_ty, expr, "property access")?;
        }
        // A nested packed field is an embedded record: reading it yields the record's ADDRESS,
        // typed exactly like a packed buffer element, so `$b[0]->pos->x` resolves the inner
        // field from the outer one's compile-time offset.
        let obj_ty = match obj_ty {
            PhpType::Packed(class_name) => PhpType::Pointer(Some(class_name)),
            other => other,
        };
        if let PhpType::Pointer(Some(class_name)) = &obj_ty {
            if let Some(field_ty) = self.extern_field_type(class_name, property) {
                return Ok(field_ty);
            }
            if let Some(field_ty) = self.packed_field_type(class_name, property) {
                return Ok(match field_ty {
                    PhpType::Packed(inner) => PhpType::Pointer(Some(inner)),
                    other => other,
                });
            }
            if self.extern_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined extern field: {}::{}", class_name, property),
                ));
            }
            if self.packed_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined packed field: {}::{}", class_name, property),
                ));
            }
        }
        // Mixed receivers fall through to runtime dispatch. The decoded
        // value may be a stdClass (e.g. from `json_decode($json)`), an
        // associative array, or a scalar — codegen unboxes the Mixed cell,
        // checks the tag, and routes object payloads through
        // `__rt_stdclass_get`. Non-object payloads return Mixed(null) at
        // runtime, mirroring PHP's "attempt to read property on non-object"
        // diagnostic for the most common idiom (`$obj->name` after
        // json_decode).
        let _ = property;
        if matches!(obj_ty, PhpType::Mixed) {
            return Ok(PhpType::Mixed);
        }
        // `isset($n->p)` / `$n->p ?? $d` reach through a null receiver in PHP and answer
        // `false` / the default; only a probe context may do so.
        if matches!(obj_ty, PhpType::Void) && self.null_probe_depth > 0 {
            return Ok(PhpType::Void);
        }
        Err(CompileError::new(
            expr.span,
            "Property access requires an object or typed pointer",
        ))
    }

    /// Infers the type of a nullsafe property access expression (`$obj?->prop`).
    ///
    /// For `Mixed` receivers returns `Mixed`. For valid nullable object unions,
    /// returns a union of the property type with `void`. Returns `void` for
    /// invalid receivers.
    pub(crate) fn infer_nullsafe_property_access_type(
        &mut self,
        object: &Expr,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        self.record_member_receiver_type(expr.span, &obj_ty);
        if matches!(obj_ty, PhpType::Mixed) {
            return Ok(PhpType::Mixed);
        }
        let Some((class_name, nullable)) =
            self.nullsafe_object_receiver(&obj_ty, expr, "property access")?
        else {
            return Ok(PhpType::Void);
        };
        let property_ty =
            self.infer_property_on_class_type(&class_name, property, object, expr)?;
        if nullable {
            Ok(self.normalize_union_type(vec![property_ty, PhpType::Void]))
        } else {
            Ok(property_ty)
        }
    }

    /// Infers the type of a dynamic property access expression (`$obj->$prop`).
    ///
    /// The property name expression must be `string`, `int`, or `Mixed`.
    /// Resolves string literals via `infer_property_access_type` or
    /// `infer_nullsafe_property_access_type`; returns `Mixed` for runtime
    /// dispatch on `Object`, `Union`, or `Mixed` receivers.
    pub(crate) fn infer_dynamic_property_access_type(
        &mut self,
        object: &Expr,
        property: &Expr,
        expr: &Expr,
        env: &TypeEnv,
        nullsafe: bool,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        self.record_member_receiver_type(expr.span, &obj_ty);
        if nullsafe && matches!(obj_ty, PhpType::Void) {
            return Ok(PhpType::Void);
        }

        let property_ty = self.infer_type(property, env)?;
        if !property_ty.is_dynamic_property_name() {
            return Err(CompileError::new(
                property.span,
                "Dynamic property name must be string or integer",
            ));
        }

        if let ExprKind::StringLiteral(name) = &property.kind {
            return if nullsafe {
                self.infer_nullsafe_property_access_type(object, name, expr, env)
            } else {
                self.infer_property_access_type(object, name, expr, env)
            };
        }

        match obj_ty {
            PhpType::Object(_) | PhpType::Union(_) | PhpType::Mixed => Ok(PhpType::Mixed),
            _ if nullsafe => {
                self.nullsafe_object_receiver(&obj_ty, expr, "property access")?;
                Ok(PhpType::Mixed)
            }
            _ => Err(CompileError::new(
                expr.span,
                "Property access requires an object or typed pointer",
            )),
        }
    }

    /// Resolves a property name against a known class's schema.
    ///
    /// Returns the property type after checking visibility, or errors on
    /// undefined properties. Returns `Mixed` for `stdClass` and classes
    /// marked `#[AllowDynamicProperties]`. Uses `__get` signature when
    /// no declared property matches.
    pub(crate) fn infer_property_on_class_type(
        &self,
        class_name: &str,
        property: &str,
        receiver: &Expr,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        if class_name.trim_start_matches('\\').is_empty() {
            return Ok(PhpType::Mixed);
        }
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            return Ok(PhpType::Mixed);
        }
        if let Some(property_ty) =
            crate::types::checker::reflection_virtual_property_type_in(class_name, property, |c| {
                self.classes.get(c).and_then(|info| info.parent.clone())
            })
        {
            return Ok(property_ty);
        }
        if let Some(property_ty) = self.enum_interface_property_type(class_name, property) {
            return Ok(property_ty);
        }
        if let Some(interface_info) = self.interfaces.get(class_name) {
            if let Some(property_ty) = interface_info
                .properties
                .get(property)
                .and_then(|contract| contract.get_type.clone())
            {
                return Ok(property_ty);
            }
            // The interface itself declares nothing, but an IMPLEMENTATION may — and a receiver
            // typed as the interface holds one of those at run time. Same treatment the class
            // branch gives a base whose descendant declares the property: the union of what they
            // declare, made nullable for the implementation that does not.
            //
            // `ReflectionCaster` reads `$m->name` off a value narrowed to `\Reflector`, which every
            // concrete Reflection class carries.
            let mut implementor_types =
                self.descendant_declared_property_types(class_name, property);
            // A Reflection implementation's `name` / `class` is a virtual property, which no slot
            // table lists: psysh's `SignatureFormatter` reads `$reflector->class` off a
            // `\Reflector` it switched on `get_class()`.
            for (implementor, _) in self.classes.iter() {
                if !self.class_implements_interface(implementor, class_name) {
                    continue;
                }
                if let Some(ty) = crate::types::checker::reflection_virtual_property_type_in(
                    implementor,
                    property,
                    |c| self.classes.get(c).and_then(|info| info.parent.clone()),
                ) {
                    if !implementor_types.contains(&ty) {
                        implementor_types.push(ty);
                    }
                }
            }
            if !implementor_types.is_empty() {
                let mut members = implementor_types;
                members.push(PhpType::Void);
                return Ok(self.normalize_union_type(members));
            }
            // An implementation that answers undeclared reads itself (`__get`, or
            // `#[\AllowDynamicProperties]`) makes the read the run-time object's to resolve:
            // Carbon's language files read `$date->dayOfWeek` off a `CarbonInterface`, which only
            // `Carbon::__get` serves. The value is whatever that hook returns. `CarbonInterface`
            // declares `__get` itself, which answers even when no implementation is compiled
            // (Carbon's class file can be deferred to the autoloader).
            let interface_declares_get = interface_info.methods.contains_key("__get");
            if interface_declares_get
                || self.descendant_answers_undeclared_reads(class_name)
                || self.null_probe_depth > 0
            {
                return Ok(PhpType::Mixed);
            }
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined property: {}::{}", class_name, property),
            ));
        }
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(visibility) = class_info.property_visibilities.get(property) {
                // `property_visibilities` answers for the PHYSICAL slot table, which still carries
                // a strict ancestor's private slot under its plain name. php 7.4 removed shadow
                // properties, so outside the class that declared it that name is a DYNAMIC
                // property: a read warns `Undefined property` and answers null rather than being
                // an access error. `resolve_property_name` is the authority for which of the two
                // this is, so the refusal below stays exactly where php raises it.
                let resolution = crate::types::resolve_property_name(
                    &self.classes,
                    class_name,
                    property,
                    self.current_class.as_deref(),
                );
                if resolution == crate::types::PropertyNameResolution::Dynamic {
                    return Ok(PhpType::Mixed);
                }
                let declaring_class = class_info
                    .property_declaring_classes
                    .get(property)
                    .map(String::as_str)
                    .unwrap_or(class_name);
                let inaccessible_dead_this_branch = matches!(visibility, crate::parser::ast::Visibility::Protected)
                    && matches!(receiver.kind, ExprKind::This)
                    && self.current_class.as_deref().is_some_and(|current| {
                        self.classes_are_instanceof_incompatible(current, declaring_class)
                    });
                if !self.can_access_member(declaring_class, visibility)
                    && !inaccessible_dead_this_branch
                {
                    // An inaccessible declared property is read through __get when the
                    // class provides that hook.  Do not expose the private slot type: the
                    // caller observes the magic result, not the hidden storage.
                    if let Some(sig) = class_info.methods.get("__get") {
                        return Ok(sig.return_type.clone());
                    }
                    return Err(CompileError::new(
                        expr.span,
                        &format!(
                            "Cannot access {} property: {}::{}",
                            Self::visibility_label(visibility),
                            class_name,
                            property
                        ),
                    ));
                }
            }
            if let Some(ty) =
                self.spl_callback_filter_runtime_property_type(class_name, property)
            {
                return Ok(ty);
            }
            if let Some((_, (_, ty))) = class_info.visible_property(property) {
                return Ok(ty.clone());
            }
            if super::super::super::scope_dynamic_storage::magic_set_reentry_property_is_readable(
                self, class_name, property,
            ) {
                return Ok(PhpType::Mixed);
            }
            // A name this program creates on the class (php 8.2+'s deprecated dynamic property)
            // is read back from the instance's hash, unless `__get` answers undeclared reads.
            if !class_info.methods.contains_key("__get")
                && super::super::super::scope_dynamic_storage::created_property_is_readable(
                    self, class_name, property,
                )
            {
                return Ok(PhpType::Mixed);
            }
            // A class that also allows dynamic properties stores some names itself, and php reads
            // an existing dynamic property without calling `__get`: guzzle's `FnStream` keeps
            // its `_fn_*` callables that way behind a throwing `__get(): void`.
            if class_info.allow_dynamic_properties && class_info.methods.contains_key("__get") {
                return Ok(PhpType::Mixed);
            }
            if let Some(sig) = class_info.methods.get("__get") {
                return Ok(sig.return_type.clone());
            }
            if class_info.allow_dynamic_properties {
                // PHP 8.2 #[\AllowDynamicProperties]: undeclared property
                // reads are dispatched to the side-table hashtable; the
                // value is statically `Mixed` because we cannot infer it.
                return Ok(PhpType::Mixed);
            }
            // A class that does not declare the property, where a DESCENDANT does. The runtime
            // receiver may be any of them, so the read's type is what they declare, made nullable
            // for the case where it really is the base (PHP's undefined-property warning answers
            // null there). Returning a bare `null` folded the whole access away and handed back
            // null for an object that held a value — silently.
            //
            // `$event->controllerMetadata?->getAttributes('*')` over Symfony's `KernelEvent`
            // hierarchy is the shape: only `ControllerArgumentsEvent` declares the property, and
            // every caller passes one.
            let descendant_types = self.descendant_declared_property_types(class_name, property);
            if !descendant_types.is_empty() {
                let mut members = descendant_types;
                members.push(PhpType::Void);
                return Ok(self.normalize_union_type(members));
            }
            // A descendant that answers undeclared reads itself: `DateTime::modify()` and
            // `setDate()` are declared to return `DateTime`, and Carbon's `Modifiers` trait reads
            // `$date->month` off that, which `Carbon::__get` serves at run time.
            if self.descendant_answers_undeclared_reads(class_name) {
                return Ok(PhpType::Mixed);
            }
            if self.null_probe_depth > 0 {
                return Ok(PhpType::Void);
            }
            // A read php answers with a Warning and null when it runs, judged against this
            // class and the descendants the checker knows; see `RuntimeFault::UndefinedProperty`.
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined property: {}::{}", class_name, property),
            )
            .with_runtime_fault(RuntimeFault::UndefinedProperty {
                class: class_name.to_string(),
                property: property.to_string(),
            }));
        }
        if self.allows_absent_runtime_class() {
            // PHP permits a declaration to name an absent class. In a function-like body that
            // class can remain optional until the branch constructing an instance is executed;
            // no class schema exists yet, so preserve the access as a gradual runtime dispatch.
            return Ok(PhpType::Mixed);
        }
        if self.awaits_generic_instantiation(class_name) {
            // Resolved by this round, spliced by the next one. `Mixed` keeps the rest of this
            // round meaningful; the next sees an ordinary class and types the read properly.
            return Ok(PhpType::Mixed);
        }
        // A closed world holds every class of an optional dependency's bridge, and only the class
        // it needs can be missing (`AmpClientState` over an uninstalled amphp): php never reaches
        // this read once loading that class has failed.
        if crate::closed_world::is_enabled() {
            return Ok(PhpType::Mixed);
        }
        Err(CompileError::new(
            expr.span,
            &format!("Undefined class: {}", class_name),
        ))
    }

    /// Returns the readonly property type guaranteed by PHP's enum interfaces.
    ///
    /// Enum case objects carry these properties even though reflection does not expose them as
    /// declarations on the interfaces. Descendant interfaces retain the same guarantee because
    /// only enum declarations may implement this hierarchy.
    fn enum_interface_property_type(
        &self,
        interface_name: &str,
        property: &str,
    ) -> Option<PhpType> {
        let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
        let is_unit_enum = interface_key == php_symbol_key("UnitEnum")
            || self.interface_extends_interface(interface_name, "UnitEnum");
        if is_unit_enum && property == "name" {
            return Some(PhpType::Str);
        }
        let is_backed_enum = interface_key == php_symbol_key("BackedEnum")
            || self.interface_extends_interface(interface_name, "BackedEnum");
        if is_backed_enum && property == "value" {
            return Some(PhpType::Union(vec![PhpType::Int, PhpType::Str]));
        }
        None
    }

    /// Returns precise SPL runtime storage metadata for callback-filter internals.
    fn spl_callback_filter_runtime_property_type(
        &self,
        class_name: &str,
        property: &str,
    ) -> Option<PhpType> {
        let class_name = class_name.trim_start_matches('\\');
        if class_name != "CallbackFilterIterator"
            && !self.is_subclass_of(class_name, "CallbackFilterIterator")
        {
            return None;
        }
        match property {
            "callback" => Some(PhpType::Callable),
            "callbackEnv" => Some(PhpType::Pointer(None)),
            _ => None,
        }
    }

    /// Extracts the single class name from a nullable object type for nullsafe ops.
    ///
    /// Returns `None` for `void`. On `Union` types, validates that exactly one
    /// class is present alongside optional `void`s; errors on mixed non-object
    /// union members. The `bool` in the result indicates whether `void` was
    /// present (i.e. whether the original type was nullable).
    pub(crate) fn nullsafe_object_receiver(
        &self,
        obj_ty: &PhpType,
        expr: &Expr,
        context: &str,
    ) -> Result<Option<(String, bool)>, CompileError> {
        match obj_ty {
            PhpType::Void => Ok(None),
            PhpType::Object(class_name) => Ok(Some((class_name.clone(), false))),
            PhpType::Union(members) => {
                let mut class_name = None;
                let mut nullable = false;
                for member in members {
                    // `?Closure` is typed `callable|null`; its object is a `Closure`.
                    let closure_class = PhpType::Object("Closure".to_string());
                    let member = if matches!(member, PhpType::Callable) {
                        &closure_class
                    } else {
                        member
                    };
                    match member {
                        PhpType::Void => nullable = true,
                        PhpType::Object(candidate) => {
                            if class_name
                                .as_ref()
                                .is_some_and(|existing: &String| existing != candidate)
                            {
                                return Err(CompileError::new(
                                    expr.span,
                                    &format!(
                                        "Nullsafe {} requires a single nullable object type, got {}",
                                        context, obj_ty
                                    ),
                                ));
                            }
                            class_name = Some(candidate.clone());
                        }
                        _ => {
                            return Err(CompileError::new(
                                expr.span,
                                &format!(
                                    "Nullsafe {} requires an object or null, got {}",
                                    context, obj_ty
                                ),
                            ));
                        }
                    }
                }
                Ok(class_name.map(|name| (name, nullable)))
            }
            _ => Err(CompileError::new(
                expr.span,
                &format!(
                    "Nullsafe {} requires an object or null, got {}",
                    context, obj_ty
                ),
            )),
        }
    }

    /// Returns the single distinct object class in a union receiver, ignoring any
    /// non-object members (scalars, `void`).
    ///
    /// Returns `None` when the type is not a union, or the union has zero or more
    /// than one distinct object class. Used to allow regular `->` method calls on
    /// unions such as `Foo|false`: codegen dispatches on the runtime class id and
    /// faults like PHP when the value is not an object, so the checker only needs
    /// the single object class to surface the method's return type.
    pub(crate) fn union_single_object_class(&self, obj_ty: &PhpType) -> Option<String> {
        let PhpType::Union(members) = obj_ty else {
            return None;
        };
        let mut found: Option<String> = None;
        for member in members {
            if let PhpType::Object(name) = member {
                match &found {
                    Some(existing) if existing != name => return None,
                    _ => found = Some(name.clone()),
                }
            }
        }
        found
    }

    /// Returns every distinct object class in a union receiver, in declaration
    /// order, ignoring non-object members (scalars, `void`).
    ///
    /// Used to allow regular `->` method/property access on a union of two or
    /// more distinct object classes (`A|B`, `A|B|false`): codegen unboxes the
    /// receiver and dispatches on the runtime class id (`lower_mixed_method_call`
    /// and the Mixed property path), so the checker only needs the set of object
    /// classes to validate the member and merge the per-class result types.
    pub(crate) fn union_object_classes(&self, obj_ty: &PhpType) -> Vec<String> {
        let PhpType::Union(members) = obj_ty else {
            return Vec::new();
        };
        let mut classes: Vec<String> = Vec::new();
        for member in members {
            if let PhpType::Object(name) = member {
                if !classes.iter().any(|existing| existing == name) {
                    classes.push(name.clone());
                }
            }
        }
        classes
    }

    /// Infers the type of a static property access (`Foo::$prop`).
    ///
    /// Resolves the static receiver (named, `self::`, `static::`, `parent::`)
    /// to a class name, then looks up the declared static property type after
    /// validating visibility rules.
    ///
    /// A flow narrowing recorded for the same place (`self::$p === null` guards,
    /// `self::$p = <non-null>` writes) wins over the declared type, the same way
    /// `infer_property_access_type` consults its instance-property key.
    pub(crate) fn infer_static_property_access_type(
        &mut self,
        receiver: &StaticReceiver,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        if let Some(key) = self.narrowed_static_property_env_key(receiver, property, expr) {
            if let Some(narrowed) = env.get(&key) {
                return Ok(narrowed.clone());
            }
        }
        let class_name = self.resolve_static_property_receiver(receiver, expr)?;
        let Some(class_info) = self.classes.get(&class_name) else {
            if self.eval_barrier_active && matches!(receiver, StaticReceiver::Named(_)) {
                return Ok(PhpType::Mixed);
            }
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined class: {}", class_name),
            )
            .with_runtime_fault(RuntimeFault::UnknownClass {
                class: class_name.clone(),
            }));
        };
        if let Some(visibility) = class_info.static_property_visibilities.get(property) {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if !self.can_access_member(declaring_class, visibility) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Cannot access {} static property: {}::{}",
                        Self::visibility_label(visibility),
                        class_name,
                        property
                    ),
                ));
            }
        }
        // `isset(self::$p)` / `self::$p ?? $d` on a class without `$p` (Symfony's
        // `HttpClientTrait` in a client that declares no `$emptyDefaults`) is php's plain `false`:
        // the probe reads as null, and lowering answers it without touching storage.
        let declared = class_info.static_properties.iter().any(|(name, _)| name == property);        if !declared && self.null_probe_depth > 0 && !self.eval_barrier_active {
            return Ok(PhpType::Void);
        }
        class_info
            .static_properties
            .iter()
            .find(|(name, _)| name == property)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| {
                let error = CompileError::new(
                    expr.span,
                    &format!("Undefined static property: {}::{}", class_name, property),
                );
                // `isset(self::$p)` answers false in php rather than throwing, so only a plain
                // read is the `Error` the fault names.
                if self.null_probe_depth > 0 {
                    return error;
                }
                error.with_runtime_fault(RuntimeFault::UndefinedStaticProperty {
                    class: class_name.clone(),
                    property: property.to_string(),
                })
            })
    }

    /// Infers a static-property read whose property name is computed at runtime.
    pub(crate) fn infer_dynamic_static_property_access_type(
        &mut self,
        receiver: &StaticReceiver,
        property: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let name_ty = self.infer_type(property, env)?;
        if !matches!(name_ty, PhpType::Str | PhpType::Mixed | PhpType::Int) {
            return Err(CompileError::new(
                property.span,
                &format!("Dynamic static property name must be a string, got `{name_ty}`"),
            ));
        }
        let class_name = self.resolve_static_property_receiver(receiver, expr)?;
        let Some(class_info) = self.classes.get(&class_name) else {
            return Err(CompileError::new(
                expr.span,
                "Dynamic static property access requires a statically-known class",
            ));
        };
        let mut types = class_info.static_properties.iter().map(|(_, ty)| ty.clone());
        let Some(first) = types.next() else {
            return Err(CompileError::new(
                expr.span,
                &format!("Class {class_name} has no static properties"),
            ));
        };
        if types.all(|ty| ty == first) {
            Ok(first)
        } else {
            Ok(PhpType::Mixed)
        }
    }

    /// Resolves a static property receiver to its class name.
    ///
    /// `Named` returns the class directly. `Self_`/`Static` require a class
    /// context. `Parent` returns the parent of the current class.
    pub(crate) fn resolve_static_property_receiver(
        &self,
        receiver: &StaticReceiver,
        expr: &Expr,
    ) -> Result<String, CompileError> {
        // A receiver written `Box<int>::of()` names `Box` until instantiation renames
        // it, and this pass can run on a generic function's template body — which is
        // walked and then stripped, never instantiated.
        let receiver = &receiver.written_class_receiver();
        match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(class_name) => Ok(class_name.as_str().to_string()),
            StaticReceiver::Self_ => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use self:: outside class method scope")
            }),
            StaticReceiver::Static => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use static:: outside class method scope")
            }),
            StaticReceiver::Parent => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(expr.span, "Cannot use parent:: outside class method scope")
                })?;
                let current_info = self.classes.get(current_class).ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined class: {}", current_class))
                })?;
                current_info.parent.as_ref().cloned().ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })
            }
        }
    }

    /// Infers the type of `$this` inside a class method.
    ///
    /// Errors for a direct static-method use or outside an object-capable closure context.
    /// Returns the flow-narrowed receiver type when an `instanceof` guard proves one, otherwise
    /// a concrete current class or a gradual receiver for closures that may be bound later.
    pub(crate) fn infer_this_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        if let Some(narrowed) = env.get(Self::narrowed_this_env_key()) {
            return Ok(narrowed.clone());
        }
        if self.current_method_is_static {
            if self.closure_depth == 0 {
                return Err(CompileError::new(
                    expr.span,
                    "Cannot use $this inside a static method",
                ));
            }
            // A non-static closure created by a static method can receive `$this`
            // later through `Closure::bind`, `bindTo`, or `call`. Its eventual
            // receiver class is unrelated to the lexical class.
            return Ok(PhpType::Mixed);
        }
        if let Some(class_name) = &self.current_class {
            Ok(PhpType::Object(class_name.clone()))
        } else if self.closure_depth > 0 {
            // A non-static closure defined outside a class method may still use
            // `$this` when it is later bound to an object via `Closure::bind` /
            // `bindTo`. The bound class is unknown here, so `$this` is a
            // runtime-dispatched receiver (`Mixed`).
            Ok(PhpType::Mixed)
        } else {
            Err(CompileError::new(
                expr.span,
                "Cannot use $this outside of a class method",
            ))
        }
    }

    /// Infers the type of a `ptr_cast<T>()` expression.
    ///
    /// Validates the inner expression is a pointer type, normalizes the target
    /// type string, and returns `PhpType::Pointer(Some(normalized))`.
    pub(crate) fn infer_ptr_cast_type(
        &mut self,
        target_type: &str,
        inner: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let inner_ty = self.infer_type(inner, env)?;
        self.ensure_pointer_type(&inner_ty, expr.span, "ptr_cast()")?;
        let normalized = self
            .normalize_pointer_target_type(target_type)
            .ok_or_else(|| {
                CompileError::new(
                    expr.span,
                    &format!("Unknown ptr_cast target type: {}", target_type),
                )
            })?;
        Ok(PhpType::Pointer(Some(normalized)))
    }
}

impl Checker {
    /// Collects the declared types of `property` on every class descending from `ancestor`.
    ///
    /// Used when the ancestor itself declares no such property: the receiver can only be one of
    /// these descendants, so their declarations are the answer, rather than the `null` an
    /// undeclared read would otherwise produce.
    ///
    /// Duplicates are dropped so a property declared identically down a long hierarchy does not
    /// turn into a union of one type repeated.
    /// Returns whether some class below `ancestor` serves undeclared property reads itself: it
    /// has a `__get` (own or inherited) or allows dynamic properties.
    pub(crate) fn descendant_answers_undeclared_reads(&self, ancestor: &str) -> bool {
        self.classes.iter().any(|(name, info)| {
            name.as_str() != ancestor
                && (info.methods.contains_key("__get") || info.allow_dynamic_properties)
                && self.class_descends_from(name, ancestor)
        })
    }

    pub(crate) fn descendant_declared_property_types(
        &self,
        ancestor: &str,
        property: &str,
    ) -> Vec<PhpType> {
        let mut types: Vec<PhpType> = Vec::new();
        for name in self.descendant_class_names(ancestor).iter() {
            let Some(info) = self.classes.get(name) else {
                continue;
            };
            let Some((_, (_, ty))) = info.visible_property(property) else {
                continue;
            };
            if !types.contains(ty) {
                types.push(ty.clone());
            }
        }
        // A hierarchy usually REDECLARES the property with a narrower class — Symfony's
        // `ControllerArgumentsMetadata extends ControllerMetadata` is the shape — and listing
        // both would build a union of two object classes where one already describes the other.
        // Keeping only the most general leaves a single object class, which is what a nullsafe
        // call and a method dispatch can each resolve.
        let general: Vec<PhpType> = types
            .iter()
            .filter(|candidate| {
                let Some(candidate_class) = Self::single_object_class_name(candidate) else {
                    return true;
                };
                !types.iter().any(|other| {
                    Self::single_object_class_name(other).is_some_and(|other_class| {
                        other_class != candidate_class
                            && self.class_descends_from(candidate_class, other_class)
                    })
                })
            })
            .cloned()
            .collect();
        general
    }

    /// Names the one object class a declared property type describes, if it describes exactly one.
    ///
    /// A nullable declaration (`?Foo`) still describes one class; a union of two unrelated classes
    /// describes none, and is left alone by the caller's collapse.
    fn single_object_class_name(ty: &PhpType) -> Option<&str> {
        match ty {
            PhpType::Object(name) if !name.is_empty() => Some(name.as_str()),
            PhpType::Union(members) => {
                let mut found = None;
                for member in members {
                    match member {
                        PhpType::Object(name) if !name.is_empty() => {
                            if found.is_some() {
                                return None;
                            }
                            found = Some(name.as_str());
                        }
                        PhpType::Void => {}
                        _ => return None,
                    }
                }
                found
            }
            _ => None,
        }
    }

    /// Walks a class's parent chain looking for `ancestor`.
    /// The classes other than `ancestor` that extend or implement it, SORTED: the class table is
    /// a hash map and `descendant_declared_property_types` builds a union from them, whose
    /// spelling reaches the emitted assembly (`compiler_determinism_tests`). Memoized per table
    /// size, as the hierarchy is fixed once the table is built.
    fn descendant_class_names(&self, ancestor: &str) -> std::sync::Arc<[String]> {
        {
            let mut cache = self.descendant_classes_cache.borrow_mut();
            if cache.0 != self.classes.len() {
                cache.0 = self.classes.len();
                cache.1.clear();
            }
            if let Some(names) = cache.1.get(ancestor) {
                return names.clone();
            }
        }
        #[cfg(test)]
        crate::types::checker::DESCENDANT_CLASS_SCANS.with(|count| count.set(count.get() + 1));
        let mut names: Vec<String> = self
            .classes
            .keys()
            .filter(|name| name.as_str() != ancestor && self.class_descends_from(name, ancestor))
            .cloned()
            .collect();
        names.sort();
        let names: std::sync::Arc<[String]> = names.into();
        self.descendant_classes_cache.borrow_mut().1.insert(ancestor.to_string(), names.clone());
        names
    }

    fn class_descends_from(&self, class_name: &str, ancestor: &str) -> bool {
        let mut current = Some(class_name);
        while let Some(name) = current {
            let Some(info) = self.classes.get(name) else {
                return false;
            };
            if name != class_name && name == ancestor {
                return true;
            }
            // Implementing an interface is descending from it for this purpose: the property the
            // caller reads is declared by the IMPLEMENTATION, which is exactly the class a
            // receiver typed as the interface can hold at run time.
            if info
                .interfaces
                .iter()
                .any(|implemented| self.interface_satisfies(implemented, ancestor))
            {
                return true;
            }
            current = info.parent.as_deref();
        }
        false
    }

    /// Whether an interface IS the named one, or extends it.
    fn interface_satisfies(&self, interface_name: &str, ancestor: &str) -> bool {
        if interface_name == ancestor {
            return true;
        }
        let Some(info) = self.interfaces.get(interface_name) else {
            return false;
        };
        info.parents
            .iter()
            .any(|parent| self.interface_satisfies(parent, ancestor))
    }
}
