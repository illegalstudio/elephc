//! Purpose:
//! The contract of a declared property, as php-src's standard property handlers enforce it:
//! visibility in the accessing scope, readonly initialization and mutation, and the declared type
//! (coercing the assigned value in weak mode).
//!
//! Called from:
//! - `crate::object`'s `zend_std_*_property` handlers, before they touch the properties table.
//! - `crate::host`, which names the program's scope for one property access.
//!
//! Key details:
//! - The accessing scope is `EG(fake_scope)` (`zend_update_property()` and the like), else the
//!   program's while the host runs one of its accesses, else the class of the extension function
//!   running (null, the global scope, outside any call).
//! - A program class is never an engine class: it stands for its nearest hosted ancestor in a
//!   protected check, and it never declared a private property.
//! - The host first runs a program access as a PROBE, without naming the caller: an access the
//!   global scope may perform goes ahead, a private one is decided without a scope, and only one a
//!   protected (or `protected(set)` readonly) rule leaves to the caller stops, untouched, so the
//!   host can find the caller's class (which costs a backtrace) and run it again.
//! - Messages follow PHP 8.5: an inaccessible property names the object's class (the program's
//!   wrapper class when the program accesses it), readonly and type errors the declaring class.

use std::ffi::c_int;
use std::ptr;

use crate::abi::*;
use crate::global::Global;
use crate::hash;
use crate::startup::eg;
use crate::string::{self, bytes};
use crate::zval::{copy_value, set_bool, set_double, set_long, type_of, zval_ptr_dtor};

/// The class scope of the program's magic property access in progress, as compiled code
/// publishes it right before calling a `__get`, `__set`, `__isset` or `__unset` (the class
/// name's bytes, null for the global scope) and clears it once the call returns. The wrapper's
/// accessor takes it on entry (`take_magic_scope`).
#[repr(C)]
pub struct MagicScope {
    pub name: *const u8,
    pub len: usize,
    /// Non-zero when the code making the access declared `strict_types=1`.
    pub strict: usize,
}

/// c: hidden
#[no_mangle]
pub static mut elephc_php_ext_magic_scope: MagicScope = MagicScope { name: ptr::null(), len: 0, strict: 0 };

/// Marks a published scope whose code declared `strict_types=1`, ahead of the class chain.
pub const STRICT_MARK: u8 = b'!';

/// The class name compiled code published for the magic accessor running ("" for the global
/// scope, or when nothing was published), after `STRICT_MARK` when that code is strict, clearing
/// the cell so no later accessor reads it.
pub unsafe fn take_magic_scope() -> Vec<u8> {
    let cell = &mut *ptr::addr_of_mut!(elephc_php_ext_magic_scope);
    let mut scope = if cell.strict != 0 { vec![STRICT_MARK] } else { Vec::new() };
    if !cell.name.is_null() {
        scope.extend_from_slice(std::slice::from_raw_parts(cell.name, cell.len));
    }
    cell.name = ptr::null();
    cell.len = 0;
    cell.strict = 0;
    scope
}

/// The scope one program access runs in.
pub struct ProgramScope {
    /// The caller's nearest hosted class (itself when it is one), or null.
    pub ancestor: *mut zend_class_entry,
    /// The caller's class name, `None` for the global scope.
    pub name: Option<String>,
}

/// The program access the host is running.
struct ProgramAccess {
    object: *mut zend_object,
    /// The frame active when the access began: a call the extension makes from its handler runs
    /// in its own scope.
    frame: *mut zend_execute_data,
    /// The class the program sees the object as, for messages.
    class_name: String,
    /// `None` while probing.
    scope: Option<ProgramScope>,
    /// Whether a probe of this access stopped for the caller's scope.
    scope_needed: bool,
    /// Whether the code making the access declared `strict_types=1`.
    strict: bool,
}

/// The program accesses running, innermost last: an extension handler may call back into the
/// program, which may access another hosted object before the outer access ends.
static PROGRAM: Global<Vec<ProgramAccess>> = Global::new(Vec::new());

/// Starts a program access to `object`, seen as class `class_name`, in `scope` (`None` probes).
pub unsafe fn begin_program_access(object: *mut zend_object, class_name: String, scope: Option<ProgramScope>, strict: bool) {
    PROGRAM.get().push(ProgramAccess { object, frame: eg().current_execute_data, class_name, scope, scope_needed: false, strict });
}

/// Ends the innermost program access; true when its probe stopped because the caller's scope
/// decides.
pub unsafe fn end_program_access() -> bool {
    PROGRAM.get().pop().is_some_and(|access| access.scope_needed)
}

/// Where an access comes from.
enum Scope {
    /// An engine class, or null for the global scope.
    Engine(*mut zend_class_entry),
    Program { ancestor: *mut zend_class_entry, name: Option<String> },
    Probe,
}

/// The program access running on `object`, unless the extension has since entered a call.
unsafe fn program_access(object: *mut zend_object) -> Option<&'static ProgramAccess> {
    PROGRAM.get().last().filter(|access| access.object == object && access.frame == eg().current_execute_data)
}

unsafe fn current_scope(object: *mut zend_object) -> Scope {
    if !eg().fake_scope.is_null() {
        return Scope::Engine(eg().fake_scope);
    }
    match program_access(object) {
        Some(ProgramAccess { scope: None, .. }) => Scope::Probe,
        Some(ProgramAccess { scope: Some(scope), .. }) => Scope::Program { ancestor: scope.ancestor, name: scope.name.clone() },
        None => Scope::Engine(crate::api::zend_get_executed_scope()),
    }
}

/// php-src's `property_uses_strict_types`: whether the code assigning to `object`'s property
/// declared `strict_types=1`. An extension function's own writes never are.
pub unsafe fn uses_strict_types(object: *mut zend_object) -> bool {
    eg().fake_scope.is_null() && program_access(object).is_some_and(|access| access.strict)
}

/// The class name a message gives the object.
pub unsafe fn display_class(object: *mut zend_object) -> String {
    match program_access(object) {
        Some(access) => access.class_name.clone(),
        None => string::text((*(*object).ce).name),
    }
}

/// What a scope may do with a property.
pub enum Access {
    Allowed,
    /// A parent's private property: to this scope, as if it were not declared.
    Undeclared,
    /// Refused: `Cannot access private property`.
    Denied,
    /// A probe the caller's scope decides: stop without effect.
    Probe,
}

/// The declared, non-static property `name` of `ce`, or null.
pub unsafe fn info(ce: *mut zend_class_entry, name: *mut zend_string) -> *mut zend_property_info {
    let mut current = ce;
    while !current.is_null() {
        let found = hash::zend_hash_find_ptr(&(*current).properties_info, name).cast::<zend_property_info>();
        if !found.is_null() {
            return if (*found).flags & ZEND_ACC_STATIC != 0 { ptr::null_mut() } else { found };
        }
        current = (*current).parent_u.parent;
    }
    ptr::null_mut()
}

/// php-src's `is_protected_compatible_scope`: whether `scope` shares `declaring`'s lineage.
unsafe fn protected_compatible(declaring: *mut zend_class_entry, scope: &Scope) -> Option<bool> {
    match scope {
        Scope::Engine(ce) => Some(
            !ce.is_null()
                && (crate::class::instanceof_function(*ce, declaring) || crate::class::instanceof_function(declaring, *ce)),
        ),
        // An engine class never extends a program class, so only the caller's own lineage counts.
        Scope::Program { ancestor, .. } => Some(!ancestor.is_null() && crate::class::instanceof_function(*ancestor, declaring)),
        Scope::Probe => None,
    }
}

/// Whether the accessing scope sees property `info` of `object` (php-src's
/// `zend_get_property_offset`). A null `info` (an undeclared property) is always allowed.
pub unsafe fn visibility(object: *mut zend_object, info: *mut zend_property_info) -> Access {
    if info.is_null() || (*info).flags & ZEND_ACC_PUBLIC != 0 {
        return Access::Allowed;
    }
    let scope = current_scope(object);
    if let Scope::Engine(ce) = scope {
        if ce == (*info).ce {
            return Access::Allowed;
        }
    }
    if (*info).flags & ZEND_ACC_PRIVATE != 0 {
        // A private property of a parent does not exist outside the parent; nor does it on an
        // object of a program subclass, which the engine sees as the hosted class.
        let subclassed = program_access(object).is_some_and(|access| access.class_name.as_bytes() != bytes((*(*object).ce).name));
        return if (*info).ce != (*object).ce || subclassed { Access::Undeclared } else { Access::Denied };
    }
    match protected_compatible((*info).ce, &scope) {
        Some(true) => Access::Allowed,
        Some(false) => Access::Denied,
        None => Access::Probe,
    }
}

/// Records that a probe stopped for the caller's scope.
pub unsafe fn need_scope() {
    if let Some(access) = PROGRAM.get().last_mut() {
        access.scope_needed = true;
    }
}

/// Throws PHP's Error for a property the scope cannot see.
pub unsafe fn denied(object: *mut zend_object, info: *mut zend_property_info) {
    let visibility = if (*info).flags & ZEND_ACC_PRIVATE != 0 { "private" } else { "protected" };
    crate::exception::throw_error(
        crate::exception::zend_ce_error,
        &format!("Cannot access {visibility} property {}::${}", display_class(object), string::text((*info).name)),
    );
}

/// `Class::$name` of a declared property, naming its declaring class.
unsafe fn declared_name(info: *mut zend_property_info) -> String {
    format!("{}::${}", string::text((*(*info).ce).name), string::text((*info).name))
}

pub unsafe fn is_readonly(info: *mut zend_property_info) -> bool {
    !info.is_null() && (*info).flags & ZEND_ACC_READONLY != 0
}

pub unsafe fn is_typed(info: *mut zend_property_info) -> bool {
    !info.is_null() && (*info).r#type.type_mask & _ZEND_TYPE_MASK != 0
}

/// Whether the scope may initialize (or unset) an uninitialized readonly property: readonly is
/// `protected(set)`. Throws PHP's Error when it may not; `None` while probing.
pub unsafe fn readonly_scope_allows(object: *mut zend_object, info: *mut zend_property_info, unset: bool) -> Option<bool> {
    let scope = current_scope(object);
    if let Scope::Engine(ce) = scope {
        if ce == (*info).ce {
            return Some(true);
        }
    }
    let allowed = protected_compatible((*info).ce, &scope)?;
    if !allowed {
        let from = match &scope {
            Scope::Engine(ce) if !ce.is_null() => format!("scope {}", string::text((**ce).name)),
            Scope::Program { name: Some(name), .. } => format!("scope {name}"),
            _ => "global scope".to_string(),
        };
        let verb = if unset { "unset" } else { "modify" };
        crate::exception::throw_error(
            crate::exception::zend_ce_error,
            &format!("Cannot {verb} protected(set) readonly property {} from {from}", declared_name(info)),
        );
    }
    Some(allowed)
}

/// Throws PHP's Error for changing an initialized readonly property.
pub unsafe fn readonly_modified(info: *mut zend_property_info, unset: bool) {
    let verb = if unset { "unset" } else { "modify" };
    crate::exception::throw_error(crate::exception::zend_ce_error, &format!("Cannot {verb} readonly property {}", declared_name(info)));
}

/// Throws PHP's Error for reading a typed property before it was initialized.
pub unsafe fn uninitialized(info: *mut zend_property_info) {
    crate::exception::throw_error(
        crate::exception::zend_ce_error,
        &format!("Typed property {} must not be accessed before initialization", declared_name(info)),
    );
}

/// Whether `object` is an instance of a class `type` names.
unsafe fn names_class_of(t: zend_type, object: *mut zend_object) -> bool {
    let matches = |name: &[u8]| {
        let ce = crate::class::lookup(name);
        !ce.is_null() && crate::class::instanceof_function((*object).ce, ce)
    };
    let mask = t.type_mask;
    if mask & _ZEND_TYPE_LIST_BIT != 0 {
        let list = t.ptr.cast::<zend_type_list>();
        for i in 0..(*list).num_types {
            if names_class_of(*(*list).types.as_ptr().add(i as usize), object) {
                return true;
            }
        }
        false
    } else if mask & _ZEND_TYPE_NAME_BIT != 0 {
        matches(bytes(t.ptr.cast()))
    } else if mask & _ZEND_TYPE_LITERAL_NAME_BIT != 0 {
        std::ffi::CStr::from_ptr(t.ptr.cast()).to_bytes().split(|&b| b == b'|').any(matches)
    } else {
        false
    }
}

/// Checks `value` (owned, dereferenced) against the declared type of `info`, converting it in
/// place as PHP's weak mode does (`zend_verify_property_type`), or, for `strict` code, only an int
/// a float type takes; throws PHP's TypeError and returns false when it does not fit.
pub unsafe fn verify_type(info: *mut zend_property_info, value: *mut zval, strict: bool) -> bool {
    let t = (*info).r#type;
    let mask = t.type_mask & _ZEND_TYPE_MAY_BE_MASK;
    let fits = match type_of(value) {
        _ if mask & MAY_BE_ANY == MAY_BE_ANY => true,
        IS_NULL => mask & MAY_BE_NULL != 0,
        IS_FALSE => mask & MAY_BE_FALSE != 0,
        IS_TRUE => mask & MAY_BE_TRUE != 0,
        IS_LONG => mask & MAY_BE_LONG != 0,
        IS_DOUBLE => mask & MAY_BE_DOUBLE != 0,
        IS_STRING => mask & MAY_BE_STRING != 0,
        IS_ARRAY => mask & MAY_BE_ARRAY != 0,
        IS_OBJECT => mask & MAY_BE_OBJECT != 0 || names_class_of(t, (*value).value.obj),
        _ => false,
    };
    // php-src's `zend_verify_scalar_type_hint`: strict code converts nothing but an int a float
    // type takes.
    let convertible = !strict || (mask & MAY_BE_DOUBLE != 0 && type_of(value) == IS_LONG);
    if fits || (convertible && coerce_weak(mask, value)) {
        return true;
    }
    if eg().exception.is_null() {
        crate::exception::throw_error(
            crate::exception::zend_ce_type_error,
            &format!(
                "Cannot assign {} to property {} of type {}",
                crate::convert::value_name(value),
                declared_name(info),
                crate::describe::type_to_string(t).unwrap_or_default()
            ),
        );
    }
    false
}

/// php-src's `zend_verify_weak_scalar_type_hint`: converts a scalar (or a Stringable object, for a
/// string) to the first of int, float, string and bool the type allows.
unsafe fn coerce_weak(mask: u32, value: *mut zval) -> bool {
    let code = type_of(value);
    if !matches!(code, IS_FALSE | IS_TRUE | IS_LONG | IS_DOUBLE | IS_STRING | IS_OBJECT) {
        return false;
    }
    if code == IS_OBJECT && mask & MAY_BE_STRING == 0 {
        return false;
    }
    // An int|float type takes a numeric string as the number it spells.
    if code == IS_STRING && mask & MAY_BE_DOUBLE != 0 && mask & MAY_BE_LONG != 0 {
        let (mut lval, mut dval) = (0, 0.0);
        match crate::convert::is_numeric_str_function((*value).value.str, &mut lval, &mut dval) {
            IS_LONG => return replace(value, |slot| set_long(slot, lval)),
            IS_DOUBLE => return replace(value, |slot| set_double(slot, dval)),
            _ => {}
        }
    }
    let mut lval = 0;
    if mask & MAY_BE_LONG != 0 && crate::zpp::zend_parse_arg_long_weak(value, &mut lval, 0) {
        return replace(value, |slot| set_long(slot, lval));
    }
    if !eg().exception.is_null() {
        return false;
    }
    let mut dval = 0.0;
    if mask & MAY_BE_DOUBLE != 0 && crate::zpp::zend_parse_arg_double_weak(value, &mut dval, 0) {
        return replace(value, |slot| set_double(slot, dval));
    }
    let mut converted = ptr::null_mut();
    if mask & MAY_BE_STRING != 0 && crate::zpp::zend_parse_arg_str_weak(value, &mut converted, 0) {
        return true;
    }
    let mut bval = false;
    if mask & MAY_BE_BOOL == MAY_BE_BOOL && crate::zpp::zend_parse_arg_bool_weak(value, &mut bval, 0) {
        return replace(value, |slot| set_bool(slot, bval));
    }
    false
}

/// Replaces `value` with the scalar `set` writes, releasing what it held.
unsafe fn replace(value: *mut zval, set: impl FnOnce(*mut zval)) -> bool {
    let mut scalar = zval::UNDEF;
    set(&mut scalar);
    zval_ptr_dtor(value);
    copy_value(value, &scalar);
    true
}

/// The `type` argument a read handler gets: whether a missing property stays silent.
pub fn is_silent(r#type: c_int) -> bool {
    r#type == BP_VAR_IS || r#type == BP_VAR_UNSET
}
