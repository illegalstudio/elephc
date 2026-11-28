//! Purpose:
//! Tracks when eval reaches the moments php-src evaluates stored constant expressions (a
//! class initialization, a class constant being evaluated, a parameter default's first
//! report), plus the silent evaluations that drop deprecation notices.
//!
//! Called from:
//! - `crate::interpreter::constant_eval` and the class-constant fetch path, which drop a
//!   notice instead of raising it while a silent evaluation is open.
//! - Class and enum initialization (first `new`, static-property access, enum use),
//!   class-constant evaluation, reflection, and parameter-default evaluation.
//!
//! Key details:
//! - php-src evaluates a class constant on its first evaluation, a class's instance and static
//!   property defaults on its initialization, an enum case on its first evaluation, and a
//!   parameter default on its first omitted call; eval evaluates each at that moment too.
//! - A silent evaluation is eval materializing a value php-src does not compute at that point
//!   (a reflector being built): it drops notices and has no lasting effect.
//! - Class names are compared case-insensitively; constant names exactly.

use super::*;
use crate::eval_ir::EvalExpr;

/// Deprecation routing state and the evaluation moments eval has reached.
#[derive(Default)]
pub(crate) struct EvalDeprecationNotices {
    /// How many silent evaluations are open: notices are dropped and nothing waiting for its
    /// moment (a class constant's evaluation, a class initialization, a default site's first
    /// report) happens.
    silence_depth: usize,
    /// Classes and enums whose initialization already ran.
    initialized_classes: HashSet<String>,
    /// Parameter-default sites (`callable#position`) whose notices were already raised.
    reported_default_sites: HashSet<String>,
    /// Defaults of live `ReflectionParameter` objects by object identity, with the class
    /// `self::` names, which `getDefaultValue()` evaluates again on every call.
    reflected_parameter_defaults: HashMap<u64, EvalReflectedParameterDefault>,
    /// Per class or trait, the instance slots (`declaring\0name`) it copied already evaluated
    /// when it was declared, because the parent or trait they came from was initialized.
    precopied_slots: HashMap<String, HashSet<String>>,
    /// Class constants and enum cases a fetch inside a constant expression is evaluating
    /// (php-src's `MARK_CONSTANT_VISITED`), to reject a self-reference.
    constants_in_evaluation: HashSet<(String, String)>,
    /// How many constant expressions (class constant, enum case, property or parameter default)
    /// are being evaluated.
    constant_expression_depth: usize,
    /// The closure a `ReflectionFunction` being built reflects, which its parameter reflectors
    /// default factories capture (a closure cannot be named again).
    reflected_closure: Option<RuntimeCellHandle>,
    /// Constants `define()` gave a string built at run time, which php-src keeps refcounted.
    built_constants: HashSet<String>,
}

/// The default a live `ReflectionParameter` reflects, which `getDefaultValue()` evaluates
/// again on every call in the scopes of the declaring callable.
#[derive(Clone)]
pub(crate) struct EvalReflectedParameterDefault {
    /// The default expression.
    pub(crate) default: EvalExpr,
    /// The class `self::` names in the default.
    pub(crate) declaring_class: Option<String>,
    /// The declaring callable's magic-constant scope: function, method, class, and trait name.
    pub(crate) magic_scope: Option<(String, String, Option<String>, Option<String>)>,
}

/// Normalizes a class name for the notice registries.
fn notice_class_key(class_name: &str) -> String {
    class_name.trim_start_matches('\\').to_ascii_lowercase()
}

impl ElephcEvalContext {
    /// Opens a silent evaluation, for work php-src does not do at this point (materializing a
    /// reflector, re-evaluating a default per object): notices raised until the matching
    /// [`Self::end_deprecation_silence`] are dropped.
    pub(crate) fn begin_deprecation_silence(&mut self) {
        self.deprecation_notices.silence_depth += 1;
    }

    /// Closes the innermost silent evaluation.
    pub(crate) fn end_deprecation_silence(&mut self) {
        self.deprecation_notices.silence_depth = self.deprecation_notices.silence_depth.saturating_sub(1);
    }

    /// Reports whether the innermost open evaluation is silent: it must store nothing and start
    /// no initialization, which happens at the moment php-src makes that evaluation.
    pub(crate) fn deprecations_silenced(&self) -> bool {
        self.deprecation_notices.silence_depth > 0
    }

    /// Drops `message` in a silent evaluation. Returns `false` outside one, in which case the
    /// caller raises the notice itself.
    pub(crate) fn capture_deprecation(&mut self, _message: &'static str) -> bool {
        self.deprecations_silenced()
    }

    /// Marks `class_name` (a class or an enum) initialized. Returns `true` only the first time.
    pub(crate) fn begin_class_initialization(&mut self, class_name: &str) -> bool {
        self.deprecation_notices
            .initialized_classes
            .insert(notice_class_key(class_name))
    }

    /// Marks `holder::name` visited by a fetch inside a constant expression; `false` when it
    /// already is (a self-referencing constant).
    pub(crate) fn begin_class_constant_evaluation(&mut self, holder: &str, name: &str) -> bool {
        self.deprecation_notices
            .constants_in_evaluation
            .insert((notice_class_key(holder), name.to_string()))
    }

    /// Enters the evaluation of a constant expression.
    pub(crate) fn enter_constant_expression(&mut self) {
        self.deprecation_notices.constant_expression_depth += 1;
    }

    /// Leaves the evaluation entered by [`Self::enter_constant_expression`].
    pub(crate) fn leave_constant_expression(&mut self) {
        self.deprecation_notices.constant_expression_depth =
            self.deprecation_notices.constant_expression_depth.saturating_sub(1);
    }

    /// Reports whether a constant expression is being evaluated: a class-constant fetch then
    /// goes through php-src's `zend_get_class_constant_ex`, which marks the constant visited.
    pub(crate) fn in_constant_expression(&self) -> bool {
        self.deprecation_notices.constant_expression_depth > 0
    }

    /// Ends the evaluation started by [`Self::begin_class_constant_evaluation`].
    pub(crate) fn end_class_constant_evaluation(&mut self, holder: &str, name: &str) {
        self.deprecation_notices
            .constants_in_evaluation
            .remove(&(notice_class_key(holder), name.to_string()));
    }

    /// Records that `class_name` copied instance slot `declaring::$name` already evaluated.
    pub(crate) fn record_precopied_slot(&mut self, class_name: &str, declaring: &str, name: &str) {
        self.deprecation_notices
            .precopied_slots
            .entry(notice_class_key(class_name))
            .or_default()
            .insert(format!("{}\0{}", notice_class_key(declaring), name));
    }

    /// Sets the closure a `ReflectionFunction` being built reflects (borrowed), returning the
    /// previous one.
    pub(crate) fn replace_reflected_closure(&mut self, closure: Option<RuntimeCellHandle>) -> Option<RuntimeCellHandle> {
        std::mem::replace(&mut self.deprecation_notices.reflected_closure, closure)
    }

    /// Returns the closure a `ReflectionFunction` being built reflects, if any.
    pub(crate) fn reflected_closure(&self) -> Option<RuntimeCellHandle> {
        self.deprecation_notices.reflected_closure
    }

    /// Records that `define()` gave constant `name` a string built at run time.
    pub(crate) fn record_built_constant(&mut self, name: &str) {
        self.deprecation_notices.built_constants.insert(name.trim_start_matches('\\').to_string());
    }

    /// Reports whether `define()` gave constant `name` a string built at run time.
    pub(crate) fn defined_constant_was_built(&self, name: &str) -> bool {
        self.deprecation_notices.built_constants.contains(name.trim_start_matches('\\'))
    }

    /// Reports whether `class_name` copied instance slot `declaring::$name` already evaluated.
    pub(crate) fn slot_precopied(&self, class_name: &str, declaring: &str, name: &str) -> bool {
        self.deprecation_notices
            .precopied_slots
            .get(&notice_class_key(class_name))
            .is_some_and(|slots| slots.contains(&format!("{}\0{}", notice_class_key(declaring), name)))
    }

    /// Reports whether `class_name`'s initialization already ran, without starting it.
    pub(crate) fn class_initialization_ran(&self, class_name: &str) -> bool {
        self.deprecation_notices
            .initialized_classes
            .contains(&notice_class_key(class_name))
    }

    /// Records the default a `ReflectionParameter` object (`identity`) reflects.
    pub(crate) fn register_reflected_parameter_default(&mut self, identity: u64, default: EvalReflectedParameterDefault) {
        self.clear_eval_reflection_identity(identity);
        self.deprecation_notices.reflected_parameter_defaults.insert(identity, default);
        self.own_eval_reflection_identity(identity);
    }

    /// Detaches the default expression when its synthetic reflector is retired.
    pub(super) fn forget_reflected_parameter_default(&mut self, identity: u64) {
        self.deprecation_notices.reflected_parameter_defaults.remove(&identity);
    }

    /// Returns the default a `ReflectionParameter` object (`identity`) reflects, if recorded.
    pub(crate) fn reflected_parameter_default(&self, identity: u64) -> Option<EvalReflectedParameterDefault> {
        self.deprecation_notices.reflected_parameter_defaults.get(&identity).cloned()
    }

    /// Forgets that `class_name`'s initialization ran, after an initializer threw: php-src
    /// does not mark the class initialized then, so the next use retries it.
    pub(crate) fn abort_class_initialization(&mut self, class_name: &str) {
        self.deprecation_notices.initialized_classes.remove(&notice_class_key(class_name));
    }

    /// Reports whether a parameter-default site already reported its notices.
    pub(crate) fn default_site_reported(&self, site: &str) -> bool {
        self.deprecation_notices.reported_default_sites.contains(&site.to_ascii_lowercase())
    }

    /// Returns `true` the first time a parameter-default site reports its notices.
    pub(crate) fn first_default_site_report(&mut self, site: &str) -> bool {
        self.deprecation_notices.reported_default_sites.insert(site.to_ascii_lowercase())
    }
}
