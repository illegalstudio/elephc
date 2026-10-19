//! Purpose:
//! The join every checker fixpoint uses when a new observation widens a stored type: an untyped
//! parameter seen with another call-site argument, an untyped property assigned from another
//! constructor call, a magic `__call`/`__callStatic` argument array, an untyped variadic element.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution::specialization` (free-function params).
//! - `crate::types::checker::inference::objects::methods` (method params, variadics, magic args).
//! - `crate::types::checker::type_compat::object_types` (constructor-arg → property/param).
//!
//! Key details — THE RULE:
//! - The stored types form a lattice and every widening is its JOIN, so a stored type only
//!   ever climbs. `Never` is the bottom, `Mixed` the top; a type equal to the incoming one is
//!   unchanged; `Void`/`Never` arrive as the identity (with the tagged-null exceptions of
//!   [`Checker::union_param_type`]).
//! - Two DIFFERENT scalar tags join to `Mixed`, never to one of them: `int ⊔ float = mixed`,
//!   so each value keeps its own runtime tag (an int read from a float slot prints its bit
//!   pattern; a float read as int prints garbage).
//! - Two different classes join to their UNION (`B ⊔ C = B|C`, `P0 ⊔ P1 = P0|P1`, members
//!   sorted by name), never to a shared ancestor and never to one of the two: PHP is
//!   duck-typed, so `f(new B); f(new C)` may call `$x->m()` that only the children declare,
//!   and `e(new P1); e(mkp(): P0)` may call `extra()` that only the child declares and get
//!   PHP's `Error` when the runtime object is a `P0`. A method call on a union THIS JOIN
//!   built (`Checker::joined_object_unions`; a union the program declares stays strict)
//!   accepts the call when ANY member declares the method and dispatches on the runtime class id
//!   (`inference::objects::methods`), which is exactly that behavior. A union of more than
//!   [`MAX_OBJECT_UNION_MEMBERS`] classes is `Mixed`. The order is
//!   `X ⊑ X|Y ⊑ X|Y|Z ⊑ mixed`.
//!   Neither the old "the last writer wins" rule (`propagate_constructor_arg_type`) nor "the
//!   first writer wins" (`union_param_type`) is a join: both let the stored class depend on the
//!   order the writers ran in, which is exactly what made the outer fixpoint alternate
//!   Laravel's `HttpResponseException::$response` between Symfony's and Illuminate's `Response` forever.
//! - Arrays join element-wise through `union_param_type`'s payload rule.
//! - A writer may still ADOPT the first observation over a placeholder that is not an
//!   observation (the `Int` placeholder of an untyped parameter, the `Array(Mixed)` of a
//!   declared generic `array`, a property's default), but only ONCE, recorded in
//!   `param_specialization_seen`: re-adopting on a later round is a replacement, and a
//!   replacement is what oscillates (`CarbonInterval::__callStatic`'s `$parameters`).
//! - Termination follows: the lattice has finite height per program (scalar tags, the class
//!   hierarchy, bounded array nesting), so each entity changes a bounded number of times;
//!   `fixpoint_trace::OscillationDetector` turns any violation into an internal error.

use crate::types::PhpType;

use super::Checker;

impl Checker {
    /// Joins a stored (already observed) type with a new observation. See the module rule.
    pub(crate) fn join_observed_type(&self, stored: &PhpType, incoming: &PhpType) -> PhpType {
        match (stored, incoming) {
            _ if stored == incoming => stored.clone(),
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            (PhpType::Mixed, _) | (_, PhpType::Mixed) => PhpType::Mixed,
            // A union already holding the observation (`int|null` seeing `int` again) is its
            // own join; rebuilding it through `union_param_type` would climb to `Mixed`.
            (PhpType::Union(members), other) | (other, PhpType::Union(members))
                if members.contains(other) =>
            {
                PhpType::Union(members.clone())
            }
            _ if object_shaped(stored) && object_shaped(incoming) => {
                self.join_object_types(stored, incoming)
            }
            _ => Self::union_param_type(stored, incoming),
        }
    }

    /// Joins two object-shaped types (`Object`, or a union of objects and `null`) into the
    /// sorted union of their classes.
    fn join_object_types(&self, stored: &PhpType, incoming: &PhpType) -> PhpType {
        let mut classes: Vec<String> = Vec::new();
        let mut nullable = false;
        for ty in [stored, incoming] {
            let members: &[PhpType] = match ty {
                PhpType::Union(members) => members,
                other => std::slice::from_ref(other),
            };
            for member in members {
                match member {
                    PhpType::Object(name) => classes.push(name.clone()),
                    _ => nullable = true,
                }
            }
        }
        classes.sort();
        classes.dedup();
        // Related classes stay distinct members too: collapsing `P0|P1` to the ancestor `P0`
        // lost the child's own methods.
        let kept = classes;
        if kept.len() > MAX_OBJECT_UNION_MEMBERS {
            return PhpType::Mixed;
        }
        let mut members: Vec<PhpType> = kept.into_iter().map(PhpType::Object).collect();
        if nullable {
            members.push(PhpType::Void);
        }
        if members.len() == 1 {
            members.pop().expect("one member")
        } else {
            let union = PhpType::Union(members);
            self.joined_object_unions.borrow_mut().insert(format!("{union:?}"));
            union
        }
    }
}

/// Unrelated classes a join keeps as a union before it gives up to `Mixed`.
pub(crate) const MAX_OBJECT_UNION_MEMBERS: usize = 8;

/// Whether `ty` is an object or a union made only of objects and `null` (at least one object).
fn object_shaped(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) => true,
        PhpType::Union(members) => {
            members.iter().any(|member| matches!(member, PhpType::Object(_)))
                && members
                    .iter()
                    .all(|member| matches!(member, PhpType::Object(_) | PhpType::Void))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Checker {
        Checker::new(crate::codegen_support::platform::Target::new(
            crate::codegen_support::platform::Platform::MacOS,
            crate::codegen_support::platform::Arch::AArch64,
        ))
    }

    /// The join is commutative, idempotent on its result and only climbs. Class joins over a
    /// real hierarchy are covered end to end by `tests/codegen/checker_converge.rs`.
    #[test]
    fn join_is_commutative_and_monotone() {
        let checker = checker();
        let samples = vec![
            PhpType::Never,
            PhpType::Int,
            PhpType::Float,
            PhpType::Str,
            PhpType::Bool,
            PhpType::Mixed,
            PhpType::Callable,
            PhpType::Array(Box::new(PhpType::Int)),
            PhpType::Array(Box::new(PhpType::Float)),
            PhpType::Object("Unrelated".to_string()),
            PhpType::Object("Other".to_string()),
        ];
        for a in &samples {
            for b in &samples {
                let ab = checker.join_observed_type(a, b);
                let ba = checker.join_observed_type(b, a);
                assert_eq!(ab, ba, "join({a:?}, {b:?}) is not commutative");
                assert_eq!(checker.join_observed_type(&ab, a), ab, "join climbs back for {a:?}");
                assert_eq!(checker.join_observed_type(&ab, b), ab, "join climbs back for {b:?}");
            }
        }
        assert_eq!(checker.join_observed_type(&PhpType::Int, &PhpType::Float), PhpType::Mixed);
        assert_eq!(
            checker.join_observed_type(
                &PhpType::Array(Box::new(PhpType::Int)),
                &PhpType::Array(Box::new(PhpType::Float))
            ),
            PhpType::Array(Box::new(PhpType::Mixed))
        );
        assert_eq!(
            checker.join_observed_type(
                &PhpType::Object("Unrelated".to_string()),
                &PhpType::Object("Other".to_string())
            ),
            PhpType::Union(vec![
                PhpType::Object("Other".to_string()),
                PhpType::Object("Unrelated".to_string()),
            ])
        );
    }
}
