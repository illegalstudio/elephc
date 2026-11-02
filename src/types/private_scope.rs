//! Where a `private` method actually lives.
//!
//! PHP resolves `private` by the LEXICAL SCOPE, not by the receiver's class: inside `C::m()`, a
//! value typed as a SUBCLASS of `C` still reaches `C`'s private methods, because a private method
//! is not virtual and belongs to the class that declared it. Class flattening deliberately does
//! not inherit private methods into a subclass's table (`ClassLikeState::inherit_methods`), so a
//! subclass-typed receiver found nothing at all.
//!
//! MEASURED against php 8.5.10, with no `instanceof` involved:
//!
//! ```php
//! class P {
//!     private function secret($s) { return 'S' . $s; }
//!     public function viaSelf() { $o = new C(); return $o->secret('a'); }
//! }
//! class C extends P {}
//! echo (new P())->viaSelf();      // php: Sa      elephc: Undefined method: C::secret
//! ```
//!
//! Private PROPERTIES never had the gap -- `inherit_properties` copies them whatever their
//! visibility and `can_access_member` then admits the declaring scope -- so this is the method
//! half of a rule the compiler already applies to the other kind of member.
//!
//! Three passes need the same answer and each resolves members on its own: the checker types the
//! call, the IR lowering reads the signature to materialize omitted optional arguments, and
//! codegen picks the body to call. Missing it in the checker refused the program; missing it in
//! the IR lowering emitted `2 operands for 4 ABI params`. They ask here so they cannot disagree.

use crate::fast_hash::FastMap;
use crate::parser::ast::Visibility;
use crate::types::schema::ClassInfo;

/// Returns the class whose private declaration answers a call to `method_key` on a receiver typed
/// `receiver_class`, or `None` when the receiver's own resolution stands.
///
/// Deliberately conservative about which declaration wins: the scope is consulted only when the
/// receiver's class has NO method of that name. PHP would prefer the lexical private one even
/// over a public method the subclass declares; that case is left alone because changing it would
/// retarget calls that resolve today.
pub(crate) fn lexical_private_method_scope(
    classes: &FastMap<String, ClassInfo>,
    current_class: Option<&str>,
    receiver_class: &str,
    method_key: &str,
) -> Option<String> {
    let receiver = receiver_class.trim_start_matches('\\');
    if classes
        .get(receiver)
        .is_some_and(|info| info.methods.contains_key(method_key))
    {
        return None;
    }
    let scope = current_class?.trim_start_matches('\\');
    if scope == receiver {
        return None;
    }
    if classes
        .get(scope)
        .and_then(|info| info.method_visibilities.get(method_key))
        != Some(&Visibility::Private)
    {
        return None;
    }
    descends_from(classes, receiver, scope).then(|| scope.to_string())
}

/// Returns whether `class_name` is `ancestor` or inherits from it. A malformed inheritance cycle
/// ends the walk instead of hanging the compiler.
fn descends_from(classes: &FastMap<String, ClassInfo>, class_name: &str, ancestor: &str) -> bool {
    // Borrowed names and a hop bound instead of an owned name and a seen-list per hop: this runs
    // for method calls in both the checker and lowering. A chain longer than the class table can
    // only be a cycle, which ends the walk as before.
    let mut current = Some(class_name);
    let mut hops = 0usize;
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        hops += 1;
        if hops > classes.len() + 1 {
            return false;
        }
        current = classes
            .get(name)
            .and_then(|info| info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\'));
    }
    false
}
