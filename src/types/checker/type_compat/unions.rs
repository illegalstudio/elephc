//! Purpose:
//! Checks type compatibility for unions cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::types::PhpType;

use super::super::Checker;

/// Flattens nested unions, drops duplicates, lets `Mixed` absorb everything, and applies
/// PHP's `bool`/`false` subtype rule — `bool` already contains the literal `false`, so the
/// two never coexist as separate members.
///
/// Free-standing because the inferred-return fold in
/// `checker::functions::returns::wider_type` is an associated function with no `self` and
/// must build its unions through the SAME normalizer. Hand-rolled member pushing there
/// produced `false|string|bool`, whose redundant `False` made two structurally equal types
/// compare unequal — and `PhpType`/`FunctionSig` equality is what callable-return metadata
/// is keyed on.
pub(crate) fn normalize_union_members(members: Vec<PhpType>) -> PhpType {
    let mut flat = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => flat.extend(inner),
            PhpType::Mixed => return PhpType::Mixed,
            PhpType::Never => {}
            other => flat.push(other),
        }
    }

    let mut deduped: Vec<PhpType> = Vec::new();
    for member in flat {
        // `bool` already contains the literal `false` subtype. Keep `False` only when the
        // declaration is specifically false-only (for example `int|false`).
        if member == PhpType::False && deduped.iter().any(|existing| existing == &PhpType::Bool) {
            continue;
        }
        if member == PhpType::Bool {
            deduped.retain(|existing| existing != &PhpType::False);
        }
        if !deduped.iter().any(|existing| existing == &member) {
            deduped.push(member);
        }
    }

    if deduped.is_empty() {
        PhpType::Never
    } else if deduped.len() == 1 {
        deduped.pop().expect("union member exists")
    } else {
        PhpType::Union(deduped)
    }
}

impl Checker {
    /// Flattens nested unions, removes duplicates and `PhpType::Mixed` (which absorbs all),
    /// and returns a single `PhpType` or a `PhpType::Union` with deduped members.
    pub(crate) fn normalize_union_type(&self, members: Vec<PhpType>) -> PhpType {
        normalize_union_members(members)
    }

    /// Returns true if `expected` type can accept a value of `actual` type (i.e., the
    /// assignment `expected = actual` is valid). Checks identity, Mixed, unions, arrays,
    /// associative arrays, object class/interface compatibility, `iterable`, pointers,
    /// and resources.
    pub(crate) fn type_accepts(&self, expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual {
            return true;
        }
        // The arithmetic union is decided by the arms below, not here: this per-member rule asks
        // whether `int` accepts `float`, which is false, so it would refuse `int|float` at every
        // position — including the `int` parameters the preludes pass a loop counter to.
        // A union that CONTAINS `callable` is a value already PROVEN callable, whose runtime shape
        // is merely not decided: that is what `is_callable($x)` narrows an unconstrained value to
        // (`callable|string`), because the storage stays the boxed cell it was and a callable may
        // be a plain function-name string. Decomposing it member by member asks the wrong
        // question — `string` alone is not proven callable, so the `string` member would sink the
        // whole union — while PHP's own `callable` destination accepts every one of these shapes.
        // A union WITHOUT a `callable` member (`string|array`) carries no such proof and keeps the
        // member-by-member rule below.
        if matches!(expected, PhpType::Callable) {
            if let PhpType::Union(actual_members) = actual {
                if actual_members.contains(&PhpType::Callable)
                    && actual_members.iter().all(|member| {
                        matches!(
                            member,
                            PhpType::Callable
                                | PhpType::Str
                                | PhpType::Array(_)
                                | PhpType::AssocArray { .. }
                                | PhpType::Object(_)
                        )
                    })
                {
                    return true;
                }
            }
        }
        // A value whose flow type is a union is assignable only when every possible runtime
        // member satisfies the destination. Apply this before dispatching on `expected` so
        // non-union supertypes such as `iterable`, `object`, and generic arrays see each member
        // instead of rejecting the union wrapper itself.
        if let PhpType::Union(members) = actual {
            if !actual.is_int_float_union() {
                // Inside a WIDER union the arithmetic pair is still judged as the unit it is:
                // `$last ? null : -$i` is `int|float|null`, and splitting it asked whether `int`
                // accepts `float`, so `?int $d` refused what it takes as `int|float` alone and as
                // `null` alone. Every other member keeps the ordinary per-member rule, which is
                // what still catches an unchecked `X|false` sentinel.
                if members.contains(&PhpType::Int) && members.contains(&PhpType::Float) {
                    let arithmetic = PhpType::Union(vec![PhpType::Int, PhpType::Float]);
                    return self.type_accepts(expected, &arithmetic)
                        && members
                            .iter()
                            .filter(|member| !matches!(member, PhpType::Int | PhpType::Float))
                            .all(|member| self.type_accepts(expected, member));
                }
                return members.iter().all(|member| self.type_accepts(expected, member));
            }
        }
        // A `callable` value is, at run time, exactly one of three things: a function-name string,
        // an `[object, 'method']` array, or an invokable object. PHP has no `callable` PROPERTY or
        // return type, so code that needs to store one declares those three shapes instead —
        // Symfony's `ControllerEvent::$controller` is `private string|array|object`, written
        // straight from a `callable $controller` parameter. Such a destination accepts a callable
        // exactly when it accepts ALL THREE shapes; one that leaves a shape out (`string|object`,
        // say) would still fail at run time for the shape it omitted, so it is refused.
        if matches!(actual, PhpType::Callable) && !matches!(expected, PhpType::Callable) {
            return self.type_accepts(expected, &PhpType::Str)
                && self.type_accepts(expected, &PhpType::Array(Box::new(PhpType::Mixed)))
                && self.type_accepts(expected, &PhpType::Object(String::new()));
        }
        match expected {
            PhpType::Mixed => true,
            PhpType::Bool if matches!(actual, PhpType::False) => true,
            PhpType::Callable if actual.is_closure_object() => true,
            // The third callable shape: an object whose class declares `__invoke`.
            // symfony/event-dispatcher's `TraceableEventDispatcher::getListenerPriority(...,
            // callable|array $listener)` is handed a `WrappedListener`, and the union refused it
            // (a bare `callable` parameter already accepted it through the call-site coercion).
            PhpType::Callable
                if matches!(actual, PhpType::Object(class) if self.class_declares_invoke(class)) =>
            {
                true
            }
            // php's `callable` accepts an ARRAY: `[$object, 'method']` and `['Class', 'method']`
            // are two of the three shapes a callable HAS, not a coercion into one. elephc already
            // resolves both in builtin callback position (`array_map([$g, 'hello'], …)` compiles
            // and runs), so refusing them at a user-declared `callable` parameter was an
            // inconsistency rather than a policy: `symfony/http-foundation`'s
            // `SessionFactory(…, ?callable $usageReporter)` is handed exactly such a pair by the
            // generated container, and the whole build stopped there.
            //
            // The element bound is not cosmetic. `Op::NormalizeCallable` -- which
            // `coerce_scalar_arg_to_param_storage` emits to turn the pair into the descriptor the
            // parameter's storage actually holds -- implements `Array` of `Mixed` or `Str` and
            // refuses anything else, so accepting a wider element here would hand the backend a
            // shape it cannot convert. Whether the pair NAMES something real stays a runtime
            // question, exactly as it is in php.
            PhpType::Callable
                if matches!(
                    actual,
                    PhpType::Array(element)
                        if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Str)
                ) =>
            {
                true
            }
            // PHP coercive mode: scalars accept Mixed with runtime narrowing. `False`
            // belongs here alongside `Bool` — it is the same runtime representation, and
            // leaving it out would make a `T|false` contract stricter than the `T|bool`
            // one it replaces, rejecting bodies that both accepted before.
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
                if matches!(actual, PhpType::Mixed) =>
            {
                true
            }

            // The arithmetic union belongs with `Mixed` at a scalar position, and leaving it out
            // was inconsistent: `mixed` is the widest type there is, so refusing `int|float`
            // while accepting `mixed` made the NARROWER type the stricter one. It surfaced the
            // moment `$i++` started reporting `int|float`: every prelude that passes a loop
            // counter to an `int` parameter stopped compiling.
            //
            // Only THIS union, not unions in general. `string|false` at a `string` parameter
            // must keep being refused — that diagnostic is how an unchecked `false` sentinel
            // gets caught, and it is worth more than the symmetry.
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str
                if actual.is_int_float_union() =>
            {
                true
            }
            // An expected UNION containing a number accepts the arithmetic union whole. The
            // per-member recursion below cannot: it asks whether `int` accepts `float`, which is
            // false, so `?int $x` refused a counter that a bare `int $x` took. `mixed` was
            // accepted at all of these positions, so this is the same "narrower type is the
            // stricter one" hole, one level up.
            PhpType::Union(members)
                if actual.is_int_float_union()
                    && members
                        .iter()
                        .any(|member| matches!(member, PhpType::Int | PhpType::Float)) =>
            {
                true
            }
            PhpType::Union(members) => match actual {
                PhpType::Union(actual_members) => actual_members.iter().all(|actual_member| {
                    members
                        .iter()
                        .any(|expected_member| self.type_accepts(expected_member, actual_member))
                }),
                _ => members
                    .iter()
                    .any(|member| self.type_accepts(member, actual)),
            },

            PhpType::Array(expected_elem) => match actual {
                PhpType::Array(actual_elem) if matches!(actual_elem.as_ref(), PhpType::Never) => {
                    true
                }
                PhpType::Array(actual_elem) => {
                    self.type_accepts(expected_elem.as_ref(), actual_elem.as_ref())
                }
                PhpType::AssocArray { .. } => matches!(expected_elem.as_ref(), PhpType::Mixed),
                _ => false,
            },
            PhpType::AssocArray {
                key: expected_key,
                value: expected_value,
            } => match actual {
                PhpType::AssocArray {
                    key: actual_key,
                    value: actual_value,
                } => {
                    self.type_accepts(expected_key.as_ref(), actual_key.as_ref())
                        && self.type_accepts(expected_value.as_ref(), actual_value.as_ref())
                }
                // The empty literal `[]` is `Array(Never)` and carries no keys and no element
                // storage, so it satisfies EVERY declared container form — including a
                // string-keyed one, which the `Mixed | Int` case below cannot reach. Mirrors
                // the same shortcut the indexed arm takes.
                PhpType::Array(actual_elem) if matches!(actual_elem.as_ref(), PhpType::Never) => {
                    true
                }
                PhpType::Array(actual_elem)
                    if matches!(expected_key.as_ref(), PhpType::Mixed | PhpType::Int)
                        && self.type_accepts(expected_value.as_ref(), actual_elem.as_ref()) =>
                {
                    true
                }
                _ => false,
            },
            PhpType::Object(expected_name) => match actual {
                PhpType::Object(actual_name) => {
                    expected_name.is_empty()
                        || expected_name == actual_name
                        || self.is_subclass_of(actual_name, expected_name)
                        || self.class_implements_interface(actual_name, expected_name)
                        || self.interface_extends_interface(actual_name, expected_name)
                        // Two instantiations of one template are unrelated classes, so every
                        // disjunct above has already said no. A variance marker is the only
                        // thing that can relate them, and it relates them here rather than
                        // through an inheritance edge — see `generic_variance`.
                        || self.variance_permits(expected_name, actual_name)
                        || (expected_name == "Throwable"
                            && self.object_type_implements_throwable(actual_name))
                        // A class the closed world does not have (one the runtime autoloader
                        // supplies) cannot be PROVEN incompatible: its parents and interfaces are
                        // unknown here, and php checks the relation when the value arrives.
                        // Refusing it invents an answer where the closed world has none.
                        || self.absent_class_defers_to_runtime(actual_name)
                        || self.absent_class_defers_to_runtime(expected_name)
                }
                _ => false,
            },
            PhpType::Iterable => match actual {
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
                PhpType::Object(actual_name) => self.object_type_implements_iterable(actual_name),
                _ => false,
            },
            PhpType::Pointer(_) => Self::pointer_types_compatible(expected, actual),
            PhpType::Resource(_) => PhpType::resource_types_compatible(expected, actual),
            _ => false,
        }
    }

    /// Returns true if `ty` is a `PhpType::Union` that contains `PhpType::Void`.
    pub(crate) fn union_contains_void(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Void))
    }

    /// Removes `PhpType::Void` from a union type, re-normalizing the result.
    /// If all members are removed, returns `PhpType::Never`. If only one member remains,
    /// returns it directly without wrapping in a union.
    pub(crate) fn strip_void_from_union(&self, ty: &PhpType) -> PhpType {
        match ty {
            PhpType::Union(members) => {
                let filtered: Vec<PhpType> = members
                    .iter()
                    .filter(|member| **member != PhpType::Void)
                    .cloned()
                    .collect();
                self.normalize_union_type(filtered)
            }
            other => other.clone(),
        }
    }

    /// Returns true if `ty` is `Int`, `Bool`, `Void`, `Str`, or a union of only those types.
    /// These types support fast integer-dispatch in `Mixed` value handling.
    pub(crate) fn type_supports_mixed_int_dispatch(&self, ty: &PhpType) -> bool {
        let _ = self;
        match ty {
            PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void | PhpType::Str => true,
            PhpType::Union(members) => members
                .iter()
                .all(|member| self.type_supports_mixed_int_dispatch(member)),
            _ => false,
        }
    }

    // `is_union_with_mixed_int_dispatch` lived here until the merge: every caller now asks
    // `is_union_with_mixed_numeric_dispatch`, which subsumes it (it falls through to
    // `type_supports_mixed_int_dispatch` for every non-float member).

    /// Returns true if every member of `ty` can take part in a numeric comparison once boxed.
    ///
    /// Runtime ordering inspects each boxed tag, preserving float payloads and PHP's boolean
    /// truthiness rules. Integer-only dispatch remains intentionally narrower.
    fn type_supports_mixed_numeric_dispatch(&self, ty: &PhpType) -> bool {
        match ty {
            PhpType::Float => true,
            PhpType::Union(members) => members
                .iter()
                .all(|member| self.type_supports_mixed_numeric_dispatch(member)),
            other => self.type_supports_mixed_int_dispatch(other),
        }
    }

    /// Returns true if `ty` is a union whose every member can take part in a numeric comparison.
    pub(crate) fn is_union_with_mixed_numeric_dispatch(&self, ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(_)) && self.type_supports_mixed_numeric_dispatch(ty)
    }

    /// Computes the merged type when assigning `new_ty` to a variable that already has
    /// `existing` type. Returns `Some(merged)` when types are compatible for compound assignment
    /// (e.g., `+=`), or `None` when the types cannot be merged (e.g., two incompatible scalars).
    /// Returns whether every runtime value of `actual` is representable in `expected`
    /// WITHOUT a runtime narrowing.
    ///
    /// This is the containment question an inferred local's assignment merge asks, as
    /// opposed to the assignability question `type_accepts` answers. The two differ exactly
    /// on PHP's coercive acceptances: a declared `int` parameter accepts a `mixed` argument
    /// because the runtime narrows it at that declared boundary, but an inferred local has
    /// no boundary — a `mixed` value assigned to it (int arithmetic carries the
    /// overflow-to-float promotion) really is the local's content from then on. Answering
    /// the merge with `type_accepts` kept such locals narrow, which re-truncated promoted
    /// floats at every typed read.
    ///
    /// Delegating the non-Mixed cases to `type_accepts` knowingly keeps its structural
    /// array arms, so an `array<int>` still "contains" an `array<mixed>` — the element-level
    /// instance of the same lie. That family has its own storage-contract machinery
    /// (`loop_carried_storage_types`) and is left for a separate audit.
    fn type_contains_values(&self, expected: &PhpType, actual: &PhpType) -> bool {
        match actual {
            PhpType::Mixed => matches!(expected, PhpType::Mixed),
            PhpType::Union(members) => members
                .iter()
                .all(|member| self.type_contains_values(expected, member)),
            _ => self.type_accepts(expected, actual),
        }
    }

    /// Whether `class` (or a class it inherits from) declares `__invoke`, which makes its
    /// instances callable.
    pub(crate) fn class_declares_invoke(&self, class: &str) -> bool {
        self.classes
            .get(class.trim_start_matches('\\'))
            .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
    }

    /// The type a slot holds after being assigned `new_ty` while holding
    /// `existing` — the widening that a second write to one variable forces.
    pub(crate) fn merged_assignment_type(
        &self,
        existing: &PhpType,
        new_ty: &PhpType,
    ) -> Option<PhpType> {
        // A `Callable` slot is a callable DESCRIPTOR: a string or an array stored into it would be
        // converted, and php keeps the value. `$callback = [$r->getClosureThis(), $r->name]` after
        // `$callback instanceof \Closure` narrowed it to `Callable` came out of `is_array()` false
        // and out of `$callback[0]` as "Cannot index non-array" (symfony/cache's
        // `EarlyExpirationMessage::create()`). Such a store widens the slot to `Mixed`, which keeps
        // the value as is and still resolves it when it is called, as the scalar pairs below widen.
        if *existing == PhpType::Callable
            && matches!(
                new_ty.codegen_repr(),
                PhpType::Str | PhpType::Array(_) | PhpType::AssocArray { .. }
            )
        {
            return Some(PhpType::Mixed);
        }
        if self.type_contains_values(existing, new_ty) {
            return Some(existing.clone());
        }
        // A Mixed side that reaches here failed containment, so the merge widens: the
        // variable's content can no longer be represented by the narrower side.
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }
        // The arithmetic union merges with either number it is made of. It is what an
        // incremented integer local carries, and the checker walks a body more than once: the
        // first walk sees `$t = $i++` as `int` and the second as `int|float`, so refusing the
        // merge made a re-walk a hard `cannot reassign` on code that had not changed.
        if (existing.is_int_float_union()
            && matches!(new_ty, PhpType::Int | PhpType::Float))
            || (new_ty.is_int_float_union()
                && matches!(existing, PhpType::Int | PhpType::Float))
        {
            return Some(PhpType::Union(vec![PhpType::Int, PhpType::Float]));
        }
        if matches!(existing, PhpType::Union(_)) {
            return None;
        }
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
            && matches!(new_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            return Some(new_ty.clone());
        }
        // The unrestricted `array` contract (`PhpType::php_array()`, what `array_flip()` and
        // `array_filter()` return) is a boxed union of the two array layouts, not one of them. An
        // empty-array binding must still accept it: Symfony's `ChoiceType` seeds
        // `$unknownValues = []` outside a `use (&$unknownValues)` closure and the closure stores
        // `$unknownValues = $valueMap` (`array_flip(...)`) -- `cannot reassign ... from
        // array<never> to array`.
        if matches!(existing, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
            && matches!(new_ty, PhpType::Union(members)
                if !members.is_empty()
                    && members.iter().all(|m| matches!(m, PhpType::Array(_) | PhpType::AssocArray { .. })))
        {
            return Some(new_ty.clone());
        }
        if matches!(new_ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
            && matches!(existing, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            return Some(existing.clone());
        }
        if *new_ty == PhpType::Void {
            return Some(existing.clone());
        }
        if *existing == PhpType::Void {
            return Some(new_ty.clone());
        }
        if matches!((existing, new_ty), (PhpType::False, PhpType::Bool)) {
            // `false` is a bool value: Bool holds everything both sides can produce.
            return Some(PhpType::Bool);
        }
        if matches!(existing, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float)
            && matches!(new_ty, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float)
        {
            // Heterogeneous scalar pairs widen: keeping `existing` made the merge lie in
            // BOTH directions — `$x = 1; $x = 1.5;` kept Int and the inferred-int return
            // re-truncated the float, while `$x = 1.5; $x = 2;` kept Float and the
            // inferred-float return converted the int back up. Homogeneous pairs never
            // reach here (the containment shortcut returns them), so this arm's old
            // `existing` answer only ever served the lying cases.
            return Some(PhpType::Mixed);
        }
        if Self::pointer_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Pointer(Some(left)), PhpType::Pointer(Some(right))) if left == right => {
                    PhpType::Pointer(Some(left.clone()))
                }
                (PhpType::Pointer(None), PhpType::Pointer(Some(tag)))
                | (PhpType::Pointer(Some(tag)), PhpType::Pointer(None)) => {
                    PhpType::Pointer(Some(tag.clone()))
                }
                _ => PhpType::Pointer(None),
            });
        }
        if PhpType::resource_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Resource(Some(left)), PhpType::Resource(Some(right)))
                    if left == right =>
                {
                    PhpType::Resource(Some(left.clone()))
                }
                (PhpType::Resource(None), PhpType::Resource(Some(kind)))
                | (PhpType::Resource(Some(kind)), PhpType::Resource(None)) => {
                    PhpType::Resource(Some(kind.clone()))
                }
                _ => PhpType::Resource(None),
            });
        }
        None
    }

    /// Computes the merged array element type when writing a value of `new_ty` into an
    /// array that already has `existing` element type. Returns `Some(merged)` for compatible
    /// types (same, `Never`, `Mixed`, or compatible objects), or `None` otherwise.
    pub(crate) fn merge_array_element_type(
        &self,
        existing: &PhpType,
        new_ty: &PhpType,
    ) -> Option<PhpType> {
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Never) {
            return Some(new_ty.clone());
        }
        if matches!(new_ty, PhpType::Never) {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }

        match (existing, new_ty) {
            (PhpType::Object(left), PhpType::Object(right)) => self.common_object_type(left, right),
            // Pointers and `null` share the pointer word (`?ptr<T>` elements).
            _ => crate::types::merge_pointer_elements(existing, new_ty),
        }
    }

    /// Returns true if `ty` is `PhpType::Array(Box::new(PhpType::Mixed))`, i.e., an
    /// untyped `array` hint without element type specialization.
    pub(crate) fn is_generic_array_hint(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Mixed))
    }

    /// Whether a method's stored parameter may be sharpened to a call site's array shape.
    ///
    /// Only an unparameterized hint can be. A WRITTEN `array<mixed>` resolves to the same
    /// `Array(Mixed)` but is a contract: sharpening the method's stored signature to the first
    /// call's `array<int>` made every later call check against it, and `$k->g($strings)` was
    /// refused with `expects array<int>, got array<string>`.
    pub(crate) fn sharpens_to_call_site_array(sig: &crate::types::FunctionSig, index: usize) -> bool {
        Self::is_generic_array_hint(&sig.params[index].1)
            && !matches!(
                sig.param_type_exprs.get(index),
                Some(Some(
                    crate::parser::ast::TypeExpr::Array(_)
                        | crate::parser::ast::TypeExpr::AssocArray { .. }
                ))
            )
    }

    /// If `declared_ty` is a generic array hint and `actual_ty` is a concrete array or
    /// assoc-array, returns `actual_ty` (specialization). Otherwise returns `declared_ty`.
    /// Used to sharpen untyped `array` parameters when the actual argument type is known.
    pub(crate) fn specialize_generic_array_hint(
        declared_ty: &PhpType,
        actual_ty: &PhpType,
    ) -> PhpType {
        if Self::is_generic_array_hint(declared_ty)
            && matches!(actual_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            actual_ty.clone()
        } else {
            declared_ty.clone()
        }
    }

    /// Specializes a bare PHP `array` parameter without pinning object elements to one class.
    ///
    /// PHP does not expose array element generics, so a concrete object element inferred from
    /// one call site is not a valid contract for later calls. An empty literal contributes no
    /// element evidence and therefore keeps the generic declaration. Indexed object arrays stay
    /// `Array(Mixed)`, while associative arrays retain their key/storage shape but erase an object
    /// value to `Mixed`. Scalar element types remain specialized for existing inference.
    pub(crate) fn specialize_generic_array_param_hint(
        declared_ty: &PhpType,
        actual_ty: &PhpType,
    ) -> PhpType {
        if !Self::is_generic_array_hint(declared_ty) {
            return declared_ty.clone();
        }
        if let PhpType::Union(members) = actual_ty {
            if members.iter().all(|member| matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })) {
                return actual_ty.clone();
            }
        }
        match actual_ty {
            PhpType::Array(element) if matches!(element.as_ref(), PhpType::Never) => {
                declared_ty.clone()
            }
            PhpType::Array(element)
                if matches!(element.as_ref(), PhpType::Object(_))
                    || matches!(element.as_ref(), PhpType::Union(members) if members
                        .iter()
                        .any(|member| matches!(member, PhpType::Object(_)))) =>
            {
                PhpType::Array(Box::new(PhpType::Mixed))
            }
            PhpType::AssocArray { key, value }
                if matches!(value.as_ref(), PhpType::Object(_))
                    || matches!(value.as_ref(), PhpType::Union(members) if members
                        .iter()
                        .any(|member| matches!(member, PhpType::Object(_)))) =>
            {
                PhpType::AssocArray {
                    key: key.clone(),
                    value: Box::new(PhpType::Mixed),
                }
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } => actual_ty.clone(),
            _ => declared_ty.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Platform, Target};

    /// Verifies inferred bottom paths do not make nullable values reject nullsafe access.
    #[test]
    fn normalize_union_drops_never_when_a_value_member_exists() {
        let checker = Checker::new(Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(
            checker.normalize_union_type(vec![PhpType::Void, PhpType::Never]),
            PhpType::Void
        );
    }

    /// Verifies a union containing only bottom paths remains bottom.
    #[test]
    fn normalize_union_of_only_never_is_never() {
        let checker = Checker::new(Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(
            checker.normalize_union_type(vec![PhpType::Never, PhpType::Never]),
            PhpType::Never
        );
    }
}
