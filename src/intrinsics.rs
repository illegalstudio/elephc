//! Purpose:
//! Defines compiler-recognized intrinsic method calls for runtime-managed core objects.
//! Centralizes the small set of method calls that must bypass normal PHP method bodies.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Intrinsics preserve PHP-facing class/method signatures while routing codegen directly to runtime helpers.

use crate::names::php_symbol_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Form of intrinsic call.
pub enum IntrinsicCallForm {
    Instance,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Kind of intrinsic call.
pub enum IntrinsicCallKind {
    FiberStart,
    FiberResume,
    FiberThrow,
    FiberGetReturn,
    FiberIsStarted,
    FiberIsRunning,
    FiberIsSuspended,
    FiberIsTerminated,
    FiberSuspend,
    FiberGetCurrent,
    GeneratorCurrent,
    GeneratorKey,
    GeneratorNext,
    GeneratorValid,
    GeneratorRewind,
    GeneratorSend,
    GeneratorThrow,
    GeneratorGetReturn,
    SplDllAdd,
    SplDllPop,
    SplDllShift,
    SplDllPush,
    SplDllUnshift,
    SplDllTop,
    SplDllBottom,
    SplDllCount,
    SplDllIsEmpty,
    SplDllSetIteratorMode,
    SplDllGetIteratorMode,
    SplDllSerialize,
    SplDllUnserialize,
    SplDllSerializeArray,
    SplDllOffsetExists,
    SplDllOffsetGet,
    SplDllOffsetSet,
    SplDllOffsetUnset,
    SplDllRewind,
    SplDllCurrent,
    SplDllKey,
    SplDllPrev,
    SplDllNext,
    SplDllValid,
    SplQueueEnqueue,
    SplQueueDequeue,
    SplFixedConstruct,
    SplFixedCount,
    SplFixedToArray,
    SplFixedGetSize,
    SplFixedSetSize,
    SplFixedFromArray,
    SplFixedOffsetExists,
    SplFixedOffsetGet,
    SplFixedOffsetSet,
    SplFixedOffsetUnset,
    SplFixedJsonSerialize,
    SplFixedUnserialize,
    CallbackFilterAccept,
    SplRecursiveAssumeIterator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Intrinsic call representation.
pub struct IntrinsicCall {
    kind: IntrinsicCallKind,
    form: IntrinsicCallForm,
    spec_index: usize,
}

#[derive(Debug, Clone, Copy)]
/// Static specification for a compiler-recognized intrinsic method.
/// Contains the class/method identity and the corresponding runtime helper name.
struct IntrinsicSpec {
    kind: IntrinsicCallKind,
    form: IntrinsicCallForm,
    class_key: &'static str,
    class_name: &'static str,
    method_key: &'static str,
    runtime_helper: Option<&'static str>,
}

const INTRINSICS: &[IntrinsicSpec] = &[
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberStart,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "start",
        runtime_helper: Some("__rt_fiber_start"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberResume,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "resume",
        runtime_helper: Some("__rt_fiber_resume"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberThrow,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "throw",
        runtime_helper: Some("__rt_fiber_throw"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberGetReturn,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "getreturn",
        runtime_helper: Some("__rt_fiber_get_return"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsStarted,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isstarted",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsRunning,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isrunning",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsSuspended,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "issuspended",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsTerminated,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isterminated",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberSuspend,
        form: IntrinsicCallForm::Static,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "suspend",
        runtime_helper: Some("__rt_fiber_suspend"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberGetCurrent,
        form: IntrinsicCallForm::Static,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "getcurrent",
        runtime_helper: Some("__rt_fiber_get_current"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorCurrent,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "current",
        runtime_helper: Some("__rt_gen_current"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorKey,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "key",
        runtime_helper: Some("__rt_gen_key"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorNext,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "next",
        runtime_helper: Some("__rt_gen_next"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorValid,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "valid",
        runtime_helper: Some("__rt_gen_valid"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorRewind,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "rewind",
        runtime_helper: Some("__rt_gen_rewind"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorSend,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "send",
        runtime_helper: Some("__rt_gen_send"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorThrow,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "throw",
        runtime_helper: Some("__rt_gen_throw"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorGetReturn,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "getreturn",
        runtime_helper: Some("__rt_gen_get_return"),
    },
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "add", IntrinsicCallKind::SplDllAdd, "__rt_spl_dll_insert"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "pop", IntrinsicCallKind::SplDllPop, "__rt_spl_dll_pop"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "shift", IntrinsicCallKind::SplDllShift, "__rt_spl_dll_shift"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "push", IntrinsicCallKind::SplDllPush, "__rt_spl_dll_push"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "unshift", IntrinsicCallKind::SplDllUnshift, "__rt_spl_dll_unshift"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "top", IntrinsicCallKind::SplDllTop, "__rt_spl_dll_top"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "bottom", IntrinsicCallKind::SplDllBottom, "__rt_spl_dll_bottom"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "count", IntrinsicCallKind::SplDllCount, "__rt_spl_dll_count"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "isempty", IntrinsicCallKind::SplDllIsEmpty, "__rt_spl_dll_is_empty"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "setiteratormode", IntrinsicCallKind::SplDllSetIteratorMode, "__rt_spl_dll_set_iterator_mode"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "getiteratormode", IntrinsicCallKind::SplDllGetIteratorMode, "__rt_spl_dll_get_iterator_mode"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "serialize", IntrinsicCallKind::SplDllSerialize, "__rt_spl_dll_serialize"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "unserialize", IntrinsicCallKind::SplDllUnserialize, "__rt_spl_dll_unserialize"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "__serialize", IntrinsicCallKind::SplDllSerializeArray, "__rt_spl_dll_serialize_array"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "offsetexists", IntrinsicCallKind::SplDllOffsetExists, "__rt_spl_dll_offset_exists"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "offsetget", IntrinsicCallKind::SplDllOffsetGet, "__rt_spl_dll_offset_get"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "offsetset", IntrinsicCallKind::SplDllOffsetSet, "__rt_spl_dll_offset_set"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "offsetunset", IntrinsicCallKind::SplDllOffsetUnset, "__rt_spl_dll_offset_unset"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "rewind", IntrinsicCallKind::SplDllRewind, "__rt_spl_dll_rewind"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "current", IntrinsicCallKind::SplDllCurrent, "__rt_spl_dll_current"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "key", IntrinsicCallKind::SplDllKey, "__rt_spl_dll_key"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "prev", IntrinsicCallKind::SplDllPrev, "__rt_spl_dll_prev"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "next", IntrinsicCallKind::SplDllNext, "__rt_spl_dll_next"),
    spl_instance_spec("spldoublylinkedlist", "SplDoublyLinkedList", "valid", IntrinsicCallKind::SplDllValid, "__rt_spl_dll_valid"),
    spl_instance_spec("splstack", "SplStack", "add", IntrinsicCallKind::SplDllAdd, "__rt_spl_dll_insert"),
    spl_instance_spec("splstack", "SplStack", "pop", IntrinsicCallKind::SplDllPop, "__rt_spl_dll_pop"),
    spl_instance_spec("splstack", "SplStack", "shift", IntrinsicCallKind::SplDllShift, "__rt_spl_dll_shift"),
    spl_instance_spec("splstack", "SplStack", "push", IntrinsicCallKind::SplDllPush, "__rt_spl_dll_push"),
    spl_instance_spec("splstack", "SplStack", "unshift", IntrinsicCallKind::SplDllUnshift, "__rt_spl_dll_unshift"),
    spl_instance_spec("splstack", "SplStack", "top", IntrinsicCallKind::SplDllTop, "__rt_spl_dll_top"),
    spl_instance_spec("splstack", "SplStack", "bottom", IntrinsicCallKind::SplDllBottom, "__rt_spl_dll_bottom"),
    spl_instance_spec("splstack", "SplStack", "count", IntrinsicCallKind::SplDllCount, "__rt_spl_dll_count"),
    spl_instance_spec("splstack", "SplStack", "isempty", IntrinsicCallKind::SplDllIsEmpty, "__rt_spl_dll_is_empty"),
    spl_instance_spec("splstack", "SplStack", "setiteratormode", IntrinsicCallKind::SplDllSetIteratorMode, "__rt_spl_dll_set_iterator_mode"),
    spl_instance_spec("splstack", "SplStack", "getiteratormode", IntrinsicCallKind::SplDllGetIteratorMode, "__rt_spl_dll_get_iterator_mode"),
    spl_instance_spec("splstack", "SplStack", "serialize", IntrinsicCallKind::SplDllSerialize, "__rt_spl_dll_serialize"),
    spl_instance_spec("splstack", "SplStack", "unserialize", IntrinsicCallKind::SplDllUnserialize, "__rt_spl_dll_unserialize"),
    spl_instance_spec("splstack", "SplStack", "__serialize", IntrinsicCallKind::SplDllSerializeArray, "__rt_spl_dll_serialize_array"),
    spl_instance_spec("splstack", "SplStack", "offsetexists", IntrinsicCallKind::SplDllOffsetExists, "__rt_spl_dll_offset_exists"),
    spl_instance_spec("splstack", "SplStack", "offsetget", IntrinsicCallKind::SplDllOffsetGet, "__rt_spl_dll_offset_get"),
    spl_instance_spec("splstack", "SplStack", "offsetset", IntrinsicCallKind::SplDllOffsetSet, "__rt_spl_dll_offset_set"),
    spl_instance_spec("splstack", "SplStack", "offsetunset", IntrinsicCallKind::SplDllOffsetUnset, "__rt_spl_dll_offset_unset"),
    spl_instance_spec("splstack", "SplStack", "rewind", IntrinsicCallKind::SplDllRewind, "__rt_spl_dll_rewind"),
    spl_instance_spec("splstack", "SplStack", "current", IntrinsicCallKind::SplDllCurrent, "__rt_spl_dll_current"),
    spl_instance_spec("splstack", "SplStack", "key", IntrinsicCallKind::SplDllKey, "__rt_spl_dll_key"),
    spl_instance_spec("splstack", "SplStack", "prev", IntrinsicCallKind::SplDllPrev, "__rt_spl_dll_prev"),
    spl_instance_spec("splstack", "SplStack", "next", IntrinsicCallKind::SplDllNext, "__rt_spl_dll_next"),
    spl_instance_spec("splstack", "SplStack", "valid", IntrinsicCallKind::SplDllValid, "__rt_spl_dll_valid"),
    spl_instance_spec("splqueue", "SplQueue", "add", IntrinsicCallKind::SplDllAdd, "__rt_spl_dll_insert"),
    spl_instance_spec("splqueue", "SplQueue", "pop", IntrinsicCallKind::SplDllPop, "__rt_spl_dll_pop"),
    spl_instance_spec("splqueue", "SplQueue", "shift", IntrinsicCallKind::SplDllShift, "__rt_spl_dll_shift"),
    spl_instance_spec("splqueue", "SplQueue", "push", IntrinsicCallKind::SplDllPush, "__rt_spl_dll_push"),
    spl_instance_spec("splqueue", "SplQueue", "unshift", IntrinsicCallKind::SplDllUnshift, "__rt_spl_dll_unshift"),
    spl_instance_spec("splqueue", "SplQueue", "top", IntrinsicCallKind::SplDllTop, "__rt_spl_dll_top"),
    spl_instance_spec("splqueue", "SplQueue", "bottom", IntrinsicCallKind::SplDllBottom, "__rt_spl_dll_bottom"),
    spl_instance_spec("splqueue", "SplQueue", "count", IntrinsicCallKind::SplDllCount, "__rt_spl_dll_count"),
    spl_instance_spec("splqueue", "SplQueue", "isempty", IntrinsicCallKind::SplDllIsEmpty, "__rt_spl_dll_is_empty"),
    spl_instance_spec("splqueue", "SplQueue", "setiteratormode", IntrinsicCallKind::SplDllSetIteratorMode, "__rt_spl_dll_set_iterator_mode"),
    spl_instance_spec("splqueue", "SplQueue", "getiteratormode", IntrinsicCallKind::SplDllGetIteratorMode, "__rt_spl_dll_get_iterator_mode"),
    spl_instance_spec("splqueue", "SplQueue", "serialize", IntrinsicCallKind::SplDllSerialize, "__rt_spl_dll_serialize"),
    spl_instance_spec("splqueue", "SplQueue", "unserialize", IntrinsicCallKind::SplDllUnserialize, "__rt_spl_dll_unserialize"),
    spl_instance_spec("splqueue", "SplQueue", "__serialize", IntrinsicCallKind::SplDllSerializeArray, "__rt_spl_dll_serialize_array"),
    spl_instance_spec("splqueue", "SplQueue", "offsetexists", IntrinsicCallKind::SplDllOffsetExists, "__rt_spl_dll_offset_exists"),
    spl_instance_spec("splqueue", "SplQueue", "offsetget", IntrinsicCallKind::SplDllOffsetGet, "__rt_spl_dll_offset_get"),
    spl_instance_spec("splqueue", "SplQueue", "offsetset", IntrinsicCallKind::SplDllOffsetSet, "__rt_spl_dll_offset_set"),
    spl_instance_spec("splqueue", "SplQueue", "offsetunset", IntrinsicCallKind::SplDllOffsetUnset, "__rt_spl_dll_offset_unset"),
    spl_instance_spec("splqueue", "SplQueue", "rewind", IntrinsicCallKind::SplDllRewind, "__rt_spl_dll_rewind"),
    spl_instance_spec("splqueue", "SplQueue", "current", IntrinsicCallKind::SplDllCurrent, "__rt_spl_dll_current"),
    spl_instance_spec("splqueue", "SplQueue", "key", IntrinsicCallKind::SplDllKey, "__rt_spl_dll_key"),
    spl_instance_spec("splqueue", "SplQueue", "prev", IntrinsicCallKind::SplDllPrev, "__rt_spl_dll_prev"),
    spl_instance_spec("splqueue", "SplQueue", "next", IntrinsicCallKind::SplDllNext, "__rt_spl_dll_next"),
    spl_instance_spec("splqueue", "SplQueue", "valid", IntrinsicCallKind::SplDllValid, "__rt_spl_dll_valid"),
    spl_instance_spec("splqueue", "SplQueue", "enqueue", IntrinsicCallKind::SplQueueEnqueue, "__rt_spl_dll_push"),
    spl_instance_spec("splqueue", "SplQueue", "dequeue", IntrinsicCallKind::SplQueueDequeue, "__rt_spl_dll_shift"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "__construct", IntrinsicCallKind::SplFixedConstruct, "__rt_spl_fixed_set_size"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "count", IntrinsicCallKind::SplFixedCount, "__rt_spl_fixed_count"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "toarray", IntrinsicCallKind::SplFixedToArray, "__rt_spl_fixed_to_array"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "getsize", IntrinsicCallKind::SplFixedGetSize, "__rt_spl_fixed_count"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "setsize", IntrinsicCallKind::SplFixedSetSize, "__rt_spl_fixed_set_size"),
    spl_static_spec("splfixedarray", "SplFixedArray", "fromarray", IntrinsicCallKind::SplFixedFromArray, "__rt_spl_fixed_from_array"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "offsetexists", IntrinsicCallKind::SplFixedOffsetExists, "__rt_spl_fixed_offset_exists"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "offsetget", IntrinsicCallKind::SplFixedOffsetGet, "__rt_spl_fixed_offset_get"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "offsetset", IntrinsicCallKind::SplFixedOffsetSet, "__rt_spl_fixed_offset_set"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "offsetunset", IntrinsicCallKind::SplFixedOffsetUnset, "__rt_spl_fixed_offset_unset"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "jsonserialize", IntrinsicCallKind::SplFixedJsonSerialize, "__rt_spl_fixed_to_array"),
    spl_instance_spec("splfixedarray", "SplFixedArray", "__unserialize", IntrinsicCallKind::SplFixedUnserialize, "__rt_spl_fixed_unserialize"),
    IntrinsicSpec {
        kind: IntrinsicCallKind::CallbackFilterAccept,
        form: IntrinsicCallForm::Instance,
        class_key: "callbackfilteriterator",
        class_name: "CallbackFilterIterator",
        method_key: "__elephcacceptcallback",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::CallbackFilterAccept,
        form: IntrinsicCallForm::Instance,
        class_key: "recursivecallbackfilteriterator",
        class_name: "RecursiveCallbackFilterIterator",
        method_key: "__elephcacceptcallback",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursivearrayiterator",
        class_name: "RecursiveArrayIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursivefilteriterator",
        class_name: "RecursiveFilterIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursivecallbackfilteriterator",
        class_name: "RecursiveCallbackFilterIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursiveiteratoriterator",
        class_name: "RecursiveIteratorIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "parentiterator",
        class_name: "ParentIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursiveregexiterator",
        class_name: "RecursiveRegexIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::SplRecursiveAssumeIterator,
        form: IntrinsicCallForm::Instance,
        class_key: "recursivecachingiterator",
        class_name: "RecursiveCachingIterator",
        method_key: "__elephcassumerecursiveiterator",
        runtime_helper: None,
    },
];

/// Constructs an instance-method intrinsic spec for a Spl class.
const fn spl_instance_spec(
    class_key: &'static str,
    class_name: &'static str,
    method_key: &'static str,
    kind: IntrinsicCallKind,
    runtime_helper: &'static str,
) -> IntrinsicSpec {
    IntrinsicSpec {
        kind,
        form: IntrinsicCallForm::Instance,
        class_key,
        class_name,
        method_key,
        runtime_helper: Some(runtime_helper),
    }
}

/// Constructs a static-method intrinsic spec for a Spl class.
const fn spl_static_spec(
    class_key: &'static str,
    class_name: &'static str,
    method_key: &'static str,
    kind: IntrinsicCallKind,
    runtime_helper: &'static str,
) -> IntrinsicSpec {
    IntrinsicSpec {
        kind,
        form: IntrinsicCallForm::Static,
        class_key,
        class_name,
        method_key,
        runtime_helper: Some(runtime_helper),
    }
}

impl IntrinsicCall {
    /// Looks up an instance method as an intrinsic, returning the call descriptor if found.
    /// Matching is case-insensitive per PHP method lookup rules.
    pub fn instance_method(class_name: &str, method: &str) -> Option<Self> {
        resolve(IntrinsicCallForm::Instance, class_name, method)
    }

    /// Looks up a static method as an intrinsic, returning the call descriptor if found.
    /// Matching is case-insensitive per PHP method lookup rules.
    pub fn static_method(class_name: &str, method: &str) -> Option<Self> {
        resolve(IntrinsicCallForm::Static, class_name, method)
    }

    /// Returns the specific kind of intrinsic call (e.g., FiberStart, GeneratorCurrent).
    pub fn kind(self) -> IntrinsicCallKind {
        self.kind
    }

    /// Returns whether this intrinsic is called on an instance or statically.
    pub fn form(self) -> IntrinsicCallForm {
        self.form
    }

    /// Returns the PHP class name this intrinsic is registered under.
    pub fn class_name(self) -> &'static str {
        INTRINSICS[self.spec_index].class_name
    }

    /// Returns the lowercase method key used for intrinsic lookup.
    pub fn method_key(self) -> &'static str {
        INTRINSICS[self.spec_index].method_key
    }

    /// Returns the name of the runtime helper that implements this intrinsic,
    /// or None if the intrinsic has no separate helper (shouldn't happen in practice).
    pub fn runtime_helper(self) -> Option<&'static str> {
        INTRINSICS[self.spec_index].runtime_helper
    }
}

/// Looks up an intrinsic by call form, PHP class name, and method name.
fn resolve(form: IntrinsicCallForm, class_name: &str, method: &str) -> Option<IntrinsicCall> {
    let class_key = php_symbol_key(class_name);
    let method_key = php_symbol_key(method);
    INTRINSICS
        .iter()
        .enumerate()
        .find(|spec| {
            spec.1.form == form && spec.1.class_key == class_key && spec.1.method_key == method_key
        })
        .map(|(spec_index, spec)| IntrinsicCall {
            kind: spec.kind,
            form: spec.form,
            spec_index,
        })
}

#[cfg(test)]
mod tests {
    use super::{IntrinsicCall, IntrinsicCallForm, IntrinsicCallKind};

    /// Provides the Resolves generator methods case insensitively helper used by the intrinsics module.
    #[test]
    fn resolves_generator_methods_case_insensitively() {
        let call = IntrinsicCall::instance_method("Generator", "getReturn")
            .expect("Generator::getReturn should be intrinsic");

        assert_eq!(call.kind(), IntrinsicCallKind::GeneratorGetReturn);
        assert_eq!(call.form(), IntrinsicCallForm::Instance);
        assert_eq!(call.runtime_helper(), Some("__rt_gen_get_return"));
    }

    /// Builds the method list for separates static and instance fiber.
    #[test]
    fn separates_static_and_instance_fiber_methods() {
        assert!(IntrinsicCall::instance_method("Fiber", "suspend").is_none());
        assert!(IntrinsicCall::static_method("Fiber", "suspend").is_some());
        assert!(IntrinsicCall::instance_method("Fiber", "start").is_some());
        assert!(IntrinsicCall::static_method("Fiber", "start").is_none());
    }

    /// Provides the Ignores user classes with matching method names helper used by the intrinsics module.
    #[test]
    fn ignores_user_classes_with_matching_method_names() {
        assert!(IntrinsicCall::instance_method("UserFiber", "start").is_none());
        assert!(IntrinsicCall::instance_method("Box", "current").is_none());
    }
}
