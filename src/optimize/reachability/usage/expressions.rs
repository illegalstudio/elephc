//! Purpose:
//! Scans expression trees for declaration edges, builtin libraries, and dynamic lookup hazards.
//!
//! Called from:
//! - `crate::optimize::reachability::usage::Scanner` while walking statements and declarations.
//!
//! Key details:
//! - Local new-object facts refine ordinary method calls without weakening dynamic-call hazards.
//! - Registry-backed builtin requirements are recorded so checker libraries can be reconciled.

use std::collections::HashSet;

use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver, TypeExpr,
};
use crate::types::FunctionSig;

use super::{type_is_definitely_non_object, Scanner};

impl Scanner<'_> {
    /// Scans one expression and records all declaration edges it contains.
    pub(super) fn scan_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => self.scan_function_call(name.as_str(), args),
            ExprKind::FirstClassCallable(target) => self.scan_callable_target(target),
            ExprKind::ExprCall { callee, args } => {
                self.usage.hazards.dynamic_function = true;
                self.usage.hazards.dynamic_method = true;
                self.scan_expr(callee); self.scan_exprs(args);
            }
            ExprKind::ClosureCall { args, .. } => {
                self.usage.hazards.dynamic_function = true;
                self.usage.hazards.dynamic_method = true;
                self.scan_exprs(args);
            }
            ExprKind::NewObject { class_name, args } => {
                let key = self.record_class(class_name.as_str());
                self.usage.instantiated_classes.insert(key.clone());
                if key == php_symbol_key("IteratorIterator") {
                    if let Some(source) = args.first().map(unwrap_named_arg) {
                        self.record_iterator_protocol(source);
                    }
                }
                self.scan_method_signature_arguments(
                    &[key.clone()].into_iter().collect(),
                    "__construct",
                    false,
                    args,
                );
                self.scan_exprs(args);
            }
            ExprKind::NewDynamic { name_expr, args } => {
                self.usage.hazards.dynamic_class = true;
                self.scan_expr(name_expr);
                self.scan_exprs(args);
                self.scan_method_signature_arguments(
                    &HashSet::new(),
                    "__construct",
                    false,
                    args,
                );
            }
            ExprKind::NewDynamicObject { class_name, fallback_class, required_parent, args } => {
                self.usage.hazards.dynamic_class = true;
                self.record_class(fallback_class.as_str()); self.record_class(required_parent.as_str());
                self.scan_expr(class_name); self.scan_exprs(args);
                self.scan_method_signature_arguments(
                    &HashSet::new(),
                    "__construct",
                    false,
                    args,
                );
            }
            ExprKind::NewScopedObject { receiver, args } => {
                let lexical_class = self.receiver_class(receiver);
                let classes = if matches!(receiver, StaticReceiver::Static) {
                    lexical_class
                        .as_deref()
                        .and_then(|class| {
                            self.call_signatures
                                .map(|signatures| signatures.subclasses_including(class))
                        })
                        .filter(|classes| !classes.is_empty())
                        .unwrap_or_else(|| lexical_class.iter().cloned().collect())
                } else {
                    lexical_class.iter().cloned().collect()
                };
                if let Some(class) = lexical_class {
                    if matches!(receiver, StaticReceiver::Static) {
                        self.usage.instantiated_subclass_roots.insert(class);
                    }
                    self.usage.classes.extend(classes.iter().cloned());
                    self.usage
                        .instantiated_classes
                        .extend(classes.iter().cloned());
                } else if matches!(receiver, StaticReceiver::Parent) {
                    self.usage.hazards.dynamic_method = true;
                }
                self.scan_method_signature_arguments(&classes, "__construct", false, args);
                self.scan_exprs(args);
            }
            ExprKind::MethodCall { object, method, args }
            | ExprKind::NullsafeMethodCall { object, method, args } => {
                let classes = self.expr_classes(object);
                self.record_instance_method(object, method);
                self.scan_method_signature_arguments(&classes, method, false, args);
                self.scan_expr(object);
                self.scan_exprs(args);
            }
            ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
                self.usage.hazards.dynamic_method = true;
                self.scan_expr(object); self.scan_expr(method); self.scan_exprs(args);
            }
            ExprKind::StaticMethodCall { receiver, method, args } => {
                let classes = self.receiver_class(receiver).into_iter().collect();
                self.record_static_method(receiver, method);
                self.scan_method_signature_arguments(&classes, method, true, args);
                self.scan_exprs(args);
            }
            ExprKind::InstanceOf { value, target } => {
                self.scan_expr(value);
                match target { InstanceOfTarget::Name(name) => { self.record_class(name.as_str()); }, InstanceOfTarget::Expr(expr) => { self.usage.hazards.dynamic_class = true; self.scan_expr(expr); } }
            }
            ExprKind::ClassConstant { receiver } | ExprKind::ScopedConstantAccess { receiver, .. }
            | ExprKind::StaticPropertyAccess { receiver, .. } => self.scan_receiver(receiver),
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                self.record_instance_method(object, &property_hook_get_method(property));
                self.scan_expr(object);
            }
            ExprKind::ObjectClassName { object } => self.scan_expr(object),
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.usage.hazards.dynamic_method = true;
                self.scan_expr(object);
                self.scan_expr(property);
            }
            ExprKind::BinaryOp { left, right, .. } => { self.scan_expr(left); self.scan_expr(right); }
            ExprKind::Negate(e) | ExprKind::Not(e) | ExprKind::BitNot(e) | ExprKind::Throw(e)
            | ExprKind::Clone(e) | ExprKind::ErrorSuppress(e) | ExprKind::Print(e)
            | ExprKind::Spread(e) | ExprKind::Cast { expr: e, .. } | ExprKind::PtrCast { expr: e, .. }
            | ExprKind::YieldFrom(e) | ExprKind::IncludeValue { path: e, .. } => self.scan_expr(e),
            ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default }
            | ExprKind::Pipe { value, callable: default } => { self.scan_expr(value); self.scan_expr(default); }
            ExprKind::Assignment { target, value, result_target, prelude, .. } => {
                self.scan_assignment_target(target);
                self.scan_expr(target); self.scan_expr(value);
                if let ExprKind::Variable(name) = &target.kind { self.remember_assignment(name, value); }
                if let Some(target) = result_target { self.scan_expr(target); }
                self.scan_nested(prelude, true);
            }
            ExprKind::ArrayLiteral(values) => {
                if values.len() == 2
                    && matches!(unwrap_named_arg(&values[1]).kind, ExprKind::StringLiteral(_))
                {
                    self.scan_array_callable(values);
                }
                self.scan_exprs(values);
            }
            ExprKind::ArrayLiteralAssoc(values) => { for (key, value) in values { self.scan_expr(key); self.scan_expr(value); } }
            ExprKind::Match { subject, arms, default } => {
                self.scan_expr(subject);
                for (patterns, value) in arms { self.scan_exprs(patterns); self.scan_expr(value); }
                if let Some(default) = default { self.scan_expr(default); }
            }
            ExprKind::ArrayAccess { array, index } => {
                if matches!(&array.kind, ExprKind::Variable(name) if name == "GLOBALS") {
                    self.record_globals_index(index);
                } else {
                    self.record_protocol_methods(
                        array,
                        &["offsetExists", "offsetGet", "offsetSet", "offsetUnset"],
                    );
                }
                self.scan_expr(array);
                self.scan_expr(index);
            }
            ExprKind::Ternary { condition, then_expr, else_expr } => { self.scan_expr(condition); self.scan_expr(then_expr); self.scan_expr(else_expr); }
            ExprKind::Closure { params, variadic_type, return_type, body, .. } => {
                self.scan_params(params, variadic_type.as_ref(), return_type.as_ref()); self.scan_nested(body, true);
            }
            ExprKind::NamedArg { value, .. } => self.scan_expr(value),
            ExprKind::BufferNew { element_type, len } => { self.scan_type(element_type); self.scan_expr(len); }
            ExprKind::Yield { key, value } => { if let Some(key) = key { self.scan_expr(key); } if let Some(value) = value { self.scan_expr(value); } }
            ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
                self.forget_variable(name);
            }
            ExprKind::StringLiteral(name) => {
                if let Some((class, method)) = name.rsplit_once("::") {
                    let class = self.record_class(class);
                    self.usage
                        .methods
                        .insert((class, php_symbol_key(method), true));
                }
            }
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_) | ExprKind::BoolLiteral(_) | ExprKind::Null
            | ExprKind::ConstRef(_) | ExprKind::This
            | ExprKind::MagicConstant(_) => {}
        }
    }

    /// Scans a direct call, distinguishing literal introspection from dynamic hazards.
    pub(super) fn scan_function_call(&mut self, name: &str, args: &[Expr]) {
        let key = self.record_callable(name);
        self.record_builtin_requirements(name, args);
        self.scan_builtin_callback_arguments(&key, args);
        self.scan_declaration_arguments(&key, args);
        self.scan_function_signature_arguments(&key, args);
        // The backend lowering calls this private PDOStatement method directly. Keep the
        // reachability edge next to that lowering contract until it becomes registry metadata.
        if key == "__elephc_initialize_pdo_statement" {
            let class = self.record_class("PDOStatement");
            self.usage.methods.insert((
                class,
                php_symbol_key("__elephcInitialize"),
                false,
            ));
        }
        let normalized = self.normalized_function_arguments(&key, args);
        let first = normalized.first().map(unwrap_named_arg);
        match key.as_str() {
            "count" => {
                if let Some(receiver) = first {
                    self.record_protocol_methods(receiver, &["count"]);
                }
            }
            "iterator_apply" | "iterator_count" | "iterator_to_array" => {
                if let Some(receiver) = first {
                    self.record_iterator_protocol(receiver);
                }
            }
            "json_encode" => {
                self.usage
                    .wildcard_methods
                    .insert((php_symbol_key("jsonSerialize"), false));
            }
            "function_exists" => self.literal_function_or_hazard(first),
            "is_callable" | "call_user_func" | "call_user_func_array" => self.callable_or_hazard(first),
            "method_exists" => self.method_exists(args),
            "class_exists" | "interface_exists" | "enum_exists" | "trait_exists" => self.literal_class_or_hazard(first),
            "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" | "unserialize" => self.usage.hazards.dynamic_class = true,
            "eval" => {
                self.usage.hazards.dynamic_function = true;
                self.usage.hazards.dynamic_method = true;
                self.usage.hazards.dynamic_class = true;
            }
            _ => {}
        }
        self.scan_exprs(args);
    }

    /// Records class-like declarations and runtime protocols selected by builtin arguments.
    fn scan_declaration_arguments(&mut self, name: &str, args: &[Expr]) {
        match name {
            "class_attribute_args" | "class_attribute_names" | "class_get_attributes"
            | "ptr_sizeof" | "class_implements" | "class_parents" | "class_uses"
            | "get_parent_class" | "property_exists" | "spl_autoload" | "spl_autoload_call" => {
                self.scan_class_argument(name, args, 0, false);
            }
            "is_a" | "is_subclass_of" => {
                self.scan_class_argument(name, args, 0, false);
                self.scan_class_argument(name, args, 1, false);
            }
            "stream_wrapper_register" | "stream_filter_register" => {
                self.scan_class_argument(name, args, 1, true);
                self.usage.hazards.dynamic_method = true;
            }
            "__elephc_new_without_constructor" => {
                // `PDO::prepare` validates the driver-selected runtime class against
                // `PDOStatement` before this internal allocation. Keep that compatible
                // subclass family without rooting unrelated dynamically named classes.
                if self
                    .current_class
                    .as_deref()
                    .is_some_and(|class| php_symbol_key(class) == php_symbol_key("PDO"))
                    && self
                        .current_method
                        .as_deref()
                        .is_some_and(|method| php_symbol_key(method) == php_symbol_key("prepare"))
                {
                    self.usage
                        .instantiated_subclass_roots
                        .insert(php_symbol_key("PDOStatement"));
                } else {
                    self.scan_class_argument(name, args, 0, true);
                }
            }
            "__elephc_class_has_constructor" => {
                if let Some(class) = self.scan_class_argument(name, args, 0, false) {
                    self.usage.methods.insert((
                        class,
                        php_symbol_key("__construct"),
                        false,
                    ));
                } else {
                    self.usage
                        .wildcard_methods
                        .insert((php_symbol_key("__construct"), false));
                }
            }
            _ => {}
        }
    }

    /// Resolves one normalized class-name argument and optionally marks it runtime-instantiated.
    fn scan_class_argument(
        &mut self,
        function: &str,
        args: &[Expr],
        index: usize,
        instantiated: bool,
    ) -> Option<String> {
        let argument = self.normalized_function_arguments(function, args).get(index).cloned();
        let Some(argument) = argument.as_ref().map(unwrap_named_arg) else {
            self.usage.hazards.dynamic_class = true;
            return None;
        };
        let classes = self.expr_classes(argument);
        if classes.is_empty() {
            self.usage.hazards.dynamic_class = true;
            return None;
        }
        let mut first = None;
        for class in classes {
            self.usage.classes.insert(class.clone());
            if instantiated {
                self.usage.instantiated_classes.insert(class.clone());
            }
            if first.is_none() {
                first = Some(class);
            }
        }
        first
    }

    /// Records arguments bound to registry parameters recognized as callbacks.
    pub(super) fn scan_builtin_callback_arguments(&mut self, name: &str, args: &[Expr]) {
        let Some(definition) = crate::builtins::registry::lookup(name) else {
            return;
        };
        let callback_indices =
            crate::builtins::registry::callback_parameter_indices(definition);
        if callback_indices.is_empty() {
            return;
        }
        let Some(signature) = crate::builtins::registry::function_sig(name) else {
            return;
        };
        let call_span = args
            .first()
            .map(|arg| arg.span)
            .unwrap_or_else(crate::span::Span::dummy);
        let Ok(plan) = crate::types::call_args::plan_call_args(
            &signature,
            args,
            call_span,
            false,
            false,
        ) else {
            self.usage.hazards.dynamic_function = true;
            self.usage.hazards.dynamic_method = true;
            return;
        };
        let normalized = plan.normalized_args();
        for index in callback_indices {
            let Some(callback) = normalized.get(index).map(unwrap_named_arg) else {
                if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
                    self.usage.hazards.dynamic_function = true;
                    self.usage.hazards.dynamic_method = true;
                }
                continue;
            };
            match &callback.kind {
                ExprKind::Closure { .. } | ExprKind::Null => {}
                ExprKind::FirstClassCallable(target) => self.scan_callable_target(target),
                ExprKind::StringLiteral(_) | ExprKind::ArrayLiteral(_) => {
                    self.callable_or_hazard(Some(callback));
                }
                _ => {
                    self.usage.hazards.dynamic_function = true;
                    self.usage.hazards.dynamic_method = true;
                }
            }
        }
    }

    /// Records link libraries contributed by one registry-backed builtin call.
    pub(super) fn record_builtin_requirements(&mut self, name: &str, args: &[Expr]) {
        let Some(definition) = crate::builtins::registry::lookup(name) else {
            return;
        };
        let requirements = match definition.spec.semantics.requirements {
            crate::builtins::semantics::BuiltinRequirements::Static(requirements) => {
                requirements.to_vec()
            }
            crate::builtins::semantics::BuiltinRequirements::Shared(resolve) => {
                let normalized = self
                    .normalized_builtin_arguments(name, args)
                    .unwrap_or_else(|| args.to_vec());
                resolve(&crate::builtins::semantics::BuiltinRequirementInput {
                    args: &normalized,
                })
            }
        };
        for requirement in requirements {
            match requirement {
                crate::builtins::semantics::BuiltinRequirement::Bridge(library)
                | crate::builtins::semantics::BuiltinRequirement::SystemLibrary(library)
                | crate::builtins::semantics::BuiltinRequirement::MacOsLibrary(library) => {
                    self.usage.required_libraries.insert(library.to_string());
                }
                crate::builtins::semantics::BuiltinRequirement::RuntimeFeature(_) => {}
            }
        }
    }

    /// Records a literal free-function name or enables dynamic function reachability.
    pub(super) fn literal_function_or_hazard(&mut self, expr: Option<&Expr>) {
        if let Some(Expr { kind: ExprKind::StringLiteral(name), .. }) = expr {
            self.record_callable(name);
        } else { self.usage.hazards.dynamic_function = true; }
    }

    /// Records a literal callable descriptor or widens both callable hazard classes.
    pub(super) fn callable_or_hazard(&mut self, expr: Option<&Expr>) {
        match expr.map(|expr| &expr.kind) {
            Some(ExprKind::StringLiteral(name)) if name.contains("::") => {
                if let Some((class, method)) = name.rsplit_once("::") {
                    let class = self.record_class(class);
                    self.usage.methods.insert((class, php_symbol_key(method), true));
                }
            }
            Some(ExprKind::StringLiteral(name)) => { self.record_callable(name); }
            Some(ExprKind::ArrayLiteral(values)) if values.len() == 2 => self.scan_array_callable(values),
            _ => { self.usage.hazards.dynamic_function = true; self.usage.hazards.dynamic_method = true; }
        }
    }

    /// Scans one argument whose checker-validated parameter type permits a callable.
    fn scan_callable_argument(&mut self, argument: &Expr) {
        let argument = unwrap_named_arg(argument);
        match &argument.kind {
            ExprKind::Closure { .. } | ExprKind::Null => {}
            ExprKind::FirstClassCallable(target) => self.scan_callable_target(target),
            ExprKind::StringLiteral(_) | ExprKind::ArrayLiteral(_) => {
                self.callable_or_hazard(Some(argument));
            }
            _ => {
                self.usage.hazards.dynamic_function = true;
                self.usage.hazards.dynamic_method = true;
            }
        }
    }

    /// Records `method_exists` literal method probes without treating them as dynamic lookup.
    pub(super) fn method_exists(&mut self, args: &[Expr]) {
        let Some(normalized) = self.normalized_builtin_arguments("method_exists", args) else {
            self.usage.hazards.dynamic_class = true;
            self.usage.hazards.dynamic_method = true;
            return;
        };
        let Some(target) = normalized.first().map(unwrap_named_arg) else {
            self.usage.hazards.dynamic_class = true;
            self.usage.hazards.dynamic_method = true;
            return;
        };
        let classes = self.expr_classes(target);
        self.usage.classes.extend(classes.iter().cloned());
        let Some(method) = normalized.get(1).map(unwrap_named_arg) else {
            self.usage.hazards.dynamic_method = true;
            return;
        };
        let ExprKind::StringLiteral(method) = &method.kind else {
            self.usage.hazards.dynamic_method = true;
            if classes.is_empty() {
                self.usage.hazards.dynamic_class = true;
            }
            return;
        };
        let method = php_symbol_key(method);
        if classes.is_empty() {
            self.usage.wildcard_methods.insert((method.clone(), false));
            self.usage.wildcard_methods.insert((method, true));
            self.usage.hazards.dynamic_class = true;
        }
        else {
            for class in classes {
                self.usage
                    .methods
                    .insert((class.clone(), method.clone(), false));
                self.usage.methods.insert((class, method.clone(), true));
            }
        }
    }

    /// Records a literal class probe or enables dynamic class reachability.
    pub(super) fn literal_class_or_hazard(&mut self, expr: Option<&Expr>) {
        if let Some(Expr { kind: ExprKind::StringLiteral(name), .. }) = expr { self.record_class(name); }
        else { self.usage.hazards.dynamic_class = true; }
    }

    /// Scans one first-class callable target.
    pub(super) fn scan_callable_target(&mut self, target: &CallableTarget) {
        match target {
            CallableTarget::Function(name) => { self.record_callable(name.as_str()); }
            CallableTarget::StaticMethod { receiver, method } => self.record_static_method(receiver, method),
            CallableTarget::Method { object, method } => { self.record_instance_method(object, method); self.scan_expr(object); }
        }
    }

    /// Recognizes a two-element literal callable array and records its method target.
    pub(super) fn scan_array_callable(&mut self, values: &[Expr]) {
        if values.len() != 2 { return; }
        let ExprKind::StringLiteral(method) = &unwrap_named_arg(&values[1]).kind else {
            let receiver_classes = self.expr_classes(unwrap_named_arg(&values[0]));
            self.usage.classes.extend(receiver_classes);
            self.usage.hazards.dynamic_method = true;
            return;
        };
        let first = unwrap_named_arg(&values[0]);
        match &first.kind {
            ExprKind::StringLiteral(class) => { let class = self.record_class(class); self.usage.methods.insert((class, php_symbol_key(method), true)); }
            _ => {
                let classes = self.expr_classes(first);
                if classes.is_empty() {
                    let method = php_symbol_key(method);
                    self.usage.wildcard_methods.insert((method.clone(), false));
                    self.usage.wildcard_methods.insert((method, true));
                    self.usage.hazards.dynamic_class = true;
                }
                else {
                    let method = php_symbol_key(method);
                    for class in classes {
                        self.usage
                            .methods
                            .insert((class.clone(), method.clone(), false));
                        self.usage.methods.insert((class, method.clone(), true));
                    }
                }
            }
        }
    }

    /// Records one instance-method reference using local new-object facts when available.
    pub(super) fn record_instance_method(&mut self, object: &Expr, method: &str) {
        let method = php_symbol_key(method);
        if let ExprKind::Variable(variable) = &object.kind {
            let methods = self
                .usage
                .variable_methods
                .entry(variable.clone())
                .or_default();
            methods.insert((method.clone(), false));
            methods.insert((method.clone(), true));
        }
        let classes = self.expr_classes(object);
        if classes.is_empty() {
            self.usage.wildcard_methods.insert((method.clone(), false));
            self.usage.wildcard_methods.insert((method, true));
        } else {
            for class in classes {
                self.usage
                    .methods
                    .insert((class.clone(), method.clone(), false));
                self.usage.methods.insert((class, method.clone(), true));
            }
        }
    }

    /// Records implicit instance calls emitted for one compiler-managed runtime protocol.
    pub(super) fn record_protocol_methods(&mut self, receiver: &Expr, methods: &[&str]) {
        if !self.expr_may_be_object(receiver) {
            return;
        }
        self.record_protocol_methods_for_classes(self.expr_classes(receiver), methods);
    }

    /// Records implicit protocol calls for a variable-backed receiver without fabricating AST.
    pub(super) fn record_variable_protocol_methods(&mut self, variable: &str, methods: &[&str]) {
        if self
            .definitely_non_object_variables
            .contains(variable)
        {
            return;
        }
        let classes = self
            .variable_classes
            .get(variable)
            .cloned()
            .unwrap_or_default();
        self.record_protocol_methods_for_classes(classes, methods);
    }

    /// Adds exact or wildcard method edges for the resolved runtime-protocol receiver set.
    fn record_protocol_methods_for_classes(
        &mut self,
        classes: HashSet<String>,
        methods: &[&str],
    ) {
        for method in methods {
            let method = php_symbol_key(method);
            if classes.is_empty() {
                self.usage.wildcard_methods.insert((method, false));
            } else {
                self.usage.methods.extend(
                    classes
                        .iter()
                        .cloned()
                        .map(|class| (class, method.clone(), false)),
                );
            }
        }
    }

    /// Records the complete Iterator and IteratorAggregate call surface for one source.
    fn record_iterator_protocol(&mut self, receiver: &Expr) {
        self.record_protocol_methods(
            receiver,
            &["getIterator", "rewind", "valid", "current", "key", "next"],
        );
    }

    /// Records one static-method reference and its receiver class when statically known.
    pub(super) fn record_static_method(&mut self, receiver: &StaticReceiver, method: &str) {
        if let Some(class) = self.receiver_class(receiver) {
            self.usage.classes.insert(class.clone());
            let method = php_symbol_key(method);
            let methods = if matches!(receiver, StaticReceiver::Parent) {
                &mut self.usage.scoped_methods
            } else {
                &mut self.usage.methods
            };
            methods.insert((class.clone(), method.clone(), true));
            methods.insert((class, method, false));
        } else {
            let method = php_symbol_key(method);
            self.usage.wildcard_methods.insert((method.clone(), true));
            self.usage.wildcard_methods.insert((method, false));
        }
    }

    /// Scans a static receiver for a named class dependency.
    pub(super) fn scan_receiver(&mut self, receiver: &StaticReceiver) {
        if let Some(class) = self.receiver_class(receiver) { self.usage.classes.insert(class); }
    }

    /// Resolves a static receiver to its named, current, or immediate parent class.
    pub(super) fn receiver_class(&self, receiver: &StaticReceiver) -> Option<String> {
        match receiver {
            StaticReceiver::Named(name) => Some(self.class_key(name.as_str())),
            StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.as_deref().map(php_symbol_key),
            StaticReceiver::Parent => self.parent_class.as_deref().map(php_symbol_key),
        }
    }

    /// Unions statically evident receiver classes without reviving an opaque variable fact.
    pub(super) fn remember_assignment(&mut self, name: &str, value: &Expr) {
        self.definitely_non_object_variables.remove(name);
        let classes = self.expr_classes(value);
        if classes.is_empty() {
            self.forget_variable(name);
        } else if !self.invalidated_variables.contains(name) {
            self.variable_classes
                .entry(name.to_string())
                .or_default()
                .extend(classes);
        }
    }

    /// Records the declared receiver domain of a typed local even when its initializer is null.
    pub(super) fn remember_typed_assignment(
        &mut self,
        name: &str,
        type_expr: &TypeExpr,
        value: &Expr,
    ) {
        self.definitely_non_object_variables.remove(name);
        let mut classes = self.type_classes(type_expr);
        classes.extend(self.expr_classes(value));
        if classes.is_empty() {
            if type_is_definitely_non_object(type_expr) {
                self.variable_classes.remove(name);
                self.definitely_non_object_variables
                    .insert(name.to_string());
            } else {
                self.forget_variable(name);
            }
        } else if !self.invalidated_variables.contains(name) {
            self.variable_classes
                .entry(name.to_string())
                .or_default()
                .extend(classes);
        }
    }

    /// Permanently makes one local receiver opaque for the remainder of this conservative scan.
    pub(super) fn forget_variable(&mut self, name: &str) {
        self.variable_classes.remove(name);
        self.definitely_non_object_variables.remove(name);
        self.invalidated_variables.insert(name.to_string());
    }

    /// Returns whether an expression may carry an object that implements a runtime protocol.
    fn expr_may_be_object(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Variable(name) => !self.definitely_non_object_variables.contains(name),
            ExprKind::This
            | ExprKind::NewObject { .. }
            | ExprKind::NewDynamic { .. }
            | ExprKind::NewDynamicObject { .. }
            | ExprKind::NewScopedObject { .. } => true,
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self.expr_may_be_object(then_expr) || self.expr_may_be_object(else_expr),
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                self.expr_may_be_object(value) || self.expr_may_be_object(default)
            }
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ArrayLiteral(_)
            | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::ClassConstant { .. }
            | ExprKind::ObjectClassName { .. }
            | ExprKind::BufferNew { .. }
            | ExprKind::Cast { .. }
            | ExprKind::PtrCast { .. } => false,
            _ => true,
        }
    }

    /// Returns statically evident runtime classes for a narrow set of value expressions.
    pub(super) fn expr_classes(&self, expr: &Expr) -> HashSet<String> {
        match &expr.kind {
            ExprKind::Variable(name) => self.variable_classes.get(name).cloned().unwrap_or_default(),
            ExprKind::This => self.current_class.iter().map(|name| php_symbol_key(name)).collect(),
            ExprKind::NewObject { class_name, .. } => [php_symbol_key(class_name.as_str())].into_iter().collect(),
            ExprKind::StringLiteral(name) => [php_symbol_key(name.trim_start_matches('\\'))].into_iter().collect(),
            ExprKind::ClassConstant { receiver } => {
                self.receiver_class(receiver).into_iter().collect()
            }
            ExprKind::ObjectClassName { object } => self.expr_classes(object),
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                let owners = self.expr_classes(object);
                self.call_signatures
                    .map(|signatures| signatures.property(&owners, property))
                    .unwrap_or_default()
            }
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let mut classes = self.expr_classes(then_expr);
                classes.extend(self.expr_classes(else_expr));
                classes
            }
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                let mut classes = self.expr_classes(value);
                classes.extend(self.expr_classes(default));
                classes
            }
            _ => HashSet::new(),
        }
    }

    /// Returns concrete class/interface receiver candidates declared by a local type hint.
    pub(super) fn type_classes(&self, type_expr: &TypeExpr) -> HashSet<String> {
        match type_expr {
            TypeExpr::Named(name)
                if !matches!(
                    name.as_str().to_ascii_lowercase().as_str(),
                    "array" | "mixed" | "callable" | "object" | "void"
                ) => [self.class_key(name.as_str())].into_iter().collect(),
            TypeExpr::Nullable(inner) => self.type_classes(inner),
            TypeExpr::Union(types) | TypeExpr::Intersection(types) => types
                .iter()
                .flat_map(|type_expr| self.type_classes(type_expr))
                .collect(),
            _ => HashSet::new(),
        }
    }

    /// Applies callable and by-reference parameter effects of a direct function call.
    fn scan_function_signature_arguments(&mut self, function: &str, args: &[Expr]) {
        let signatures: Vec<_> = self
            .call_signatures
            .and_then(|signatures| signatures.function(function))
            .into_iter()
            .collect();
        self.scan_signature_arguments(&signatures, args);
    }

    /// Applies callable and by-reference parameter effects of a direct method call.
    fn scan_method_signature_arguments(
        &mut self,
        classes: &HashSet<String>,
        method: &str,
        is_static: bool,
        args: &[Expr],
    ) {
        let method = php_symbol_key(method);
        let signatures = self
            .call_signatures
            .map(|index| index.method(classes, &method, is_static))
            .unwrap_or_default();
        self.scan_signature_arguments(&signatures, args);
    }

    /// Applies every reachability-relevant parameter contract shared by calls.
    fn scan_signature_arguments(&mut self, signatures: &[FunctionSig], args: &[Expr]) {
        for signature in signatures {
            let callable_indices: Vec<_> = signature
                .params
                .iter()
                .enumerate()
                .filter_map(|(index, (_, ty))| php_type_may_be_callable(ty).then_some(index))
                .collect();
            if callable_indices.is_empty() {
                continue;
            }
            let normalized = self.normalized_arguments(signature, args);
            if normalized.is_empty() && !args.is_empty() {
                self.usage.hazards.dynamic_function = true;
                self.usage.hazards.dynamic_method = true;
                continue;
            }
            for index in callable_indices {
                if let Some(argument) = normalized.get(index) {
                    self.scan_callable_argument(argument);
                } else if args.iter().any(|argument| matches!(argument.kind, ExprKind::Spread(_))) {
                    self.usage.hazards.dynamic_function = true;
                    self.usage.hazards.dynamic_method = true;
                }
            }
        }
        self.invalidate_ref_arguments(signatures, args);
    }

    /// Normalizes one call against a known signature, preserving named and static-spread mapping.
    fn normalized_arguments(&self, signature: &FunctionSig, args: &[Expr]) -> Vec<Expr> {
        let call_span = args
            .first()
            .map(|argument| argument.span)
            .unwrap_or_else(crate::span::Span::dummy);
        crate::types::call_args::plan_call_args(signature, args, call_span, false, true)
            .map(|plan| plan.normalized_args())
            .unwrap_or_default()
    }

    /// Normalizes arguments using the checker, extern, or registry signature for a function.
    fn normalized_function_arguments(&self, function: &str, args: &[Expr]) -> Vec<Expr> {
        self.call_signatures
            .and_then(|signatures| signatures.function(function))
            .or_else(|| crate::builtins::registry::function_sig(function))
            .map(|signature| self.normalized_arguments(&signature, args))
            .unwrap_or_else(|| args.to_vec())
    }

    /// Normalizes one registry builtin exactly like checker-side builtin validation.
    fn normalized_builtin_arguments(&self, function: &str, args: &[Expr]) -> Option<Vec<Expr>> {
        let signature = crate::builtins::registry::function_sig(function)?;
        let call_span = args
            .first()
            .map(|argument| argument.span)
            .unwrap_or_else(crate::span::Span::dummy);
        crate::types::call_args::plan_call_args(
            &signature,
            args,
            call_span,
            true,
            false,
        )
        .ok()
        .map(|plan| plan.normalized_args())
    }

    /// Uses the shared argument planner to find storage that a reachable callable may rebind.
    fn invalidate_ref_arguments(&mut self, signatures: &[FunctionSig], args: &[Expr]) {
        let mut variables = HashSet::new();
        for signature in signatures {
            if !signature.ref_params.iter().any(|by_ref| *by_ref) {
                continue;
            }
            let call_span = args
                .first()
                .map(|argument| argument.span)
                .unwrap_or_else(crate::span::Span::dummy);
            let Ok(plan) = crate::types::call_args::plan_call_args(
                signature,
                args,
                call_span,
                false,
                true,
            ) else {
                self.usage.hazards.dynamic_method = true;
                continue;
            };
            for (index, argument) in plan.normalized_args().iter().enumerate() {
                if !signature
                    .ref_params
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| {
                        signature.variadic.is_some()
                            && signature.ref_params.last().copied().unwrap_or(false)
                    })
                {
                    continue;
                }
                if let ExprKind::Variable(variable) = &unwrap_named_arg(argument).kind {
                    variables.insert(variable.clone());
                }
            }
        }
        for variable in variables {
            self.forget_variable(&variable);
        }
    }

    /// Records a free-function/extern callable key.
    pub(super) fn record_callable(&mut self, name: &str) -> String {
        let key = php_symbol_key(name.trim_start_matches('\\'));
        self.usage.functions.insert(key.clone()); self.usage.externs.insert(key.clone()); key
    }

    /// Records a class key and applies conservative Reflection hazards.
    pub(super) fn record_class(&mut self, name: &str) -> String {
        let key = self.class_key(name);
        if key
            .rsplit('\\')
            .next()
            .is_some_and(|name| name.starts_with("reflection"))
        {
            self.usage.hazards.dynamic_function = true;
            self.usage.hazards.dynamic_method = true;
            self.usage.hazards.dynamic_class = true;
        }
        self.usage.classes.insert(key.clone()); key
    }

    /// Normalizes a named or scope-relative class reference in the current method context.
    pub(super) fn class_key(&self, name: &str) -> String {
        match name.trim_start_matches('\\').to_ascii_lowercase().as_str() {
            "self" | "static" => self.current_class.as_deref().map(php_symbol_key).unwrap_or_else(|| php_symbol_key(name)),
            "parent" => self.parent_class.as_deref().map(php_symbol_key).unwrap_or_else(|| php_symbol_key(name)),
            _ => php_symbol_key(name.trim_start_matches('\\')),
        }
    }

    /// Scans a slice of expressions in source order.
    pub(super) fn scan_exprs(&mut self, expressions: &[Expr]) {
        for expression in expressions { self.scan_expr(expression); }
    }

    /// Records setter hooks reachable through an assignment target before scanning its value shape.
    pub(super) fn scan_assignment_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                self.record_instance_method(object, &property_hook_set_method(property));
            }
            ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. } => {
                self.usage.hazards.dynamic_method = true;
            }
            ExprKind::ArrayAccess { array, .. } => self.scan_assignment_target(array),
            _ => {}
        }
    }

}

/// Returns whether a declared PHP parameter type can receive a callable descriptor.
fn php_type_may_be_callable(ty: &crate::types::PhpType) -> bool {
    match ty {
        crate::types::PhpType::Callable => true,
        crate::types::PhpType::Union(types) => types.iter().any(php_type_may_be_callable),
        _ => false,
    }
}

/// Removes a named-argument wrapper when inspecting builtin control arguments.
fn unwrap_named_arg(expr: &Expr) -> &Expr {
    if let ExprKind::NamedArg { value, .. } = &expr.kind { value } else { expr }
}

#[cfg(test)]
mod tests {
    const CALLBACK_BUILTINS: &[&str] = &[
        "array_all",
        "array_any",
        "array_filter",
        "array_find",
        "array_map",
        "array_reduce",
        "array_udiff",
        "array_uintersect",
        "array_walk",
        "array_walk_recursive",
        "call_user_func",
        "call_user_func_array",
        "iterator_apply",
        "ob_start",
        "preg_replace_callback",
        "spl_autoload_register",
        "spl_autoload_unregister",
        "uasort",
        "uksort",
        "usort",
    ];

    /// Verifies every callback-consuming AOT builtin exposes structural scanner metadata.
    #[test]
    fn callback_builtin_registry_contract_is_complete() {
        let mut registry_callbacks = crate::builtins::registry::names()
            .filter(|name| {
                crate::builtins::registry::lookup(name).is_some_and(|definition| {
                    !crate::builtins::registry::callback_parameter_indices(definition).is_empty()
                })
            })
            .collect::<Vec<_>>();
        registry_callbacks.sort_unstable();

        assert_eq!(
            registry_callbacks,
            CALLBACK_BUILTINS,
            "callback-taking builtins must expose structural callback metadata or a callable type",
        );
    }

    /// Verifies the legacy `$callback` convention cannot bypass structural callback metadata.
    #[test]
    fn callback_named_registry_parameters_are_declared() {
        for builtin_name in crate::builtins::registry::names() {
            let definition = crate::builtins::registry::lookup(builtin_name)
                .expect("registry names must resolve to builtin definitions");
            let callback_indices =
                crate::builtins::registry::callback_parameter_indices(definition);
            for (index, (parameter, _)) in definition.params.iter().enumerate() {
                if parameter == "callback" {
                    assert!(
                        callback_indices.contains(&index),
                        "{}() parameter ${parameter} must declare structural callback metadata or a callable type",
                        definition.name,
                    );
                }
            }
        }
    }
}
