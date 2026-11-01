//! Purpose:
//! SPL metadata-required method collection.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Adds builtin SPL methods required by runtime class/interface metadata.
pub(super) fn push_builtin_spl_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let mut current = Some(class_name);
    while let Some(name) = current {
        push_builtin_spl_interface_metadata_methods(methods, module, name);
        for method_name in required_builtin_spl_metadata_methods(name) {
            let method_key = php_method_key(method_name);
            if is_supported_builtin_spl_method(name, &method_key) {
                methods.push((name.to_string(), method_key));
            }
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
}

/// Adds builtin SPL methods referenced by runtime interface dispatch tables for one class.
pub(super) fn push_builtin_spl_interface_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let mut seen = HashSet::new();
    let mut stack = class_info
        .interfaces
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = module.interface_infos.get(interface_name) else {
            continue;
        };
        for method_key in &interface_info.method_order {
            if let Some(impl_class) = class_info.method_impl_classes.get(method_key) {
                if is_supported_builtin_spl_method(impl_class, method_key) {
                    methods.push((impl_class.clone(), method_key.clone()));
                    continue;
                }
            }
            push_supported_builtin_spl_method_for_receiver(methods, module, class_name, method_key);
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
}

/// Returns methods needed even when user code does not call them directly.
pub(super) fn required_builtin_spl_metadata_methods(class_name: &str) -> &'static [&'static str] {
    match class_name {
        // Every Throwable can be stringified through a dynamic `__toString()` dispatch (an
        // `echo $e` on a boxed value), and those slots used to stay empty.
        "Exception" | "Error" => &["__toString", "getTraceAsString"],
        "EmptyIterator" => &["current", "key", "next", "rewind", "valid"],
        "ArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        // `$o[] = $v` reaches `append()` through the vtable; it was kept only while `offsetSet()`
        // called it, and an emptied slot made the first push jump to address 0.
        "ArrayObject" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "append",
            "getArrayCopy",
            "exchangeArray",
            "__serialize",
            "__unserialize",
        ],
        "SplFixedArray" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "jsonSerialize",
        ],
        "InternalIterator" => &["current", "key", "next", "rewind", "valid"],
        "IteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "LimitIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "getPosition",
        ],
        "NoRewindIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "InfiniteIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "FilterIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "CallbackFilterIterator" => &["accept"],
        "CachingIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "hasNext",
            "__toString",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "getCache",
            "count",
        ],
        "AppendIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "MultipleIterator" => &["current", "key", "next", "rewind", "valid"],
        "__ElephcAppendIteratorArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        "SplDoublyLinkedList" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "SplHeap" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplMaxHeap" | "SplMinHeap" => &["compare"],
        "SplPriorityQueue" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplObjectStorage" | "WeakMap" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "RegexIterator" => &["accept", "current", "key"],
        "RecursiveArrayIterator" => &["hasChildren", "getChildren"],
        "RecursiveFilterIterator" => &["hasChildren"],
        "RecursiveCallbackFilterIterator" => &["hasChildren", "getChildren"],
        "RecursiveRegexIterator" => &["accept", "current", "key", "hasChildren", "getChildren"],
        "ParentIterator" => &["accept", "getChildren"],
        "RecursiveIteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "RecursiveTreeIterator" => &["setPrefixPart"],
        "SplFileInfo" => &["__toString"],
        "SplFileObject" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "hasChildren",
            "getChildren",
        ],
        "DirectoryIterator" => &["current", "key", "next", "rewind", "valid", "seek"],
        "FilesystemIterator" => &["current", "key"],
        "GlobIterator" => &["count"],
        "RecursiveDirectoryIterator" => &["hasChildren", "getChildren"],
        "RecursiveCachingIterator" => &["hasChildren", "getChildren"],
        _ => &[],
    }
}
