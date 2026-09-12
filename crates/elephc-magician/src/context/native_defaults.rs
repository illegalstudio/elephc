//! Purpose:
//! Defines native callable defaults and reusable instance/static/constructor signatures.
//!
//! Called from:
//! - FFI registration, argument binding, Reflection, and default materialization.
//!
//! Key details:
//! - Scalar, array, and object defaults preserve keyed/named structure without runtime cells.

use super::*;

/// Default value for a native AOT callable parameter visible to eval fragments.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeCallableDefault {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    EmptyArray,
    Array(Vec<NativeCallableArrayDefaultElement>),
    Object {
        class_name: String,
        args: Vec<NativeCallableObjectDefaultArg>,
    },
}

/// One element in an array-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCallableArrayDefaultElement {
    pub key: Option<NativeCallableArrayDefaultKey>,
    pub value: NativeCallableDefault,
}

impl NativeCallableArrayDefaultElement {
    /// Creates one auto-indexed element for an array-valued default.
    pub fn positional(value: NativeCallableDefault) -> Self {
        Self { key: None, value }
    }

    /// Creates one explicitly keyed element for an array-valued default.
    pub fn keyed(key: NativeCallableArrayDefaultKey, value: NativeCallableDefault) -> Self {
        Self {
            key: Some(key),
            value,
        }
    }
}

/// Static PHP array key retained for an array-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeCallableArrayDefaultKey {
    Int(i64),
    String(String),
}

/// Constructor argument for an object-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCallableObjectDefaultArg {
    pub name: Option<String>,
    pub value: NativeCallableDefault,
}

impl NativeCallableObjectDefaultArg {
    /// Creates one positional constructor argument for an object-valued default.
    pub fn positional(value: NativeCallableDefault) -> Self {
        Self { name: None, value }
    }

    /// Creates one named constructor argument for an object-valued default.
    pub fn named(name: impl Into<String>, value: NativeCallableDefault) -> Self {
        Self {
            name: Some(name.into()),
            value,
        }
    }
}

/// Explicit PHP signature shape the compiler registers for one generated bridge.
///
/// This is positive metadata, not an inference. Nothing about the PHP-visible shape may be
/// recovered from the registered DEFAULTS, because a default is registered only when it is
/// representable in the eval default ABI: an enum case or a deeply nested constant expression
/// registers no default at all, and a shape derived from that absence would report an optional
/// parameter as required and a frame that carries the actual argument count as one that does not.
///
/// The three fields are emitted by `crate::codegen::lower_inst::builtins::eval` from the AST-level
/// `FunctionSig`, where every declared default is present as an expression regardless of whether
/// its VALUE can be represented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCallableShape {
    /// PHP-visible non-variadic parameters, which are the leading run of physical slots.
    pub(super) visible_regular_param_count: usize,
    /// Mandatory PHP-visible parameters, counted from the source declaration.
    pub(super) required_param_count: usize,
    /// Whether the registered variadic slot is one the PHP SOURCE declared.
    ///
    /// When false, that slot is the compiler's hidden surplus-argument collector: it is invisible
    /// to PHP, so an unknown named argument must still be refused and Reflection must not report
    /// a variadic parameter.
    pub(super) source_variadic: bool,
    /// Whether the hidden collector's first element carries the actual PHP argument count.
    ///
    /// The callee cannot tell an omitted optional from a supplied one by counting its parameters,
    /// so when a visible regular carries a default the count travels as the collector's first
    /// element. This mirrors `func_args::sig_collects_optional_arg_count` on the compiler side.
    pub(super) collector_carries_count: bool,
}

/// Bit set in the registration ABI's shape-flags word when the variadic slot is source-declared.
///
/// The generated code emits this numbering literally (it cannot depend on this crate), so the
/// compiler-side emitter in `src/codegen/lower_inst/builtins/eval` names these constants in a
/// comment. Changing either without the other silently rewrites every registered signature shape.
pub const NATIVE_SHAPE_FLAG_SOURCE_VARIADIC: u64 = 1 << 0;

/// Bit set in the shape-flags word when the hidden collector's first element is the actual count.
pub const NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT: u64 = 1 << 1;

impl NativeCallableShape {
    /// Decodes one registered shape from the three ABI words the generated bridge emits.
    pub fn from_abi(
        visible_regular_param_count: u64,
        required_param_count: u64,
        shape_flags: u64,
    ) -> Option<Self> {
        let visible_regular_param_count = usize::try_from(visible_regular_param_count).ok()?;
        let required_param_count = usize::try_from(required_param_count).ok()?;
        Some(Self::new(
            visible_regular_param_count,
            required_param_count,
            shape_flags & NATIVE_SHAPE_FLAG_SOURCE_VARIADIC != 0,
            shape_flags & NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT != 0,
        ))
    }

    /// Creates one registered PHP signature shape, clamping `required` to the visible regulars.
    ///
    /// A `required` larger than the visible regular count could only come from a malformed
    /// registration, and letting it through would make a call the PHP source accepts unreachable.
    pub fn new(
        visible_regular_param_count: usize,
        required_param_count: usize,
        source_variadic: bool,
        collector_carries_count: bool,
    ) -> Self {
        Self {
            visible_regular_param_count,
            required_param_count: required_param_count.min(visible_regular_param_count),
            source_variadic,
            collector_carries_count,
        }
    }
}

/// Resolved PHP-visible / compiler-internal partition of one callable's physical parameter slots.
///
/// Built from the registered [`NativeCallableShape`] when the bridge emitted one, and from the
/// physical parameter list alone otherwise. The fallback matters: a legacy or foreign
/// registration that never registers a shape (and may never register parameter NAMES either)
/// declares no hidden slots at all, so every physical slot stays PHP-visible.
///
/// The invariant every accessor below preserves is that the hidden slots are exactly the physical
/// suffix outside the visible source slots: `0..visible_regular_param_count` plus, when the source
/// declared one, the variadic slot.
pub struct NativeCallableFrameShape {
    param_count: usize,
    variadic_index: Option<usize>,
    declared: Option<NativeCallableShape>,
}

impl NativeCallableFrameShape {
    /// Resolves the partition for one physical parameter list and its optional declared shape.
    pub fn new(
        param_count: usize,
        variadic_index: Option<usize>,
        declared: Option<NativeCallableShape>,
    ) -> Self {
        Self {
            param_count,
            variadic_index,
            // A shape claiming more visible regulars than the bridge has physical slots is
            // malformed; falling back keeps every slot visible instead of hiding a real one.
            declared: declared.filter(|shape| shape.visible_regular_param_count <= param_count),
        }
    }

    /// Returns how many leading physical slots are PHP-visible NON-variadic parameters.
    pub fn visible_regular_param_count(&self) -> usize {
        match self.declared {
            Some(shape) => shape.visible_regular_param_count,
            // Without a declared shape the only non-PHP slot that can exist is the variadic
            // collector itself, and a foreign registration's variadic is a PHP variadic.
            None => self
                .variadic_index
                .unwrap_or(self.param_count)
                .min(self.param_count),
        }
    }

    /// Returns the variadic slot the PHP source itself declared, if the source declared one.
    pub fn source_variadic_index(&self) -> Option<usize> {
        match self.declared {
            Some(shape) => self.variadic_index.filter(|_| shape.source_variadic),
            None => self.variadic_index,
        }
    }

    /// Returns the hidden surplus-argument collector slot, if this frame carries one.
    pub fn hidden_collector_index(&self) -> Option<usize> {
        match self.declared {
            Some(shape) => self.variadic_index.filter(|_| !shape.source_variadic),
            None => None,
        }
    }

    /// Returns whether one physical slot is a compiler-internal parameter rather than a PHP one.
    pub fn param_is_hidden(&self, index: usize) -> bool {
        index < self.param_count
            && index >= self.visible_regular_param_count()
            && self.source_variadic_index() != Some(index)
    }

    /// Returns the hidden actual-argument-count slot, which follows the visible regulars.
    ///
    /// It exists only next to a SOURCE-declared variadic: a frame whose tail slot is the hidden
    /// collector carries its count inside that collector instead.
    pub fn hidden_argc_index(&self) -> Option<usize> {
        let index = self.visible_regular_param_count();
        (self.param_is_hidden(index) && self.variadic_index != Some(index)).then_some(index)
    }

    /// Returns whether the hidden collector must begin with the actual PHP argument count.
    pub fn collector_needs_count(&self) -> bool {
        self.hidden_collector_index().is_some()
            && self.declared.is_some_and(|shape| shape.collector_carries_count)
    }

    /// Returns the physical slots a PHP caller may bind, in declaration order.
    ///
    /// A SOURCE-declared variadic owns the LAST physical slot, after any hidden count parameter,
    /// so the list is not always a contiguous range.
    pub fn visible_param_indexes(&self) -> Vec<usize> {
        let mut indexes: Vec<usize> = (0..self.visible_regular_param_count()).collect();
        if let Some(index) = self.source_variadic_index() {
            if index >= indexes.len() {
                indexes.push(index);
            }
        }
        indexes
    }

    /// Returns the source-declared mandatory parameter count, when the bridge registered one.
    pub fn declared_required_param_count(&self) -> Option<usize> {
        self.declared.map(|shape| shape.required_param_count)
    }
}

/// Native AOT method or constructor signature metadata visible to eval fragments.
///
/// `param_count` is the PHYSICAL parameter count of the generated method or constructor bridge,
/// because eval calls that bridge directly and must produce one argument per physical slot.
/// A method that uses `func_get_args()`, or any method at all once the backtrace gate fires,
/// carries compiler-internal slots in that vector: a hidden actual-argument count, and a hidden
/// variadic collector for the surplus.
///
/// Which slots those are is NOT guessed from the parameter names or the registered defaults. The
/// generated bridge registers an explicit [`NativeCallableShape`], and [`NativeCallableFrameShape`]
/// turns it into the visible/hidden partition every accessor below reads. Hidden slots do keep an
/// empty registered NAME so a named argument and Reflection cannot reach them, but that spelling
/// is a consequence of the shape rather than its source of truth.
#[derive(Clone)]
pub struct NativeCallableSignature {
    pub(super) param_count: usize,
    pub(super) param_names: Vec<String>,
    pub(super) param_types: Vec<Option<EvalParameterType>>,
    pub(super) param_defaults: Vec<Option<NativeCallableDefault>>,
    /// Compiler-emitted zero-argument helpers for optional defaults outside compact metadata.
    pub(super) compiled_param_defaults: Vec<Option<usize>>,
    pub(super) param_by_ref: Vec<bool>,
    pub(super) variadic_index: Option<usize>,
    pub(super) return_type: Option<EvalParameterType>,
    pub(super) bridge_supported: bool,
    pub(super) shape: Option<NativeCallableShape>,
}

impl NativeCallableSignature {
    /// Creates signature metadata with the visible positional parameter count.
    pub const fn new(param_count: usize) -> Self {
        Self {
            param_count,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_defaults: Vec::new(),
            compiled_param_defaults: Vec::new(),
            param_by_ref: Vec::new(),
            variadic_index: None,
            return_type: None,
            bridge_supported: true,
            shape: None,
        }
    }

    /// Returns the visible positional parameter count accepted by this callable.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callable slot.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<String>) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_names.len() < self.param_count {
            self.param_names.resize(self.param_count, String::new());
        }
        self.param_names[index] = name.into();
        true
    }

    /// Records the PHP declared type metadata for one positional callable slot.
    pub fn set_param_type(&mut self, index: usize, param_type: EvalParameterType) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_types.len() < self.param_count {
            self.param_types.resize(self.param_count, None);
        }
        self.param_types[index] = Some(param_type);
        true
    }

    /// Records a PHP scalar default value for one positional callable slot.
    pub fn set_param_default(&mut self, index: usize, default: NativeCallableDefault) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_defaults.len() < self.param_count {
            self.param_defaults.resize(self.param_count, None);
        }
        self.param_defaults[index] = Some(default);
        true
    }

    /// Records a compiler-emitted Mixed-returning helper for one optional parameter default.
    pub fn set_compiled_param_default(&mut self, index: usize, callback: usize) -> bool {
        if index >= self.param_count || callback == 0 {
            return false;
        }
        if self.compiled_param_defaults.len() < self.param_count {
            self.compiled_param_defaults.resize(self.param_count, None);
        }
        self.compiled_param_defaults[index] = Some(callback);
        true
    }

    /// Records whether one positional callable parameter is by-reference.
    pub fn set_param_by_ref(&mut self, index: usize, by_ref: bool) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_by_ref.len() < self.param_count {
            self.param_by_ref.resize(self.param_count, false);
        }
        self.param_by_ref[index] = by_ref;
        true
    }

    /// Records which positional callable parameter is variadic.
    pub fn set_variadic_index(&mut self, index: usize) -> bool {
        if index >= self.param_count {
            return false;
        }
        self.variadic_index = Some(index);
        true
    }

    /// Records the PHP declared return type metadata for this callable.
    pub fn set_return_type(&mut self, return_type: EvalParameterType) {
        self.return_type = Some(return_type);
    }

    /// Records whether eval may dispatch this callable through the generated bridge.
    pub fn set_bridge_supported(&mut self, supported: bool) {
        self.bridge_supported = supported;
    }

    /// Returns the PHP-visible parameter names registered for this callable.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Returns PHP declared parameter types registered for this callable.
    pub fn param_types(&self) -> &[Option<EvalParameterType>] {
        &self.param_types
    }

    /// Returns the registered declared type for one parameter slot, if any.
    pub fn param_type(&self, index: usize) -> Option<&EvalParameterType> {
        self.param_types.get(index).and_then(Option::as_ref)
    }

    /// Returns the PHP-visible scalar parameter defaults registered for this callable.
    pub fn param_defaults(&self) -> &[Option<NativeCallableDefault>] {
        &self.param_defaults
    }

    /// Returns the registered scalar default for one parameter slot, if any.
    pub fn param_default(&self, index: usize) -> Option<&NativeCallableDefault> {
        self.param_defaults.get(index).and_then(Option::as_ref)
    }

    /// Returns the compiled fallback helper for one optional slot, if registered.
    pub fn compiled_param_default(&self, index: usize) -> Option<usize> {
        self.compiled_param_defaults.get(index).copied().flatten()
    }

    /// Returns whether one registered parameter is by-reference.
    pub fn param_by_ref(&self, index: usize) -> bool {
        self.param_by_ref.get(index).copied().unwrap_or(false)
    }

    /// Returns whether one registered parameter is the variadic parameter.
    pub fn param_variadic(&self, index: usize) -> bool {
        self.variadic_index == Some(index)
    }

    /// Returns whether eval may dispatch this callable through the generated bridge.
    pub const fn bridge_supported(&self) -> bool {
        self.bridge_supported
    }

    /// Records the explicit PHP signature shape emitted by the generated bridge.
    pub fn set_shape(&mut self, shape: NativeCallableShape) {
        self.shape = Some(shape);
    }

    /// Returns the resolved PHP-visible / compiler-internal partition of the physical slots.
    pub fn frame_shape(&self) -> NativeCallableFrameShape {
        NativeCallableFrameShape::new(self.param_count, self.variadic_index, self.shape)
    }

    /// Returns the minimum number of arguments this callable requires.
    ///
    /// The registered shape is authoritative. Only a registration that emitted no shape falls
    /// back to reading the registered defaults, which is unavoidable there and safe because such
    /// a registration also declares no hidden slots.
    pub fn required_param_count(&self) -> usize {
        let shape = self.frame_shape();
        if let Some(required) = shape.declared_required_param_count() {
            return required;
        }
        (0..shape.visible_regular_param_count())
            .rfind(|index| self.param_default(*index).is_none())
            .map_or(0, |index| index + 1)
    }

    /// Returns whether one physical slot is a compiler-internal parameter rather than a PHP one.
    pub fn param_is_hidden(&self, index: usize) -> bool {
        self.frame_shape().param_is_hidden(index)
    }

    /// Returns how many leading physical slots are PHP-visible NON-variadic parameters.
    ///
    /// This is the number of slots a positional argument may fill before it belongs to the tail,
    /// and the range over which a required-argument count is meaningful.
    pub fn visible_regular_param_count(&self) -> usize {
        self.frame_shape().visible_regular_param_count()
    }

    /// Returns the physical slots a PHP caller may bind, in declaration order.
    pub fn visible_param_indexes(&self) -> Vec<usize> {
        self.frame_shape().visible_param_indexes()
    }

    /// Returns the variadic slot the PHP source itself declared, if the source declared one.
    pub fn source_variadic_index(&self) -> Option<usize> {
        self.frame_shape().source_variadic_index()
    }

    /// Returns the hidden surplus-argument collector slot, if this frame carries one.
    pub fn hidden_collector_index(&self) -> Option<usize> {
        self.frame_shape().hidden_collector_index()
    }

    /// Returns the hidden actual-argument-count slot, which follows the visible regulars.
    pub fn hidden_argc_index(&self) -> Option<usize> {
        self.frame_shape().hidden_argc_index()
    }

    /// Returns whether the hidden collector must begin with the actual PHP argument count.
    pub fn collector_needs_count(&self) -> bool {
        self.frame_shape().collector_needs_count()
    }

    /// Returns the registered declared return type metadata, if any.
    pub fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeCallableDefault, NativeCallableFrameShape, NativeCallableShape,
        NativeCallableSignature, NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT,
        NATIVE_SHAPE_FLAG_SOURCE_VARIADIC,
    };

    /// Builds the signature a generated bridge registers for `f($a, $b = X, ...$rest)`.
    ///
    /// The physical slots are `[a, b, __elephc_func_argc, rest]`: the hidden count parameter sits
    /// between the visible regulars and the source variadic. `X` is deliberately NOT registered
    /// as a default, which is exactly what happens for an enum case or a deeply nested constant.
    fn source_variadic_with_hidden_argc() -> NativeCallableSignature {
        let mut signature = NativeCallableSignature::new(4);
        signature.set_param_name(0, "a");
        signature.set_param_name(1, "b");
        signature.set_param_name(2, "");
        signature.set_param_name(3, "rest");
        signature.set_variadic_index(3);
        signature.set_shape(NativeCallableShape::new(2, 1, true, false));
        signature
    }

    /// Builds the signature for `f($a, $b = X)` in a frame that carries the hidden collector.
    ///
    /// The physical slots are `[a, b, __elephc_func_args]`, and the collector's first element
    /// carries the actual argument count because a visible regular is optional.
    fn hidden_collector_with_count() -> NativeCallableSignature {
        let mut signature = NativeCallableSignature::new(3);
        signature.set_param_name(0, "a");
        signature.set_param_name(1, "b");
        signature.set_param_name(2, "");
        signature.set_variadic_index(2);
        signature.set_shape(NativeCallableShape::new(2, 1, false, true));
        signature
    }

    /// An unrepresentable optional default still reports the source's required count.
    ///
    /// This is the whole point of registering the shape: deriving arity from the registered
    /// defaults would call `$b` mandatory here, because no default was registered for it.
    #[test]
    fn required_count_comes_from_the_shape_not_from_registered_defaults() {
        let signature = source_variadic_with_hidden_argc();
        assert_eq!(signature.param_default(1), None);
        assert_eq!(signature.required_param_count(), 1);

        let collector = hidden_collector_with_count();
        assert_eq!(collector.param_default(1), None);
        assert_eq!(collector.required_param_count(), 1);
    }

    /// A compiled fallback fills the physical slot without becoming reflection value metadata.
    #[test]
    fn compiled_defaults_stay_separate_from_php_value_metadata() {
        let mut signature = hidden_collector_with_count();
        assert!(signature.set_compiled_param_default(1, 0x1234));
        assert_eq!(signature.param_default(1), None);
        assert_eq!(signature.compiled_param_default(1), Some(0x1234));
        assert_eq!(signature.required_param_count(), 1);
        assert!(signature.collector_needs_count());
        assert!(!signature.set_compiled_param_default(1, 0));
    }

    /// The collector's count prefix is declared metadata, never inferred from a default.
    #[test]
    fn collector_count_prefix_comes_from_the_shape() {
        let collector = hidden_collector_with_count();
        assert_eq!(collector.hidden_collector_index(), Some(2));
        assert!(collector.collector_needs_count());
        assert_eq!(collector.source_variadic_index(), None);

        // The same physical layout without the flag carries no count prefix, even though the
        // registered defaults are identical (that is, absent).
        let mut without_count = hidden_collector_with_count();
        without_count.set_shape(NativeCallableShape::new(2, 1, false, false));
        assert!(!without_count.collector_needs_count());
    }

    /// Hidden slots are exactly the physical suffix outside the visible source slots.
    #[test]
    fn hidden_slots_are_the_suffix_outside_the_visible_source_slots() {
        for signature in [source_variadic_with_hidden_argc(), hidden_collector_with_count()] {
            let visible = signature.visible_param_indexes();
            let hidden = (0..signature.param_count())
                .filter(|index| signature.param_is_hidden(*index))
                .collect::<Vec<_>>();
            let mut union = visible.clone();
            union.extend(hidden.iter().copied());
            union.sort_unstable();
            assert_eq!(union, (0..signature.param_count()).collect::<Vec<_>>());
            assert!(hidden.iter().all(|index| !visible.contains(index)));
            assert!(hidden
                .iter()
                .all(|index| *index >= signature.visible_regular_param_count()));
        }
    }

    /// A source variadic keeps its own trailing slot behind the hidden count parameter.
    #[test]
    fn source_variadic_keeps_its_trailing_slot_behind_the_hidden_count() {
        let signature = source_variadic_with_hidden_argc();
        assert_eq!(signature.visible_regular_param_count(), 2);
        assert_eq!(signature.visible_param_indexes(), vec![0, 1, 3]);
        assert_eq!(signature.source_variadic_index(), Some(3));
        assert_eq!(signature.hidden_collector_index(), None);
        assert_eq!(signature.hidden_argc_index(), Some(2));
        assert!(!signature.collector_needs_count());
    }

    /// Absent shape metadata keeps every physical slot PHP-visible, names or no names.
    ///
    /// A legacy or foreign registration must never have its trailing parameters hidden. This is
    /// the case that a spelling-based partition got wrong: it read "no registered name" as
    /// "compiler-internal slot".
    #[test]
    fn a_registration_without_a_shape_keeps_every_slot_visible() {
        let mut no_names = NativeCallableSignature::new(3);
        no_names.set_param_default(2, NativeCallableDefault::Int(7));
        assert_eq!(no_names.visible_param_indexes(), vec![0, 1, 2]);
        assert!((0..3).all(|index| !no_names.param_is_hidden(index)));
        assert_eq!(no_names.required_param_count(), 2);
        assert_eq!(no_names.hidden_argc_index(), None);
        assert!(!no_names.collector_needs_count());

        // An incompletely named signature is still fully visible for the same reason.
        let mut partial_names = NativeCallableSignature::new(3);
        partial_names.set_param_name(0, "a");
        assert_eq!(partial_names.visible_param_indexes(), vec![0, 1, 2]);
        assert!((0..3).all(|index| !partial_names.param_is_hidden(index)));

        // A foreign variadic registration reports a PHP variadic, not a hidden collector.
        let mut foreign_variadic = NativeCallableSignature::new(2);
        foreign_variadic.set_variadic_index(1);
        assert_eq!(foreign_variadic.visible_regular_param_count(), 1);
        assert_eq!(foreign_variadic.source_variadic_index(), Some(1));
        assert_eq!(foreign_variadic.hidden_collector_index(), None);
        assert_eq!(foreign_variadic.visible_param_indexes(), vec![0, 1]);
    }

    /// The ABI words decode to the same shape the accessors read.
    #[test]
    fn shape_abi_words_round_trip_through_the_registration_decoder() {
        let flags = NATIVE_SHAPE_FLAG_SOURCE_VARIADIC | NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT;
        assert_eq!(
            NativeCallableShape::from_abi(2, 1, flags),
            Some(NativeCallableShape::new(2, 1, true, true))
        );
        assert_eq!(
            NativeCallableShape::from_abi(2, 1, 0),
            Some(NativeCallableShape::new(2, 1, false, false))
        );
        // A required count past the visible regulars is clamped rather than trusted.
        assert_eq!(
            NativeCallableShape::from_abi(1, 9, 0),
            Some(NativeCallableShape::new(1, 1, false, false))
        );
        // Unknown flag bits are ignored rather than treated as a malformed registration.
        assert_eq!(
            NativeCallableShape::from_abi(2, 2, 1 << 20),
            Some(NativeCallableShape::new(2, 2, false, false))
        );
    }

    /// A shape claiming more visible regulars than the bridge has slots is ignored, not obeyed.
    #[test]
    fn an_over_wide_shape_falls_back_to_a_fully_visible_frame() {
        let shape = NativeCallableFrameShape::new(2, None, Some(NativeCallableShape::new(5, 5, false, false)));
        assert_eq!(shape.visible_regular_param_count(), 2);
        assert_eq!(shape.declared_required_param_count(), None);
        assert!(!shape.param_is_hidden(1));
    }
}
