//! Purpose:
//! Supported builtin SPL method inventory and intrinsic wrapper selection.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Returns true for builtin SPL methods intentionally lowered into EIR today.
pub(super) fn is_supported_builtin_spl_method(class_name: &str, method_key: &str) -> bool {
    match class_name {
        "SplFileInfo" => matches!(
            method_key,
            "__construct"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
                | "getfileinfo"
                | "getpathinfo"
                | "setinfoclass"
                | "openfile"
                | "setfileclass"
        ),
        "SplFileObject" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "haschildren"
                | "getchildren"
                | "eof"
                | "fgets"
                | "getcurrentline"
                | "fgetc"
                | "fread"
                | "fwrite"
                | "ftruncate"
                | "ftell"
                | "fseek"
                | "getflags"
                | "setflags"
                | "getmaxlinelen"
                | "setmaxlinelen"
                | "setcsvcontrol"
                | "getcsvcontrol"
                | "fgetcsv"
                | "fputcsv"
                | "fscanf"
                // The READ_CSV record builder. A prelude body that is DECLARED but missing
                // from this list is never lowered, and its vtable slot stays null: the call
                // branches to address 0 rather than failing to compile, so an omission here
                // shows up as a segfault at the first call and nowhere earlier.
                | "__elephccsvbuild"
                | "__elephccsvskipblank"
        ),
        "SplTempFileObject" => matches!(
            method_key,
            "__construct"
                | "eof"
                | "fgetc"
                | "fflush"
                | "fgets"
                | "fread"
                | "fwrite"
                | "fstat"
                | "ftell"
                | "fseek"
                | "ftruncate"
                | "rewind"
                | "__elephcspilltofile"
        ),
        "DirectoryIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "seek"
                | "valid"
                | "isdot"
                | "__tostring"
                | "__elephcrefreshpath"
        ),
        "FilesystemIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "getflags" | "setflags"
        ),
        "GlobIterator" => matches!(method_key, "__construct" | "count" | "setflags"),
        "RecursiveDirectoryIterator" => {
            matches!(method_key, "__construct" | "haschildren" | "getchildren")
        }
        "RecursiveCachingIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "EmptyIterator" => matches!(method_key, "current" | "key" | "next" | "rewind" | "valid"),
        "ArrayIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "ArrayObject" => matches!(
            method_key,
            "__construct"
                | "getiterator"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "SplFixedArray" => matches!(
            method_key,
            "__construct"
                | "__wakeup"
                | "__serialize"
                | "__unserialize"
                | "count"
                | "getiterator"
                | "toarray"
                | "getsize"
                | "setsize"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "jsonserialize"
        ),
        "InternalIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "next" | "rewind" | "valid"
        ),
        "SplDoublyLinkedList" | "SplStack" | "SplQueue" => matches!(
            method_key,
            "add"
                | "pop"
                | "shift"
                | "push"
                | "unshift"
                | "top"
                | "bottom"
                | "count"
                | "isempty"
                | "setiteratormode"
                | "getiteratormode"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "rewind"
                | "current"
                | "key"
                | "prev"
                | "next"
                | "valid"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "enqueue"
                | "dequeue"
        ),
        "SplHeap" => matches!(
            method_key,
            "__construct"
                | "insert"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "compare"
                | "__elephcbestindex"
                | "__elephcremoveat"
        ),
        "SplMaxHeap" | "SplMinHeap" => matches!(method_key, "compare"),
        "SplPriorityQueue" => matches!(
            method_key,
            "__construct"
                | "compare"
                | "insert"
                | "setextractflags"
                | "getextractflags"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "__elephcbestindex"
                | "__elephcoutputat"
                | "__elephcremoveat"
        ),
        "SplObjectStorage" => matches!(
            method_key,
            "__construct"
                | "attach"
                | "detach"
                | "contains"
                | "addall"
                | "removeall"
                | "removeallexcept"
                | "getinfo"
                | "setinfo"
                | "count"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
                | "seek"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "gethash"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "__elephcindexof"
        ),
        "Phar" | "PharData" => matches!(
            method_key,
            "__construct"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "addfromstring"
                | "__tostring"
                | "getpath"
                | "getpathname"
                | "getfilename"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "setstub"
                | "getstub"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
                | "count"
                | "compressfiles"
                | "decompressfiles"
                | "compress"
                | "decompress"
                | "setsignaturealgorithm"
                | "getsignature"
                | "setzippassword"
                | "delete"
        ),
        "ZipArchive" => matches!(
            method_key,
            "open"
                | "close"
                | "count"
                | "getnameindex"
                | "locatename"
                | "statindex"
                | "statname"
                | "getfromindex"
                | "getfromname"
                | "getstream"
                | "getstreamname"
                | "getstreamindex"
                | "setpassword"
                | "getstatusstring"
                | "extractto"
                | "__elephczipcleanpath"
        ),
        "PharFileInfo" => matches!(
            method_key,
            "__construct"
                | "getcontent"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
        ),
        "IteratorIterator" => matches!(
            method_key,
            "current" | "key" | "next" | "rewind" | "valid" | "getinneriterator"
        ),
        "LimitIterator" => matches!(
            method_key,
            "__construct" | "rewind" | "next" | "valid" | "seek" | "getposition"
        ),
        "NoRewindIterator" => matches!(method_key, "__construct" | "rewind"),
        "InfiniteIterator" => matches!(method_key, "__construct" | "next"),
        "FilterIterator" => matches!(method_key, "__construct" | "rewind" | "next"),
        "CallbackFilterIterator" => matches!(method_key, "accept" | "__elephcsetcallbackenv"),
        "CachingIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "next"
                | "current"
                | "key"
                | "hasnext"
                | "__tostring"
                | "getflags"
                | "setflags"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "offsetexists"
                | "getcache"
                | "count"
                | "__elephccapturecurrent"
        ),
        "AppendIterator" => matches!(
            method_key,
            "__construct"
                | "append"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getinneriterator"
                | "getiteratorindex"
                | "getarrayiterator"
                | "__elephcstoragecount"
                | "__elephcstoragephysicalcount"
                | "__elephcstorageisactive"
                | "__elephcstorageappend"
                | "__elephcstorageoffsetset"
                | "__elephcstorageoffsetexists"
                | "__elephcstorageoffsetget"
                | "__elephcstorageoffsetunset"
                | "__elephcstoragegetarraycopy"
                | "__elephcstoragekey"
                | "__elephcstoragecurrent"
        ),
        "MultipleIterator" => matches!(
            method_key,
            "__construct"
                | "getflags"
                | "setflags"
                | "attachiterator"
                | "detachiterator"
                | "containsiterator"
                | "countiterators"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
        ),
        "RegexIterator" | "RecursiveRegexIterator" => matches!(
            method_key,
            "__construct"
                | "accept"
                | "current"
                | "key"
                | "getmode"
                | "setmode"
                | "getflags"
                | "setflags"
                | "getregex"
                | "getpregflags"
                | "setpregflags"
                | "__elephcregextarget"
                | "__elephcfirstmatch"
                | "__elephcallmatches"
                | "__elephcsplit"
                | "haschildren"
                | "getchildren"
                | "__elephcassumerecursiveiterator"
        ),
        "RecursiveArrayIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveCallbackFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "ParentIterator" => matches!(
            method_key,
            "__construct" | "accept" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveIteratorIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getdepth"
                | "getinneriterator"
                | "getsubiterator"
                | "__elephcadvance"
                | "__elephcslotfordepth"
                | "__elephcassumerecursiveiterator"
        ),
        "__ElephcAppendIteratorArrayIterator" => matches!(
            method_key,
            "__construct"
                | "count"
                | "append"
                | "offsetset"
                | "offsetexists"
                | "offsetget"
                | "offsetunset"
                | "getarraycopy"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
        ),
        _ => false,
    }
}

/// Returns true when this SPL method is implemented by an intrinsic runtime wrapper.
pub(super) fn runtime_intrinsic_method_has_wrapper(
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    let intrinsic = if is_static {
        IntrinsicCall::static_method(class_name, method_key)
    } else {
        IntrinsicCall::instance_method(class_name, method_key)
    };
    intrinsic.is_some_and(|intrinsic| intrinsic.runtime_helper().is_some())
}

