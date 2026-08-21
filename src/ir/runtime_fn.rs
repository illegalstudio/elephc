//! Purpose:
//! Defines stable typed identities for runtime semantic functions reached from EIR.
//!
//! Called from:
//! - Registry semantic descriptors and typed EIR `RuntimeCall` instructions.
//! - Target backend dispatch groups under `codegen/lower_inst/runtime_functions/`.
//!
//! Key details:
//! - IDs describe runtime functions, not PHP names or per-builtin EIR opcodes; aliases can share one ID.
//! - Backend dispatch never infers behavior from a source-level function name.
//! - Physical registers, helper symbols, and platform branches remain downstream in codegen.

/// Backend materialization selected by one runtime function descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFnBackendMapping {
    /// The EIR backend owns a target-aware emitter that may call one or more raw helpers.
    TargetAwareEmitter,
}

/// Supported-target availability declared by a runtime function descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFnTargetSupport {
    /// Implemented for macOS AArch64, Linux AArch64, and Linux x86_64.
    AllSupported,
}

/// A resource destructor `__rt_mixed_free_deep` runs at scope exit, beyond the plain
/// `close()` every kind-1 stream descriptor gets.
///
/// `RuntimeFnId::resource_cleanup_kind` is the SINGLE authority for which of these a
/// program can produce: the lowering stamps the kind from it, and the runtime emitter
/// omits the ladder arm for every kind no lowered call declares. A new producer that
/// does not declare itself here compiles and runs, and silently leaks its handle at
/// scope exit — declare it in `resource_cleanup_kind` before stamping it.
///
/// Kind 0 (generic, no destructor), kind 2 (`HashContext`, stamped by the runtime helper
/// `__rt_hash_init` rather than by a lowering) and kind 5 (the eval-owned inert handle,
/// which must never gain an arm) are deliberately absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceCleanupKind {
    /// Kind 1: a native stream descriptor closed with `close()`.
    StreamFd,
    /// Kind 3: a `popen()` pipe closed and reaped through `__rt_pclose`.
    PopenPipe,
    /// Kind 4: an `opendir()` stream released through `__rt_closedir`.
    Directory,
}

impl ResourceCleanupKind {
    /// Returns the value written into the Mixed high payload word for this kind.
    pub const fn stamp(self) -> u64 {
        match self {
            Self::StreamFd => 1,
            Self::PopenPipe => 3,
            Self::Directory => 4,
        }
    }
}

/// Complete central descriptor for one typed EIR runtime function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeFnDescriptor {
    /// Stable typed identity carried by `RuntimeCall`.
    pub id: RuntimeFnId,
    /// Stable backend-neutral EIR spelling.
    pub eir_name: &'static str,
    /// Logical storage ABI validated before target register materialization.
    pub logical_signature: Option<crate::ir::RuntimeCallSignature>,
    /// Conservative observable effects of the runtime function.
    pub effects: crate::ir::Effects,
    /// Ownership and argument-aliasing contract of the result.
    pub result_ownership: crate::builtins::semantics::BuiltinResultOwnership,
    /// Linker/runtime requirements independent of PHP source names.
    pub requirements: &'static [crate::builtins::semantics::BuiltinRequirement],
    /// Backend implementation mapping for the supported target matrix.
    pub backend_mapping: RuntimeFnBackendMapping,
    /// Explicit target availability.
    pub target_support: RuntimeFnTargetSupport,
}

/// Stable semantic identity for one runtime function callable from EIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeFnId {
    ArrayAll,
    ArrayAny,
    ArrayChunk,
    ArrayColumn,
    ArrayCombine,
    ArrayCountValues,
    ArrayDiff,
    ArrayDiffAssoc,
    ArrayDiffKey,
    ArrayFill,
    ArrayFillKeys,
    ArrayFilter,
    ArrayFind,
    ArrayFlip,
    ArrayIntersect,
    ArrayIntersectAssoc,
    ArrayIntersectKey,
    ArrayIsList,
    ArrayKeyExists,
    ArrayKeyFirst,
    ArrayKeyLast,
    ArrayKeys,
    ArrayMap,
    ArrayMerge,
    ArrayMergeRecursive,
    ArrayMultisort,
    ArrayPad,
    ArrayPop,
    /// Resolves the next internal-array-pointer cursor for `reset`/`end`/`next`/`prev`.
    ArrayPtrSeek,
    /// Boxes the key at an internal-array-pointer cursor for `key()`.
    ArrayPtrKey,
    /// Boxes the value at an internal-array-pointer cursor for `current()` and friends.
    ArrayPtrValue,
    ArrayProduct,
    ArrayPush,
    ArrayRand,
    ArrayReduce,
    ArrayReplace,
    ArrayReplaceRecursive,
    ArrayReverse,
    ArraySearch,
    ArrayShift,
    ArraySlice,
    ArraySplice,
    ArraySum,
    ArrayUdiff,
    ArrayUintersect,
    ArrayUnique,
    ArrayUnshift,
    ArrayValues,
    ArrayWalk,
    ArrayWalkRecursive,
    Arsort,
    Asort,
    Count,
    InArray,
    Krsort,
    Ksort,
    Natcasesort,
    Natsort,
    Range,
    Rsort,
    Shuffle,
    Sort,
    Uasort,
    Uksort,
    Usort,
    CallUserFunc,
    CallUserFuncArray,
    ClassAlias,
    ClassExists,
    ClassImplements,
    ClassParents,
    ClassUses,
    EnumExists,
    FunctionExists,
    GetClass,
    GetObjectVars,
    GetDeclaredClasses,
    GetDeclaredInterfaces,
    GetDeclaredTraits,
    GetLoadedExtensions,
    GetParentClass,
    InterfaceExists,
    IsA,
    IsSubclassOf,
    MethodExists,
    PregReplaceCallback,
    PropertyExists,
    TraitExists,
    ElephcPharBzip2Archive,
    ElephcPharDecompressArchive,
    ElephcPharGetFileMetadata,
    ElephcPharGetMetadata,
    ElephcPharGetSignatureHash,
    ElephcPharGetSignatureType,
    ElephcPharGetStub,
    ElephcPharGzipArchive,
    ElephcPharListEntries,
    ElephcPharSetCompression,
    ElephcPharSetFileMetadata,
    ElephcPharSetMetadata,
    ElephcPharSetStub,
    ElephcPharSetZipPassword,
    ElephcPharSignHash,
    ElephcPharSignOpenssl,
    ElephcZipStatEntries,
    Basename,
    Chdir,
    Chgrp,
    Chmod,
    Chown,
    Clearstatcache,
    Closedir,
    Copy,
    Dirname,
    DiskFreeSpace,
    DiskTotalSpace,
    Fclose,
    Fdatasync,
    Feof,
    Fflush,
    Fgetc,
    Fgetcsv,
    Fgets,
    File,
    FileExists,
    FileGetContents,
    FilePutContents,
    Fileatime,
    Filectime,
    Filegroup,
    Fileinode,
    Filemtime,
    Fileowner,
    Fileperms,
    Filesize,
    Filetype,
    Flock,
    Fnmatch,
    Fopen,
    Fpassthru,
    Fprintf,
    Fputcsv,
    Fread,
    Fseek,
    Fsockopen,
    Fstat,
    Fsync,
    Ftell,
    Ftruncate,
    Fwrite,
    Getcwd,
    Gethostbyaddr,
    Gethostbyname,
    Gethostname,
    Getprotobyname,
    Getprotobynumber,
    Getservbyname,
    Getservbyport,
    Glob,
    HashFile,
    IsDir,
    IsExecutable,
    IsFile,
    IsLink,
    IsReadable,
    IsWritable,
    IsWriteable,
    Lchgrp,
    Lchown,
    Link,
    Linkinfo,
    Lstat,
    Mkdir,
    ObClean,
    ObEndClean,
    ObEndFlush,
    ObFlush,
    ObGetClean,
    ObGetContents,
    ObGetFlush,
    ObGetLength,
    ObGetLevel,
    ObGetStatus,
    ObImplicitFlush,
    ObListHandlers,
    ObStart,
    Opendir,
    Pathinfo,
    Pclose,
    Pfsockopen,
    Popen,
    PrintR,
    Readdir,
    Readfile,
    Readline,
    Readlink,
    Realpath,
    RealpathCacheGet,
    RealpathCacheSize,
    Rename,
    Rewind,
    Rewinddir,
    Rmdir,
    Scandir,
    Stat,
    StreamBucketAppend,
    StreamBucketMakeWriteable,
    StreamBucketNew,
    StreamBucketPrepend,
    StreamContextCreate,
    StreamContextGetDefault,
    StreamContextGetOptions,
    StreamContextGetParams,
    StreamContextSetDefault,
    StreamContextSetOption,
    StreamContextSetOptions,
    StreamContextSetParams,
    StreamCopyToStream,
    StreamFilterAppend,
    StreamFilterPrepend,
    StreamFilterRegister,
    StreamFilterRemove,
    StreamGetContents,
    StreamGetFilters,
    StreamGetLine,
    StreamGetMetaData,
    StreamGetTransports,
    StreamGetWrappers,
    StreamIsLocal,
    StreamIsatty,
    StreamResolveIncludePath,
    StreamSelect,
    StreamSetBlocking,
    StreamSetChunkSize,
    StreamSetReadBuffer,
    StreamSetTimeout,
    StreamSetWriteBuffer,
    StreamSocketAccept,
    StreamSocketClient,
    StreamSocketEnableCrypto,
    StreamSocketGetName,
    StreamSocketPair,
    StreamSocketRecvfrom,
    StreamSocketSendto,
    StreamSocketServer,
    StreamSocketShutdown,
    StreamSupportsLock,
    StreamWrapperRegister,
    StreamWrapperRestore,
    StreamWrapperUnregister,
    Symlink,
    SysGetTempDir,
    Tempnam,
    Tmpfile,
    Touch,
    Umask,
    Unlink,
    VarDump,
    Vfprintf,
    Abs,
    Acos,
    Asin,
    Atan,
    Atan2,
    BaseConvert,
    BcAdd,
    BcCeil,
    BcComp,
    BcDiv,
    BcDivmod,
    BcFloor,
    BcMod,
    BcMul,
    BcPow,
    BcPowmod,
    BcRound,
    BcScale,
    BcSqrt,
    BcSub,
    Ceil,
    Clamp,
    Cos,
    Cosh,
    Bindec,
    Decbin,
    Dechex,
    Decoct,
    Deg2rad,
    Exp,
    Fdiv,
    Floor,
    Fmod,
    Hexdec,
    Hypot,
    Intdiv,
    Log,
    Log10,
    Log2,
    Max,
    Min,
    MtRand,
    Octdec,
    Pi,
    Pow,
    Rad2deg,
    Rand,
    RandomInt,
    Round,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    ElephcObjectIsEnum,
    ElephcObjectPropCount,
    ElephcObjectPropName,
    ElephcObjectPropValue,
    ElephcPtrIsNull,
    ElephcPtrReadString,
    ElephcPtrWriteString,
    BufferFree,
    BufferLen,
    Ptr,
    PtrGet,
    PtrIsNull,
    PtrNull,
    PtrOffset,
    PtrRead16,
    PtrRead32,
    PtrRead8,
    PtrReadString,
    PtrSet,
    PtrSizeof,
    PtrWrite16,
    PtrWrite32,
    PtrWrite8,
    PtrWriteString,
    ZvalFree,
    ZvalPack,
    ZvalType,
    ZvalUnpack,
    IteratorApply,
    IteratorCount,
    IteratorToArray,
    SplAutoload,
    SplAutoloadCall,
    SplAutoloadExtensions,
    SplAutoloadFunctions,
    SplAutoloadRegister,
    SplAutoloadUnregister,
    SplClasses,
    SplObjectHash,
    SplObjectId,
    Base64Decode,
    Chop,
    Chr,
    ChunkSplit,
    CountChars,
    Crc32,
    CtypeAlnum,
    CtypeAlpha,
    CtypeDigit,
    CtypeSpace,
    /// Unboxes an `array|false` builtin ARGUMENT, throwing php's TypeError for the false.
    ///
    /// Inserted by the argument lowering when an `array|false` union (scandir, glob, file…)
    /// flows into an array-taking builtin: the consumer's own lowering then sees a raw array
    /// pointer and stays untouched. Operands: the boxed value, then the message string —
    /// composed at compile time, `{fn}(): Argument #{n} (${param}) must be of type array,
    /// false given` — the throw uses verbatim.
    ExpectArrayArg,
    Explode,
    GraphemeStrrev,
    Gzcompress,
    Gzdeflate,
    Gzinflate,
    Gzuncompress,
    Hash,
    HashAlgos,
    HashCopy,
    HashEquals,
    HashFinal,
    HashHmac,
    HashInit,
    HashUpdate,
    OpensslCipherIvLength,
    OpensslDecrypt,
    OpensslEncrypt,
    OpensslGetCipherMethods,
    Htmlentities,
    Htmlspecialchars,
    Implode,
    InetNtop,
    InetPton,
    Ip2long,
    Lcfirst,
    Long2ip,
    Ltrim,
    MbEregMatch,
    MbStrlen,
    Md5,
    NumberFormat,
    Ord,
    ParseUrl,
    Printf,
    Rtrim,
    Sha1,
    Sprintf,
    StrContains,
    StrGetcsv,
    StrEndsWith,
    StrIreplace,
    StrPad,
    StrRepeat,
    StrReplace,
    StrSplit,
    StrStartsWith,
    StrWordCount,
    Strcasecmp,
    Strcmp,
    Strncasecmp,
    Strncmp,
    Stripos,
    Strpos,
    Strripos,
    Strrpos,
    Strtr,
    Strstr,
    Substr,
    SubstrCount,
    SubstrReplace,
    Trim,
    Ucfirst,
    Ucwords,
    Vprintf,
    Vsprintf,
    Wordwrap,
    ElephcGmmktimeRaw,
    ElephcMktimeRaw,
    ElephcStrtotimeRaw,
    Checkdate,
    ClassAttributeArgs,
    ClassAttributeNames,
    ClassGetAttributes,
    Date,
    DateDefaultTimezoneGet,
    DateDefaultTimezoneSet,
    Define,
    Defined,
    Exec,
    ExtensionLoaded,
    Getdate,
    Getenv,
    Gmdate,
    Gmmktime,
    Header,
    Hrtime,
    HttpClearLastResponseHeaders,
    HttpGetLastResponseHeaders,
    HttpResponseCode,
    JsonDecode,
    JsonEncode,
    JsonLastError,
    JsonLastErrorMsg,
    JsonValidate,
    Localtime,
    Microtime,
    Mktime,
    Passthru,
    PhpUname,
    Phpversion,
    PregMatch,
    PregMatchAll,
    PregReplace,
    PregSplit,
    Putenv,
    Serialize,
    ShellExec,
    Sleep,
    Strtotime,
    System,
    Time,
    Unserialize,
    Usleep,
    GetResourceId,
    GetResourceType,
    Gettype,
    IntvalBase,
    IsCallable,
    IsFinite,
    IsInfinite,
    IsNan,
    IsNumeric,
    Settype,
}

impl RuntimeFnId {
    /// Returns the central logical ABI and backend contract for this runtime function.
    pub fn descriptor(self) -> RuntimeFnDescriptor {
        let logical_signature = self
            .lowering_owned_arity_bounds()
            .or_else(|| crate::builtins::registry::runtime_fn_arity_bounds(self))
            .map(
                |(min_operands, max_operands)| crate::ir::RuntimeCallSignature::Polymorphic {
                    min_operands,
                    max_operands,
                },
            );
        RuntimeFnDescriptor {
            id: self,
            eir_name: self.as_eir(),
            logical_signature,
            effects: self.effects(),
            result_ownership: self.result_ownership(),
            requirements: self.requirements(),
            backend_mapping: RuntimeFnBackendMapping::TargetAwareEmitter,
            target_support: RuntimeFnTargetSupport::AllSupported,
        }
    }

    /// Returns the operand bounds for runtime functions whose arity is owned by lowering
    /// rather than by a PHP builtin's declared parameter list.
    ///
    /// The registry normally supplies these bounds by reading the declared arity of every
    /// builtin that lists the target in its runtime-function inventory. That derivation
    /// cannot describe the internal-array-pointer family: `key`/`current`/`next`/`prev`/
    /// `reset`/`end` all take one PHP argument, but their lowering appends the hidden
    /// cursor (and, for a seek, the seek mode) as extra operands. Declaring the real
    /// runtime arity here keeps EIR validation meaningful instead of switching it off.
    const fn lowering_owned_arity_bounds(self) -> Option<(usize, Option<usize>)> {
        match self {
            RuntimeFnId::ArrayPtrSeek => Some((3, Some(3))),
            RuntimeFnId::ArrayPtrKey | RuntimeFnId::ArrayPtrValue => Some((2, Some(2))),
            // Compiler-internal: no PHP builtin declares it, so the registry cannot. The two
            // operands are the boxed `array|false` value and the TypeError message.
            RuntimeFnId::ExpectArrayArg => Some((2, Some(2))),
            _ => None,
        }
    }

    /// Returns representation-safe EIR result metadata when no checked call-site type survives.
    ///
    /// Most runtime functions use the registry declaration unchanged. Operations whose registry
    /// declaration is deliberately broad refine it here so compiler-injected or synthesized calls
    /// still materialize the container layout required by the backend.
    pub fn fallback_result_type(
        self,
        arg_types: &[crate::types::PhpType],
        declared: &crate::types::PhpType,
    ) -> crate::types::PhpType {
        use crate::types::PhpType;
        match self {
            RuntimeFnId::ArrayKeys | RuntimeFnId::ArraySlice => {
                PhpType::Array(Box::new(PhpType::Mixed))
            }
            // The removed-elements array copies the receiver's payload slots, so its element
            // layout is the receiver's. A type-changing `$replacement` promotes that receiver to
            // `array<mixed>` during lowering, and the checker's pre-promotion `array<int>` no
            // longer describes what the helper produces.
            RuntimeFnId::ArraySplice => match arg_types.first().map(PhpType::codegen_repr) {
                Some(PhpType::Array(element)) => PhpType::Array(element),
                _ => declared.clone(),
            },
            RuntimeFnId::ArrayValues => match arg_types.first().map(PhpType::codegen_repr) {
                Some(PhpType::Array(element)) => PhpType::Array(element),
                Some(PhpType::AssocArray { value, .. }) => PhpType::Array(value),
                Some(other) => other,
                None => declared.clone(),
            },
            // Reversing keeps the container shape, so a synthetic or callable-dispatched
            // `array_reverse()` with no checked call-site type still returns concrete array
            // metadata. Without it the broad declared `mixed` reached the backend, which stored a
            // raw array pointer into a boxed-Mixed slot: `$f = 'array_reverse'; $f([1, 2])` then
            // read the pointer as a Mixed cell and crashed. The `$preserve_keys` hash shape needs
            // a compile-time literal, which a dynamic wrapper cannot provide, so it is dropped
            // from the callable ABI by `refine_runtime_callable_wrapper_sig`.
            RuntimeFnId::ArrayReverse => match arg_types.first().map(PhpType::codegen_repr) {
                Some(element @ (PhpType::Array(_) | PhpType::AssocArray { .. })) => element,
                _ => declared.clone(),
            },
            // A synthetic or callable-dispatched `array_chunk()` cannot pass a literal
            // `$preserve_keys`, so it always produces the renumbered `array<array<T>>` nesting.
            RuntimeFnId::ArrayChunk => match arg_types.first().map(PhpType::codegen_repr) {
                Some(PhpType::Array(element)) => {
                    PhpType::Array(Box::new(PhpType::Array(element)))
                }
                _ => declared.clone(),
            },
            RuntimeFnId::ClassAttributeArgs => PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
            // `Fgetcsv` is deliberately absent: it boxes `array|false`, so its declared `Mixed`
            // IS the representation the lowering builds. Refining it to `array<string>` here
            // made a synthesized call — `SplFileObject::fgetcsv()`, whose prelude body has no
            // checked call-site type — read the boxed Mixed cell as a raw array pointer and
            // hand back its header words as integers.
            // `Scandir`, `File` and `Glob` left this list when their results became boxed
            // `array|false`, the same exit `Fgetcsv` made: the boxed cell IS the representation
            // the lowering builds.
            RuntimeFnId::ClassAttributeNames
            | RuntimeFnId::BcDivmod
            | RuntimeFnId::Explode
            | RuntimeFnId::SplClasses => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::ClassGetAttributes => PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            ))),
            RuntimeFnId::ElephcPharListEntries => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::ElephcZipStatEntries => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::OpensslGetCipherMethods => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::PregSplit => PhpType::Array(Box::new(PhpType::Mixed)),
            // A CSV row is `?string[]`: php answers `[null]` for a wholly empty subject, so the
            // runtime widens every row to boxed Mixed cells. A callable-dispatched
            // `$f = 'str_getcsv'; $f("")` has no checked call-site type and would otherwise read
            // those cells as raw string pointer/length pairs.
            RuntimeFnId::StrGetcsv => PhpType::Array(Box::new(PhpType::Mixed)),
            RuntimeFnId::Range => PhpType::Array(Box::new(PhpType::Int)),
            _ => declared.clone(),
        }
    }

    /// Reports whether a checked call-site result type is a valid EIR layout for these operands.
    ///
    /// `BuiltinResultType::Checked` replays the type the checker recorded for one call site, and the
    /// checker knows more about a value than EIR does in two routine cases: call-site specialization
    /// narrows an untyped parameter (`function top($scores)`) that
    /// `eir_signature_with_php_param_contracts` still lowers under the boxed-`Mixed` ABI contract,
    /// and a builtin whose EIR result was widened to `array<mixed>` keeps its precise checker type
    /// in the variable that receives it. A runtime function that COPIES an argument's element layout
    /// into its result must therefore re-derive that layout from the EIR-visible argument types.
    /// Taking the checker's narrower type would describe an array of raw payload pointers where the
    /// helper really produced boxed `Mixed` cells, and every later element read would misinterpret
    /// them.
    ///
    /// `array_slice()` and `array_splice()` are the only such targets today, because they are the
    /// only copying array helpers with a boxed-`Mixed` lowering; every other runtime function
    /// accepts the checked type unchanged. The accepted shapes mirror
    /// `require_array_slice_result_type` in the backend: the result element layout must equal the
    /// source element layout, or be the `Mixed` widening the lowering emits explicitly. A non-array checked type is the key-preserving hash form, whose
    /// values carry the source array's runtime value_type header rather than a copied static
    /// element layout, and a source the lowering cannot slice at all is left to the backend so it
    /// reports its own diagnostic. Rejecting here makes the caller fall back to
    /// `fallback_result_type`, the representation-safe layout the boxed-`Mixed` lowering builds.
    pub fn checked_result_type_fits_operands(
        self,
        arg_types: &[crate::types::PhpType],
        checked: &crate::types::PhpType,
    ) -> bool {
        use crate::types::PhpType;
        match self {
            RuntimeFnId::ArraySlice | RuntimeFnId::ArraySplice => {
                let PhpType::Array(result_element) = checked.codegen_repr() else {
                    return true;
                };
                let source_element = match arg_types.first().map(PhpType::codegen_repr) {
                    Some(PhpType::Mixed | PhpType::Union(_)) => PhpType::Mixed,
                    Some(PhpType::Array(element)) => element.codegen_repr(),
                    _ => return true,
                };
                let result_element = result_element.codegen_repr();
                result_element == source_element || result_element == PhpType::Mixed
            }
            _ => true,
        }
    }

    /// Refines the first-class callable ABI where the direct PHP signature is broader.
    pub fn refine_first_class_callable_sig(self, sig: &mut crate::types::FunctionSig) {
        use crate::types::PhpType;
        match self {
            RuntimeFnId::PregReplaceCallback => {
                if let Some((_, callback_ty)) = sig.params.get_mut(1) {
                    *callback_ty = PhpType::Callable;
                }
            }
            RuntimeFnId::ZvalPack => {
                if let Some((_, value_ty)) = sig.params.get_mut(0) {
                    *value_ty = PhpType::Mixed;
                }
                sig.return_type = PhpType::Pointer(None);
            }
            RuntimeFnId::ZvalUnpack => {
                if let Some((_, zval_ty)) = sig.params.get_mut(0) {
                    *zval_ty = PhpType::Pointer(None);
                }
                sig.return_type = PhpType::Mixed;
            }
            RuntimeFnId::ZvalType => {
                if let Some((_, zval_ty)) = sig.params.get_mut(0) {
                    *zval_ty = PhpType::Pointer(None);
                }
                sig.return_type = PhpType::Int;
            }
            RuntimeFnId::ZvalFree => {
                if let Some((_, zval_ty)) = sig.params.get_mut(0) {
                    *zval_ty = PhpType::Pointer(None);
                }
                sig.return_type = PhpType::Void;
            }
            RuntimeFnId::BufferLen => {
                if let Some((_, buffer_ty)) = sig.params.get_mut(0) {
                    *buffer_ty = PhpType::Buffer(Box::new(PhpType::Int));
                }
                sig.return_type = PhpType::Int;
            }
            _ => {}
        }
    }

    /// Refines the PHP-ABI wrapper signature required by this runtime implementation.
    pub fn refine_runtime_callable_wrapper_sig(self, sig: &mut crate::types::FunctionSig) {
        use crate::types::PhpType;
        match self {
            RuntimeFnId::Count => truncate_callable_params(sig, 1),
            // `array_reverse()`'s `$preserve_keys` and `array_slice()`'s `$preserve_keys` pick
            // between an indexed array and an integer-keyed hash, so the backend needs them as
            // compile-time literals. A dynamic callable wrapper receives runtime parameters, so
            // the flag is dropped from the wrapper ABI exactly like `count()`'s `$mode`; the
            // wrapper then always produces the renumbered indexed result. `array_slice()`'s
            // return type is pinned to the concrete indexed layout its helpers materialize,
            // because the wrapper has no per-call-site checked type to read.
            RuntimeFnId::ArrayReverse => truncate_callable_params(sig, 1),
            RuntimeFnId::ArrayChunk => truncate_callable_params(sig, 2),
            RuntimeFnId::ArraySlice => {
                truncate_callable_params(sig, 3);
                sig.return_type = PhpType::Array(Box::new(PhpType::Mixed));
            }
            RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct => {
                set_callable_param_type(sig, 0, PhpType::Array(Box::new(PhpType::Int)));
            }
            RuntimeFnId::Clamp => {
                set_callable_param_type(sig, 0, PhpType::Int);
                set_callable_param_type(sig, 1, PhpType::Int);
                set_callable_param_type(sig, 2, PhpType::Int);
                sig.return_type = PhpType::Int;
            }
            RuntimeFnId::Sort
            | RuntimeFnId::Rsort
            | RuntimeFnId::Shuffle
            | RuntimeFnId::Natsort
            | RuntimeFnId::Natcasesort
            | RuntimeFnId::Asort
            | RuntimeFnId::Arsort => {
                set_callable_param_type(sig, 0, PhpType::Array(Box::new(PhpType::Int)));
            }
            _ => {}
        }
    }

    /// Returns the conservative observable effects for this typed backend operation.
    pub const fn effects(self) -> crate::ir::Effects {
        match self {
            RuntimeFnId::BcScale => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_PROCESS.bits()
                    | crate::ir::Effects::WRITES_PROCESS.bits()
                    | crate::ir::Effects::MAY_THROW.bits(),
            ),
            RuntimeFnId::BcComp => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_PROCESS.bits()
                    | crate::ir::Effects::MAY_THROW.bits(),
            ),
            RuntimeFnId::BcAdd
            | RuntimeFnId::BcCeil
            | RuntimeFnId::BcDiv
            | RuntimeFnId::BcDivmod
            | RuntimeFnId::BcFloor
            | RuntimeFnId::BcMod
            | RuntimeFnId::BcMul
            | RuntimeFnId::BcPow
            | RuntimeFnId::BcPowmod
            | RuntimeFnId::BcRound
            | RuntimeFnId::BcSqrt
            | RuntimeFnId::BcSub => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_PROCESS.bits()
                    | crate::ir::Effects::ALLOC_HEAP.bits()
                    | crate::ir::Effects::MAY_THROW.bits(),
            ),
            RuntimeFnId::Abs |
            RuntimeFnId::Acos |
            RuntimeFnId::ArrayCombine |
            RuntimeFnId::ArrayDiffAssoc |
            RuntimeFnId::ArrayFillKeys |
            RuntimeFnId::ArrayIntersectAssoc |
            RuntimeFnId::ArrayIsList |
            RuntimeFnId::ArrayKeyExists |
            RuntimeFnId::ArrayKeyFirst |
            RuntimeFnId::ArrayKeyLast |
            RuntimeFnId::ArrayKeys |
            RuntimeFnId::ArrayMergeRecursive |
            RuntimeFnId::ArrayReplace |
            RuntimeFnId::ArrayReplaceRecursive |
            RuntimeFnId::Asin |
            RuntimeFnId::Atan |
            // `base64_decode()` only reads the subject's bytes and writes its answer into a
            // fresh concat reservation; even `$strict = true` reports a bad character as a
            // plain `false` return rather than a diagnostic, so nothing observable is lost
            // when an unused call is eliminated.
            RuntimeFnId::Base64Decode |
            RuntimeFnId::Atan2 |
            RuntimeFnId::Ceil |
            RuntimeFnId::Chop |
            RuntimeFnId::Chr |
            RuntimeFnId::Cos |
            RuntimeFnId::Cosh |
            RuntimeFnId::Crc32 |
            RuntimeFnId::CtypeAlnum |
            RuntimeFnId::CtypeAlpha |
            RuntimeFnId::CtypeDigit |
            RuntimeFnId::CtypeSpace |
            RuntimeFnId::Bindec |
            RuntimeFnId::Decbin |
            RuntimeFnId::Dechex |
            RuntimeFnId::Decoct |
            RuntimeFnId::Deg2rad |
            RuntimeFnId::Exp |
            RuntimeFnId::Fdiv |
            RuntimeFnId::Floor |
            RuntimeFnId::Fmod |
            RuntimeFnId::GetResourceId |
            RuntimeFnId::GetResourceType |
            RuntimeFnId::Gettype |
            RuntimeFnId::GraphemeStrrev |
            RuntimeFnId::HashAlgos |
            RuntimeFnId::HashEquals |
            RuntimeFnId::Htmlentities |
            RuntimeFnId::Htmlspecialchars |
            RuntimeFnId::Hexdec |
            RuntimeFnId::Hypot |
            RuntimeFnId::Implode |
            RuntimeFnId::InetNtop |
            RuntimeFnId::InetPton |
            RuntimeFnId::Ip2long |
            RuntimeFnId::IsFinite |
            RuntimeFnId::IsInfinite |
            RuntimeFnId::IsNan |
            RuntimeFnId::IsNumeric |
            RuntimeFnId::Lcfirst |
            RuntimeFnId::Log |
            RuntimeFnId::Log10 |
            RuntimeFnId::Log2 |
            RuntimeFnId::Long2ip |
            RuntimeFnId::Ltrim |
            RuntimeFnId::Md5 |
            RuntimeFnId::NumberFormat |
            RuntimeFnId::Octdec |
            RuntimeFnId::Ord |
            RuntimeFnId::Pi |
            RuntimeFnId::Pow |
            RuntimeFnId::Rad2deg |
            RuntimeFnId::Rtrim |
            RuntimeFnId::Sha1 |
            RuntimeFnId::Sin |
            RuntimeFnId::Sinh |
            RuntimeFnId::Sqrt |
            RuntimeFnId::StrContains |
            RuntimeFnId::StrEndsWith |
            RuntimeFnId::StrIreplace |
            RuntimeFnId::StrReplace |
            RuntimeFnId::StrStartsWith |
            RuntimeFnId::Strcasecmp |
            RuntimeFnId::Strcmp |
            RuntimeFnId::Strstr |
            RuntimeFnId::Substr |
            RuntimeFnId::SubstrReplace |
            RuntimeFnId::Tan |
            RuntimeFnId::Tanh |
            RuntimeFnId::Trim |
            RuntimeFnId::Ucfirst |
            RuntimeFnId::Ucwords => crate::ir::Effects::empty(),
            // These raise reference PHP's catchable `ValueError` for out-of-range
            // arguments (`array_chunk()` non-positive length, `clamp()` inverted bounds,
            // `array_fill()` negative count, `array_pad()` oversized length, `explode()`
            // empty separator, `str_pad()` empty pad string or bad pad type,
            // `str_repeat()` negative count, `str_split()` non-positive length,
            // `str_word_count()` unknown format, `count_chars()` unknown mode,
            // `range()` zero/negative/oversized `$step`, `round()` unknown rounding mode,
            // `strncmp()`/`strncasecmp()` negative compare length,
            // `strpos()`/`strrpos()`/`stripos()`/`strripos()` `$offset` outside the haystack,
            // `substr_count()` empty needle or out-of-subject offset/length,
            // `wordwrap()` empty break or zero cutting width, `min()`/`max()` over an
            // empty array, `parse_url()` unknown `$component` identifier), so they must not
            // be treated
            // as removable pure calls: dead-code elimination would drop the diagnostic, and
            // the try-prefix hoist would move the call out of the `try` that must catch it.
            // These accept an `array|false` union argument (scandir, glob, file) through the
            // lowering's unbox-or-throw wrap (`ARRAY_OR_FALSE_ARG_SITES`): a runtime `false`
            // raises php's catchable TypeError at the argument. Claiming purity let DCE drop
            // an unused call — and its throw — and let the try-prefix hoist move the call out
            // of the `try` that must catch it, so the TypeError escaped as uncaught.
            RuntimeFnId::ArrayColumn
            | RuntimeFnId::ArrayDiff
            | RuntimeFnId::ArrayDiffKey
            | RuntimeFnId::ArrayFlip
            | RuntimeFnId::ArrayIntersect
            | RuntimeFnId::ArrayIntersectKey
            | RuntimeFnId::ArrayMerge
            | RuntimeFnId::ArrayProduct
            | RuntimeFnId::ArrayReverse
            | RuntimeFnId::ArraySearch
            | RuntimeFnId::ArraySlice
            | RuntimeFnId::ArraySum
            | RuntimeFnId::ArrayUnique
            | RuntimeFnId::ArrayValues
            // These raise reference PHP's catchable `ValueError` for out-of-range arguments.
            | RuntimeFnId::ArrayChunk
            | RuntimeFnId::ArrayFill
            | RuntimeFnId::CountChars
            | RuntimeFnId::ArrayPad
            | RuntimeFnId::Clamp
            | RuntimeFnId::Explode
            | RuntimeFnId::Max
            | RuntimeFnId::Min
            | RuntimeFnId::Range
            | RuntimeFnId::Round
            | RuntimeFnId::StrPad
            | RuntimeFnId::StrRepeat
            | RuntimeFnId::StrSplit
            | RuntimeFnId::StrWordCount
            | RuntimeFnId::Strncasecmp
            | RuntimeFnId::Strncmp
            | RuntimeFnId::Stripos
            | RuntimeFnId::Strpos
            | RuntimeFnId::Strripos
            | RuntimeFnId::Strrpos
            | RuntimeFnId::SubstrCount
            | RuntimeFnId::BaseConvert
            | RuntimeFnId::ChunkSplit
            | RuntimeFnId::ParseUrl
            | RuntimeFnId::Wordwrap => crate::ir::Effects::MAY_THROW,
            RuntimeFnId::FunctionExists
            | RuntimeFnId::Defined
            | RuntimeFnId::JsonLastError
            | RuntimeFnId::JsonLastErrorMsg
            | RuntimeFnId::DateDefaultTimezoneGet
            | RuntimeFnId::ObGetLevel => crate::ir::Effects::READS_GLOBAL,
            RuntimeFnId::SplAutoloadExtensions => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_GLOBAL.bits()
                    | crate::ir::Effects::WRITES_GLOBAL.bits(),
            ),
            RuntimeFnId::SplAutoloadFunctions => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_GLOBAL.bits()
                    | crate::ir::Effects::ALLOC_HEAP.bits(),
            ),
            RuntimeFnId::GetClass
            | RuntimeFnId::GetParentClass
            | RuntimeFnId::ElephcObjectIsEnum
            | RuntimeFnId::ElephcObjectPropCount
            | RuntimeFnId::ElephcObjectPropName
            | RuntimeFnId::SplObjectId => crate::ir::Effects::READS_HEAP,
            // Re-boxing a property slot allocates the Mixed cell it hands back.
            RuntimeFnId::ElephcObjectPropValue => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_HEAP.bits() | crate::ir::Effects::ALLOC_HEAP.bits(),
            ),
            RuntimeFnId::GetObjectVars => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_HEAP.bits()
                    | crate::ir::Effects::ALLOC_HEAP.bits()
                    | crate::ir::Effects::REFCOUNT_OP.bits()
                    | crate::ir::Effects::MAY_FATAL.bits(),
            ),
            RuntimeFnId::SplObjectHash => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_HEAP.bits()
                    | crate::ir::Effects::ALLOC_CONCAT.bits(),
            ),
            RuntimeFnId::BufferLen => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_HEAP.bits() | crate::ir::Effects::MAY_FATAL.bits(),
            ),
            RuntimeFnId::Time => crate::ir::Effects::READS_PROCESS,
            RuntimeFnId::Microtime | RuntimeFnId::Hrtime => {
                crate::ir::Effects::from_bits_retain(
                    crate::ir::Effects::READS_PROCESS.bits()
                        | crate::ir::Effects::ALLOC_HEAP.bits(),
                )
            }
            RuntimeFnId::Getenv | RuntimeFnId::Gethostname => {
                crate::ir::Effects::from_bits_retain(
                    crate::ir::Effects::READS_PROCESS.bits()
                        | crate::ir::Effects::ALLOC_CONCAT.bits(),
                )
            }
            RuntimeFnId::PhpUname => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_PROCESS.bits()
                    | crate::ir::Effects::ALLOC_CONCAT.bits()
                    | crate::ir::Effects::MAY_FATAL.bits(),
            ),
            RuntimeFnId::Phpversion => crate::ir::Effects::PURE,
            RuntimeFnId::Rand => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_PROCESS.bits()
                    | crate::ir::Effects::WRITES_PROCESS.bits(),
            ),
            // `mt_rand()` and `random_int()` raise a catchable `ValueError` for an inverted
            // `[min, max]` range; `rand()` silently swaps the bounds instead.
            RuntimeFnId::MtRand | RuntimeFnId::RandomInt => {
                crate::ir::Effects::from_bits_retain(
                    crate::ir::Effects::READS_PROCESS.bits()
                        | crate::ir::Effects::WRITES_PROCESS.bits()
                        | crate::ir::Effects::MAY_THROW.bits(),
                )
            }
            RuntimeFnId::Sleep | RuntimeFnId::Usleep => crate::ir::Effects::WRITES_PROCESS,
            // `intval($value, $base)` only inspects the subject's bytes: the string parser
            // allocates nothing, and the boxed-`Mixed` entry point reads the cell before
            // handing a non-string payload to the ordinary integer cast.
            RuntimeFnId::IntvalBase => crate::ir::Effects::READS_HEAP,
            // `strtr()` reads the replacement-pair hash and materializes its result through
            // the shared concat reservation front end; it never throws or warns.
            RuntimeFnId::Strtr => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::READS_HEAP.bits()
                    | crate::ir::Effects::ALLOC_CONCAT.bits(),
            ),
            RuntimeFnId::Sprintf | RuntimeFnId::Vsprintf => {
                crate::ir::Effects::from_bits_retain(
                    crate::ir::Effects::READS_HEAP.bits()
                        | crate::ir::Effects::ALLOC_CONCAT.bits()
                        | crate::ir::Effects::MAY_WARN.bits(),
                )
            }
            _ => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::all().bits()
                    & !crate::ir::Effects::REFCOUNT_OP.bits()
                    & !crate::ir::Effects::WRITES_GLOBAL.bits(),
            ),
        }
    }

    /// Returns effects intrinsic to a callback builtin before invoking user code.
    ///
    /// Optimizer effect analysis combines this base with a statically-known closure or
    /// first-class-callable summary. Dynamic callbacks still use [`Self::effects`].
    pub const fn intrinsic_effects(self) -> crate::ir::Effects {
        use crate::ir::Effects as E;
        match self {
            RuntimeFnId::ArrayAll
            | RuntimeFnId::ArrayAny
            | RuntimeFnId::ArrayFilter
            | RuntimeFnId::ArrayFind
            | RuntimeFnId::ArrayMap
            | RuntimeFnId::ArrayReduce
            | RuntimeFnId::ArrayWalk
            | RuntimeFnId::ArrayWalkRecursive
            | RuntimeFnId::ArrayUdiff
            | RuntimeFnId::ArrayUintersect => {
                E::from_bits_retain(E::READS_HEAP.bits() | E::ALLOC_HEAP.bits())
            }
            RuntimeFnId::PregReplaceCallback => E::from_bits_retain(
                E::READS_HEAP.bits() | E::ALLOC_HEAP.bits() | E::MAY_WARN.bits(),
            ),
            RuntimeFnId::Uasort
            | RuntimeFnId::Uksort
            | RuntimeFnId::Usort => {
                E::from_bits_retain(
                    E::READS_HEAP.bits() | E::WRITES_HEAP.bits() | E::REFCOUNT_OP.bits(),
                )
            }
            RuntimeFnId::CallUserFunc => E::PURE,
            RuntimeFnId::CallUserFuncArray => E::READS_HEAP,
            _ => self.effects(),
        }
    }

    /// Returns runtime and linker requirements declared by this typed operation.
    pub const fn requirements(
        self,
    ) -> &'static [crate::builtins::semantics::BuiltinRequirement] {
        use crate::builtins::semantics::BuiltinRequirement;
        match self {
            RuntimeFnId::BcAdd
            | RuntimeFnId::BcCeil
            | RuntimeFnId::BcComp
            | RuntimeFnId::BcDiv
            | RuntimeFnId::BcDivmod
            | RuntimeFnId::BcFloor
            | RuntimeFnId::BcMod
            | RuntimeFnId::BcMul
            | RuntimeFnId::BcPow
            | RuntimeFnId::BcPowmod
            | RuntimeFnId::BcRound
            | RuntimeFnId::BcScale
            | RuntimeFnId::BcSqrt
            | RuntimeFnId::BcSub => &[BuiltinRequirement::Bridge("elephc_bcmath")],
            RuntimeFnId::ElephcPharBzip2Archive => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharDecompressArchive => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGetFileMetadata => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGetMetadata => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGetSignatureHash => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGetSignatureType => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGetStub => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharGzipArchive => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharListEntries => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSetCompression => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSetFileMetadata => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSetMetadata => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSetStub => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSetZipPassword => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSignHash => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcZipStatEntries => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::ElephcPharSignOpenssl => &[BuiltinRequirement::Bridge("elephc_phar")],
            RuntimeFnId::Gzcompress => &[BuiltinRequirement::SystemLibrary("z")],
            RuntimeFnId::Gzdeflate => &[BuiltinRequirement::SystemLibrary("z")],
            RuntimeFnId::Gzinflate => &[BuiltinRequirement::SystemLibrary("z")],
            RuntimeFnId::Gzuncompress => &[BuiltinRequirement::SystemLibrary("z")],
            RuntimeFnId::Hash => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashCopy => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashFile => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashFinal => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashHmac => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashInit => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::HashUpdate => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::OpensslCipherIvLength
            | RuntimeFnId::OpensslDecrypt
            | RuntimeFnId::OpensslEncrypt
            | RuntimeFnId::OpensslGetCipherMethods => {
                &[BuiltinRequirement::Bridge("elephc_crypto")]
            }
            RuntimeFnId::MbStrlen => &[BuiltinRequirement::MacOsLibrary("iconv")],
            RuntimeFnId::Md5 => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::Sha1 => &[BuiltinRequirement::Bridge("elephc_crypto")],
            RuntimeFnId::StreamSocketEnableCrypto => &[BuiltinRequirement::Bridge("elephc_tls")],
            _ => &[],
        }
    }

    /// Returns whether the operation has a proven generic runtime-callable wrapper.
    pub const fn runtime_callable_supported(self) -> bool {
        matches!(
            self,
            RuntimeFnId::Abs
                | RuntimeFnId::Gettype
                | RuntimeFnId::Trim
        )
    }

    /// Returns whether a dynamic source value can use this target's generic wrapper.
    pub fn callable_accepts(self, source: Option<&crate::types::PhpType>) -> bool {
        use crate::types::PhpType;
        let source = source.map(PhpType::codegen_repr);
        match self {
            RuntimeFnId::Abs => source.is_none_or(|ty| {
                matches!(
                    ty,
                    PhpType::Bool
                        | PhpType::Float
                        | PhpType::Int
                        | PhpType::Mixed
                        | PhpType::Never
                        | PhpType::TaggedScalar
                        | PhpType::Union(_)
                        | PhpType::Void
                )
            }),
            RuntimeFnId::Gettype => true,
            RuntimeFnId::Trim => source.is_none_or(|ty| matches!(ty, PhpType::Str)),
            _ => false,
        }
    }

    /// Returns whether this operation requires the optional regex runtime family.
    pub const fn uses_regex_runtime(self) -> bool {
        matches!(
            self,
            RuntimeFnId::PregMatch
                | RuntimeFnId::PregMatchAll
                | RuntimeFnId::PregReplace
                | RuntimeFnId::PregReplaceCallback
                | RuntimeFnId::PregSplit
        )
    }

    /// Returns whether this operation requires the optional multibyte-length runtime.
    pub const fn uses_mb_strlen_runtime(self) -> bool {
        matches!(self, RuntimeFnId::MbStrlen)
    }

    /// Returns the scope-cleanup kind stamped into the resource this operation boxes.
    ///
    /// Read twice, and that is the point: the lowering stamps `Some(kind).stamp()` into the
    /// Mixed high payload word, and `lowered_runtime_features` turns the same answer into the
    /// runtime feature bit that decides whether `__rt_mixed_free_deep` emits the matching arm.
    /// One table, so the producer and the destructor cannot drift apart.
    ///
    /// Only kinds with a destructor appear. Every other resource-boxing builtin — `fopen`,
    /// `tmpfile`, `fsockopen`, the socket family — carries kind 1, whose `close()` is a raw
    /// syscall on AArch64 and needs nothing gated.
    pub const fn resource_cleanup_kind(self) -> Option<ResourceCleanupKind> {
        match self {
            RuntimeFnId::Popen => Some(ResourceCleanupKind::PopenPipe),
            RuntimeFnId::Opendir => Some(ResourceCleanupKind::Directory),
            _ => None,
        }
    }

    /// Returns whether this operation can publish PHAR bridge helper symbols.
    pub const fn publishes_phar_symbols(self) -> bool {
        matches!(
            self,
            RuntimeFnId::ElephcPharListEntries
                | RuntimeFnId::ElephcZipStatEntries
                | RuntimeFnId::ElephcPharGetMetadata
                | RuntimeFnId::ElephcPharGetStub
                | RuntimeFnId::ElephcPharSetMetadata
                | RuntimeFnId::ElephcPharSetStub
                | RuntimeFnId::ElephcPharGetFileMetadata
                | RuntimeFnId::ElephcPharSetFileMetadata
                | RuntimeFnId::ElephcPharGzipArchive
                | RuntimeFnId::ElephcPharBzip2Archive
                | RuntimeFnId::ElephcPharDecompressArchive
                | RuntimeFnId::ElephcPharSignOpenssl
                | RuntimeFnId::ElephcPharSignHash
                | RuntimeFnId::ElephcPharSetZipPassword
                | RuntimeFnId::ElephcPharGetSignatureHash
                | RuntimeFnId::ElephcPharGetSignatureType
                | RuntimeFnId::FileGetContents
                | RuntimeFnId::FilePutContents
                | RuntimeFnId::Fopen
        )
    }

    /// Returns the callback operand inspected for runtime string dispatch, if any.
    pub const fn string_callback_operand_index(self) -> Option<usize> {
        match self {
            RuntimeFnId::ArrayMap
            | RuntimeFnId::CallUserFunc
            | RuntimeFnId::CallUserFuncArray => Some(0),
            RuntimeFnId::ArrayFilter
            | RuntimeFnId::ArrayReduce
            | RuntimeFnId::ArrayWalk
            | RuntimeFnId::ArrayWalkRecursive
            | RuntimeFnId::Usort
            | RuntimeFnId::Uksort
            | RuntimeFnId::Uasort
            | RuntimeFnId::IteratorApply
            | RuntimeFnId::PregReplaceCallback
            | RuntimeFnId::ArrayFind
            | RuntimeFnId::ArrayAny
            | RuntimeFnId::ArrayAll => Some(1),
            RuntimeFnId::ArrayUdiff | RuntimeFnId::ArrayUintersect => Some(2),
            _ => None,
        }
    }

    /// Returns whether this operation performs dynamic callable lookup.
    pub const fn is_callable_lookup(self) -> bool {
        matches!(self, RuntimeFnId::IsCallable)
    }

    /// Returns whether this operation reads a class name from object metadata.
    pub const fn is_class_name_lookup(self) -> bool {
        matches!(self, RuntimeFnId::GetClass | RuntimeFnId::GetParentClass)
    }

    /// Returns whether this operation registers a stream wrapper or filter class.
    pub const fn is_stream_registration(self) -> bool {
        matches!(
            self,
            RuntimeFnId::StreamWrapperRegister
                | RuntimeFnId::StreamFilterRegister
        )
    }

    /// Returns the ownership and argument-aliasing contract for this operation.
    pub const fn result_ownership(
        self,
    ) -> crate::builtins::semantics::BuiltinResultOwnership {
        use crate::builtins::semantics::BuiltinResultOwnership;
        // `intval($value, $base)` hands back a raw machine integer, never storage. Leaving it
        // in the default `MayAliasArguments` bucket would keep an owned subject temporary
        // alive for the integer's whole lifetime, which is the leak shape already documented
        // for `Strpos` and `Strtr` below.
        if matches!(
            self,
            RuntimeFnId::IntvalBase | RuntimeFnId::BcComp | RuntimeFnId::BcScale
        ) {
            return BuiltinResultOwnership::NonHeap;
        }
        if matches!(
            self,
            RuntimeFnId::Abs
                | RuntimeFnId::BcAdd
                | RuntimeFnId::BcCeil
                | RuntimeFnId::BcDiv
                | RuntimeFnId::BcDivmod
                | RuntimeFnId::BcFloor
                | RuntimeFnId::BcMod
                | RuntimeFnId::BcMul
                | RuntimeFnId::BcPow
                | RuntimeFnId::BcPowmod
                | RuntimeFnId::BcRound
                | RuntimeFnId::BcSqrt
                | RuntimeFnId::BcSub
                | RuntimeFnId::ArrayChunk
                | RuntimeFnId::ArrayColumn
                | RuntimeFnId::ArrayCombine
                | RuntimeFnId::ArrayDiff
                | RuntimeFnId::ArrayFill
                | RuntimeFnId::ArrayFillKeys
                // Every `array_flip` lowering allocates its destination table before writing a
                // single entry — `__rt_hash_flip` calls `__rt_hash_new`, the indexed helpers
                // call `__rt_array_new` — so the result can never alias the source. The default
                // `MayAliasArguments` bucket suppressed the release of an owned source
                // temporary, which leaked the whole source table on `array_flip(build())` while
                // the same call through a named local stayed clean. Its Fresh-owning siblings
                // `ArrayKeys` / `ArrayValues` were already listed here; this was the gap.
                | RuntimeFnId::ArrayCountValues
                | RuntimeFnId::ArrayFlip
                | RuntimeFnId::ArrayIntersect
                | RuntimeFnId::ArrayKeys
                | RuntimeFnId::ArrayMap
                | RuntimeFnId::ArrayMerge
                | RuntimeFnId::ArrayPad
                | RuntimeFnId::ArrayPop
                // `key()`/`current()` and the seek family all hand back a cell built by
                // `__rt_mixed_from_value`, which persists strings and increfs containers, so
                // the box is independently owned and never aliases the receiving array.
                | RuntimeFnId::ArrayPtrKey
                | RuntimeFnId::ArrayPtrValue
                | RuntimeFnId::ArrayReplace
                | RuntimeFnId::ArrayReplaceRecursive
                | RuntimeFnId::ArrayReverse
                | RuntimeFnId::ArrayShift
                | RuntimeFnId::ArraySlice
                | RuntimeFnId::ArrayUnique
                | RuntimeFnId::ArrayValues
                // `base64_decode()`'s result is `string|false`, so its lowering boxes BOTH
                // arms into a fresh Mixed cell and `__rt_mixed_from_value` persists (copies)
                // the decoded payload. Nothing handed back points into the encoded subject,
                // so the default `MayAliasArguments` bucket would only keep an owned subject
                // temporary alive for the boxed result's whole lifetime.
                | RuntimeFnId::Base64Decode
                // hexdec()/bindec()/octdec() box their `int|float` answer through
                // `__rt_mixed_from_value`, so the cell handed back is a fresh allocation
                // that cannot alias the parsed subject string.
                // Every `count_chars()` shape allocates its own result: modes 0-2 build a
                // brand-new tally hash and modes 3-4 hand back a `__rt_str_persist`-owned
                // byte list, so nothing returned can alias the subject string.
                | RuntimeFnId::CountChars
                | RuntimeFnId::Bindec
                | RuntimeFnId::Hexdec
                | RuntimeFnId::Octdec
                // Every property slot is re-boxed through `__rt_mixed_from_value`,
                // which persists strings and increfs containers, so the cell handed
                // back is independently owned and never aliases the source object's
                // storage — the caller may release it like any other temporary.
                | RuntimeFnId::ElephcObjectPropValue
                | RuntimeFnId::Explode
                | RuntimeFnId::Fgetcsv
                | RuntimeFnId::FileGetContents
                // `getcwd()` takes NO arguments, so its result cannot alias one by
                // construction; `__rt_getcwd` copies the kernel's buffer out through
                // `__rt_str_persist`. The default `MayAliasArguments` bucket made
                // `value_is_scratch_string` treat that owned block as concat scratch and skip
                // its release, leaking one block per call — measured unbounded, 10 calls left
                // 10 live blocks, so a `--web` worker calling it per request grows forever.
                | RuntimeFnId::Getcwd
                | RuntimeFnId::GetObjectVars
                | RuntimeFnId::IteratorToArray
                // `json_encode()` builds its text in fresh storage and persists it; the result
                // is new bytes, never a slice of the encoded value. Same leak shape as the
                // three siblings already documented below.
                | RuntimeFnId::JsonEncode
                // `microtime()` formats into fresh storage from the clock; it has no string
                // argument to alias. Its float mode is non-heap and unaffected.
                | RuntimeFnId::Microtime
                // Every `min()` / `max()` return path materializes fresh storage rather
                // than handing back argument storage: the scalar forms return a plain
                // register value, a `Mixed` result is boxed through
                // `__rt_mixed_from_value` (which persists strings and increfs heap
                // children), and the single-array string reduction runs its borrowed
                // winner through `__rt_str_persist`. Leaving them in the default
                // `MayAliasArguments` bucket suppressed nothing useful and leaked the
                // boxed `Mixed` result of `min([...])`.
                | RuntimeFnId::Max
                | RuntimeFnId::Min
                | RuntimeFnId::ObGetClean
                | RuntimeFnId::ObGetContents
                | RuntimeFnId::ObGetFlush
                | RuntimeFnId::ObGetLength
                | RuntimeFnId::ObGetStatus
                | RuntimeFnId::ObListHandlers
                | RuntimeFnId::OpensslCipherIvLength
                | RuntimeFnId::OpensslDecrypt
                | RuntimeFnId::OpensslEncrypt
                | RuntimeFnId::OpensslGetCipherMethods
                | RuntimeFnId::ParseUrl
                | RuntimeFnId::PregSplit
                // print_r renders into the `_print_r_buf` capture buffer and `__rt_pr_finish`
                // copies those bytes out through `__rt_str_persist`, so every mode returns
                // storage that is freshly allocated and cannot alias an argument: return mode
                // yields an owned heap string, echo mode a non-heap `true`, and the runtime-flag
                // mode a fresh Mixed cell. The default `MayAliasArguments` bucket made
                // `value_is_scratch_string` classify the return-mode string as concat scratch and
                // skip its release, leaking one block per `print_r($v, true)` call.
                | RuntimeFnId::PrintR
                | RuntimeFnId::PtrReadString
                | RuntimeFnId::Range
                | RuntimeFnId::StrSplit
                // Every `str_word_count()` shape allocates its own result: format 0 is a plain
                // integer, format 1 pushes persisted copies into a brand-new indexed array, and
                // format 2 persists each word before inserting it into a brand-new hash. Nothing
                // handed back can alias the subject or the character-list argument.
                | RuntimeFnId::StrWordCount
                | RuntimeFnId::Stripos
                | RuntimeFnId::Strpos
                | RuntimeFnId::Strripos
                | RuntimeFnId::Strrpos
                // `strtr()` writes into a reservation taken from `__rt_concat_reserve` and then
                // copies the finished bytes into owned heap storage through `__rt_str_persist`,
                // releasing the reservation afterwards, so the result is caller-owned and can
                // never alias the subject, the pair array, or the byte lists.
                | RuntimeFnId::Strtr
                // Strstr's result is `string|false`, so its lowering boxes BOTH arms into a
                // fresh Mixed cell and `__rt_mixed_from_value` persists (copies) the string
                // payload — it no longer hands back a borrowed slice of the haystack. Leaving
                // it in the default `MayAliasArguments` bucket kept an owned haystack
                // temporary alive for the boxed result's whole lifetime, which leaked one
                // block per iteration for `strstr($h, $cond ? "a" : "b")` in a loop.
                | RuntimeFnId::Strstr
                // `tempnam(directory, prefix)` returns the generated path that `mkstemp()`
                // wrote into a buffer `__rt_tempnam` allocated itself, then copied out with
                // `__rt_str_persist` — it is neither of its two argument strings. This was the
                // declared debt; measuring it showed the leak is PER CALL, not the reported
                // constant 48 bytes, and that `sys_get_temp_dir()` and `tmpfile()` — named
                // alongside it in that report — are both clean.
                | RuntimeFnId::Tempnam
                // `array_splice()` always answers with the array `__rt_array_new` allocated for
                // the removed window; the receiver is mutated through its by-reference slot and
                // is never handed back. The default `MayAliasArguments` bucket suppressed the
                // release of an owned `$replacement` argument, so `array_splice($a, 1, 2, [9])`
                // leaked the literal replacement array on every call.
                | RuntimeFnId::ArraySplice
                | RuntimeFnId::ZvalUnpack
        ) {
            BuiltinResultOwnership::Fresh
        } else if matches!(
            self,
            RuntimeFnId::BaseConvert
                // `__rt_chunk_split` always writes into a reservation taken from
                // `__rt_concat_reserve`, so the split result can never alias the subject or
                // the separator. The default `MayAliasArguments` bucket kept an owned subject
                // temporary alive for the result's whole lifetime, leaking one block per
                // `chunk_split(build())` call.
                | RuntimeFnId::ChunkSplit
                | RuntimeFnId::Decbin
                | RuntimeFnId::Dechex
                | RuntimeFnId::Decoct
                | RuntimeFnId::Htmlentities
                | RuntimeFnId::Htmlspecialchars
                | RuntimeFnId::Implode
        ) {
            BuiltinResultOwnership::Independent
        } else {
            BuiltinResultOwnership::MayAliasArguments
        }
    }

    /// Returns the stable textual EIR spelling for diagnostics and snapshots.
    pub fn as_eir(self) -> &'static str {
        match self {
            RuntimeFnId::ExpectArrayArg => "expect_array_arg",
            RuntimeFnId::ArrayAll => "array_all",
            RuntimeFnId::ArrayAny => "array_any",
            RuntimeFnId::ArrayChunk => "array_chunk",
            RuntimeFnId::ArrayColumn => "array_column",
            RuntimeFnId::ArrayCombine => "array_combine",
            RuntimeFnId::ArrayDiff => "array_diff",
            RuntimeFnId::ArrayDiffAssoc => "array_diff_assoc",
            RuntimeFnId::ArrayDiffKey => "array_diff_key",
            RuntimeFnId::ArrayFill => "array_fill",
            RuntimeFnId::ArrayFillKeys => "array_fill_keys",
            RuntimeFnId::ArrayFilter => "array_filter",
            RuntimeFnId::ArrayFind => "array_find",
            RuntimeFnId::ArrayCountValues => "array_count_values",
            RuntimeFnId::ArrayFlip => "array_flip",
            RuntimeFnId::ArrayIntersect => "array_intersect",
            RuntimeFnId::ArrayIntersectAssoc => "array_intersect_assoc",
            RuntimeFnId::ArrayIntersectKey => "array_intersect_key",
            RuntimeFnId::ArrayIsList => "array_is_list",
            RuntimeFnId::ArrayKeyExists => "array_key_exists",
            RuntimeFnId::ArrayKeyFirst => "array_key_first",
            RuntimeFnId::ArrayKeyLast => "array_key_last",
            RuntimeFnId::ArrayKeys => "array_keys",
            RuntimeFnId::ArrayMap => "array_map",
            RuntimeFnId::ArrayMerge => "array_merge",
            RuntimeFnId::ArrayMergeRecursive => "array_merge_recursive",
            RuntimeFnId::ArrayMultisort => "array_multisort",
            RuntimeFnId::ArrayPad => "array_pad",
            RuntimeFnId::ArrayPop => "array_pop",
            RuntimeFnId::ArrayPtrSeek => "array_ptr_seek",
            RuntimeFnId::ArrayPtrKey => "array_ptr_key",
            RuntimeFnId::ArrayPtrValue => "array_ptr_value",
            RuntimeFnId::ArrayProduct => "array_product",
            RuntimeFnId::ArrayPush => "array_push",
            RuntimeFnId::ArrayRand => "array_rand",
            RuntimeFnId::ArrayReduce => "array_reduce",
            RuntimeFnId::ArrayReplace => "array_replace",
            RuntimeFnId::ArrayReplaceRecursive => "array_replace_recursive",
            RuntimeFnId::ArrayReverse => "array_reverse",
            RuntimeFnId::ArraySearch => "array_search",
            RuntimeFnId::ArrayShift => "array_shift",
            RuntimeFnId::ArraySlice => "array_slice",
            RuntimeFnId::ArraySplice => "array_splice",
            RuntimeFnId::ArraySum => "array_sum",
            RuntimeFnId::ArrayUdiff => "array_udiff",
            RuntimeFnId::ArrayUintersect => "array_uintersect",
            RuntimeFnId::ArrayUnique => "array_unique",
            RuntimeFnId::ArrayUnshift => "array_unshift",
            RuntimeFnId::ArrayValues => "array_values",
            RuntimeFnId::ArrayWalk => "array_walk",
            RuntimeFnId::ArrayWalkRecursive => "array_walk_recursive",
            RuntimeFnId::Arsort => "arsort",
            RuntimeFnId::Asort => "asort",
            RuntimeFnId::Count => "count",
            RuntimeFnId::InArray => "in_array",
            RuntimeFnId::Krsort => "krsort",
            RuntimeFnId::Ksort => "ksort",
            RuntimeFnId::Natcasesort => "natcasesort",
            RuntimeFnId::Natsort => "natsort",
            RuntimeFnId::Range => "range",
            RuntimeFnId::Rsort => "rsort",
            RuntimeFnId::Shuffle => "shuffle",
            RuntimeFnId::Sort => "sort",
            RuntimeFnId::Uasort => "uasort",
            RuntimeFnId::Uksort => "uksort",
            RuntimeFnId::Usort => "usort",
            RuntimeFnId::CallUserFunc => "call_user_func",
            RuntimeFnId::CallUserFuncArray => "call_user_func_array",
            RuntimeFnId::ClassAlias => "class_alias",
            RuntimeFnId::ClassExists => "class_exists",
            RuntimeFnId::ClassImplements => "class_implements",
            RuntimeFnId::ClassParents => "class_parents",
            RuntimeFnId::ClassUses => "class_uses",
            RuntimeFnId::EnumExists => "enum_exists",
            RuntimeFnId::FunctionExists => "function_exists",
            RuntimeFnId::GetClass => "get_class",
            RuntimeFnId::GetObjectVars => "get_object_vars",
            RuntimeFnId::GetDeclaredClasses => "get_declared_classes",
            RuntimeFnId::GetDeclaredInterfaces => "get_declared_interfaces",
            RuntimeFnId::GetDeclaredTraits => "get_declared_traits",
            RuntimeFnId::GetLoadedExtensions => "get_loaded_extensions",
            RuntimeFnId::GetParentClass => "get_parent_class",
            RuntimeFnId::InterfaceExists => "interface_exists",
            RuntimeFnId::IsA => "is_a",
            RuntimeFnId::IsSubclassOf => "is_subclass_of",
            RuntimeFnId::MethodExists => "method_exists",
            RuntimeFnId::PregReplaceCallback => "preg_replace_callback",
            RuntimeFnId::PropertyExists => "property_exists",
            RuntimeFnId::TraitExists => "trait_exists",
            RuntimeFnId::ElephcPharBzip2Archive => "__elephc_phar_bzip2_archive",
            RuntimeFnId::ElephcPharDecompressArchive => "__elephc_phar_decompress_archive",
            RuntimeFnId::ElephcPharGetFileMetadata => "__elephc_phar_get_file_metadata",
            RuntimeFnId::ElephcPharGetMetadata => "__elephc_phar_get_metadata",
            RuntimeFnId::ElephcPharGetSignatureHash => "__elephc_phar_get_signature_hash",
            RuntimeFnId::ElephcPharGetSignatureType => "__elephc_phar_get_signature_type",
            RuntimeFnId::ElephcPharGetStub => "__elephc_phar_get_stub",
            RuntimeFnId::ElephcPharGzipArchive => "__elephc_phar_gzip_archive",
            RuntimeFnId::ElephcPharListEntries => "__elephc_phar_list_entries",
            RuntimeFnId::ElephcZipStatEntries => "__elephc_zip_stat_entries",
            RuntimeFnId::ElephcPharSetCompression => "__elephc_phar_set_compression",
            RuntimeFnId::ElephcPharSetFileMetadata => "__elephc_phar_set_file_metadata",
            RuntimeFnId::ElephcPharSetMetadata => "__elephc_phar_set_metadata",
            RuntimeFnId::ElephcPharSetStub => "__elephc_phar_set_stub",
            RuntimeFnId::ElephcPharSetZipPassword => "__elephc_phar_set_zip_password",
            RuntimeFnId::ElephcPharSignHash => "__elephc_phar_sign_hash",
            RuntimeFnId::ElephcPharSignOpenssl => "__elephc_phar_sign_openssl",
            RuntimeFnId::Basename => "basename",
            RuntimeFnId::Chdir => "chdir",
            RuntimeFnId::Chgrp => "chgrp",
            RuntimeFnId::Chmod => "chmod",
            RuntimeFnId::Chown => "chown",
            RuntimeFnId::Clearstatcache => "clearstatcache",
            RuntimeFnId::Closedir => "closedir",
            RuntimeFnId::Copy => "copy",
            RuntimeFnId::Dirname => "dirname",
            RuntimeFnId::DiskFreeSpace => "disk_free_space",
            RuntimeFnId::DiskTotalSpace => "disk_total_space",
            RuntimeFnId::Fclose => "fclose",
            RuntimeFnId::Fdatasync => "fdatasync",
            RuntimeFnId::Feof => "feof",
            RuntimeFnId::Fflush => "fflush",
            RuntimeFnId::Fgetc => "fgetc",
            RuntimeFnId::Fgetcsv => "fgetcsv",
            RuntimeFnId::Fgets => "fgets",
            RuntimeFnId::File => "file",
            RuntimeFnId::FileExists => "file_exists",
            RuntimeFnId::FileGetContents => "file_get_contents",
            RuntimeFnId::FilePutContents => "file_put_contents",
            RuntimeFnId::Fileatime => "fileatime",
            RuntimeFnId::Filectime => "filectime",
            RuntimeFnId::Filegroup => "filegroup",
            RuntimeFnId::Fileinode => "fileinode",
            RuntimeFnId::Filemtime => "filemtime",
            RuntimeFnId::Fileowner => "fileowner",
            RuntimeFnId::Fileperms => "fileperms",
            RuntimeFnId::Filesize => "filesize",
            RuntimeFnId::Filetype => "filetype",
            RuntimeFnId::Flock => "flock",
            RuntimeFnId::Fnmatch => "fnmatch",
            RuntimeFnId::Fopen => "fopen",
            RuntimeFnId::Fpassthru => "fpassthru",
            RuntimeFnId::Fprintf => "fprintf",
            RuntimeFnId::Fputcsv => "fputcsv",
            RuntimeFnId::Fread => "fread",
            RuntimeFnId::Fseek => "fseek",
            RuntimeFnId::Fsockopen => "fsockopen",
            RuntimeFnId::Fstat => "fstat",
            RuntimeFnId::Fsync => "fsync",
            RuntimeFnId::Ftell => "ftell",
            RuntimeFnId::Ftruncate => "ftruncate",
            RuntimeFnId::Fwrite => "fwrite",
            RuntimeFnId::Getcwd => "getcwd",
            RuntimeFnId::Gethostbyaddr => "gethostbyaddr",
            RuntimeFnId::Gethostbyname => "gethostbyname",
            RuntimeFnId::Gethostname => "gethostname",
            RuntimeFnId::Getprotobyname => "getprotobyname",
            RuntimeFnId::Getprotobynumber => "getprotobynumber",
            RuntimeFnId::Getservbyname => "getservbyname",
            RuntimeFnId::Getservbyport => "getservbyport",
            RuntimeFnId::Glob => "glob",
            RuntimeFnId::HashFile => "hash_file",
            RuntimeFnId::IsDir => "is_dir",
            RuntimeFnId::IsExecutable => "is_executable",
            RuntimeFnId::IsFile => "is_file",
            RuntimeFnId::IsLink => "is_link",
            RuntimeFnId::IsReadable => "is_readable",
            RuntimeFnId::IsWritable => "is_writable",
            RuntimeFnId::IsWriteable => "is_writeable",
            RuntimeFnId::Lchgrp => "lchgrp",
            RuntimeFnId::Lchown => "lchown",
            RuntimeFnId::Link => "link",
            RuntimeFnId::Linkinfo => "linkinfo",
            RuntimeFnId::Lstat => "lstat",
            RuntimeFnId::Mkdir => "mkdir",
            RuntimeFnId::ObClean => "ob_clean",
            RuntimeFnId::ObEndClean => "ob_end_clean",
            RuntimeFnId::ObEndFlush => "ob_end_flush",
            RuntimeFnId::ObFlush => "ob_flush",
            RuntimeFnId::ObGetClean => "ob_get_clean",
            RuntimeFnId::ObGetContents => "ob_get_contents",
            RuntimeFnId::ObGetFlush => "ob_get_flush",
            RuntimeFnId::ObGetLength => "ob_get_length",
            RuntimeFnId::ObGetLevel => "ob_get_level",
            RuntimeFnId::ObGetStatus => "ob_get_status",
            RuntimeFnId::ObImplicitFlush => "ob_implicit_flush",
            RuntimeFnId::ObListHandlers => "ob_list_handlers",
            RuntimeFnId::ObStart => "ob_start",
            RuntimeFnId::Opendir => "opendir",
            RuntimeFnId::Pathinfo => "pathinfo",
            RuntimeFnId::Pclose => "pclose",
            RuntimeFnId::Pfsockopen => "pfsockopen",
            RuntimeFnId::Popen => "popen",
            RuntimeFnId::PrintR => "print_r",
            RuntimeFnId::Readdir => "readdir",
            RuntimeFnId::Readfile => "readfile",
            RuntimeFnId::Readline => "readline",
            RuntimeFnId::Readlink => "readlink",
            RuntimeFnId::Realpath => "realpath",
            RuntimeFnId::RealpathCacheGet => "realpath_cache_get",
            RuntimeFnId::RealpathCacheSize => "realpath_cache_size",
            RuntimeFnId::Rename => "rename",
            RuntimeFnId::Rewind => "rewind",
            RuntimeFnId::Rewinddir => "rewinddir",
            RuntimeFnId::Rmdir => "rmdir",
            RuntimeFnId::Scandir => "scandir",
            RuntimeFnId::Stat => "stat",
            RuntimeFnId::StreamBucketAppend => "stream_bucket_append",
            RuntimeFnId::StreamBucketMakeWriteable => "stream_bucket_make_writeable",
            RuntimeFnId::StreamBucketNew => "stream_bucket_new",
            RuntimeFnId::StreamBucketPrepend => "stream_bucket_prepend",
            RuntimeFnId::StreamContextCreate => "stream_context_create",
            RuntimeFnId::StreamContextGetDefault => "stream_context_get_default",
            RuntimeFnId::StreamContextGetOptions => "stream_context_get_options",
            RuntimeFnId::StreamContextGetParams => "stream_context_get_params",
            RuntimeFnId::StreamContextSetDefault => "stream_context_set_default",
            RuntimeFnId::StreamContextSetOption => "stream_context_set_option",
            RuntimeFnId::StreamContextSetOptions => "stream_context_set_options",
            RuntimeFnId::StreamContextSetParams => "stream_context_set_params",
            RuntimeFnId::StreamCopyToStream => "stream_copy_to_stream",
            RuntimeFnId::StreamFilterAppend => "stream_filter_append",
            RuntimeFnId::StreamFilterPrepend => "stream_filter_prepend",
            RuntimeFnId::StreamFilterRegister => "stream_filter_register",
            RuntimeFnId::StreamFilterRemove => "stream_filter_remove",
            RuntimeFnId::StreamGetContents => "stream_get_contents",
            RuntimeFnId::StreamGetFilters => "stream_get_filters",
            RuntimeFnId::StreamGetLine => "stream_get_line",
            RuntimeFnId::StreamGetMetaData => "stream_get_meta_data",
            RuntimeFnId::StreamGetTransports => "stream_get_transports",
            RuntimeFnId::StreamGetWrappers => "stream_get_wrappers",
            RuntimeFnId::StreamIsLocal => "stream_is_local",
            RuntimeFnId::StreamIsatty => "stream_isatty",
            RuntimeFnId::StreamResolveIncludePath => "stream_resolve_include_path",
            RuntimeFnId::StreamSelect => "stream_select",
            RuntimeFnId::StreamSetBlocking => "stream_set_blocking",
            RuntimeFnId::StreamSetChunkSize => "stream_set_chunk_size",
            RuntimeFnId::StreamSetReadBuffer => "stream_set_read_buffer",
            RuntimeFnId::StreamSetTimeout => "stream_set_timeout",
            RuntimeFnId::StreamSetWriteBuffer => "stream_set_write_buffer",
            RuntimeFnId::StreamSocketAccept => "stream_socket_accept",
            RuntimeFnId::StreamSocketClient => "stream_socket_client",
            RuntimeFnId::StreamSocketEnableCrypto => "stream_socket_enable_crypto",
            RuntimeFnId::StreamSocketGetName => "stream_socket_get_name",
            RuntimeFnId::StreamSocketPair => "stream_socket_pair",
            RuntimeFnId::StreamSocketRecvfrom => "stream_socket_recvfrom",
            RuntimeFnId::StreamSocketSendto => "stream_socket_sendto",
            RuntimeFnId::StreamSocketServer => "stream_socket_server",
            RuntimeFnId::StreamSocketShutdown => "stream_socket_shutdown",
            RuntimeFnId::StreamSupportsLock => "stream_supports_lock",
            RuntimeFnId::StreamWrapperRegister => "stream_wrapper_register",
            RuntimeFnId::StreamWrapperRestore => "stream_wrapper_restore",
            RuntimeFnId::StreamWrapperUnregister => "stream_wrapper_unregister",
            RuntimeFnId::Symlink => "symlink",
            RuntimeFnId::SysGetTempDir => "sys_get_temp_dir",
            RuntimeFnId::Tempnam => "tempnam",
            RuntimeFnId::Tmpfile => "tmpfile",
            RuntimeFnId::Touch => "touch",
            RuntimeFnId::Umask => "umask",
            RuntimeFnId::Unlink => "unlink",
            RuntimeFnId::VarDump => "var_dump",
            RuntimeFnId::Vfprintf => "vfprintf",
            RuntimeFnId::Abs => "abs",
            RuntimeFnId::Acos => "acos",
            RuntimeFnId::Asin => "asin",
            RuntimeFnId::Atan => "atan",
            RuntimeFnId::Atan2 => "atan2",
            RuntimeFnId::BcAdd => "bcadd",
            RuntimeFnId::BcCeil => "bcceil",
            RuntimeFnId::BcComp => "bccomp",
            RuntimeFnId::BcDiv => "bcdiv",
            RuntimeFnId::BcDivmod => "bcdivmod",
            RuntimeFnId::BcFloor => "bcfloor",
            RuntimeFnId::BcMod => "bcmod",
            RuntimeFnId::BcMul => "bcmul",
            RuntimeFnId::BcPow => "bcpow",
            RuntimeFnId::BcPowmod => "bcpowmod",
            RuntimeFnId::BcRound => "bcround",
            RuntimeFnId::BcScale => "bcscale",
            RuntimeFnId::BcSqrt => "bcsqrt",
            RuntimeFnId::BcSub => "bcsub",
            RuntimeFnId::Ceil => "ceil",
            RuntimeFnId::Clamp => "clamp",
            RuntimeFnId::Cos => "cos",
            RuntimeFnId::Cosh => "cosh",
            RuntimeFnId::Bindec => "bindec",
            RuntimeFnId::BaseConvert => "base_convert",
            RuntimeFnId::Decbin => "decbin",
            RuntimeFnId::Dechex => "dechex",
            RuntimeFnId::Decoct => "decoct",
            RuntimeFnId::Deg2rad => "deg2rad",
            RuntimeFnId::Exp => "exp",
            RuntimeFnId::Fdiv => "fdiv",
            RuntimeFnId::Floor => "floor",
            RuntimeFnId::Fmod => "fmod",
            RuntimeFnId::Hexdec => "hexdec",
            RuntimeFnId::Hypot => "hypot",
            RuntimeFnId::Intdiv => "intdiv",
            RuntimeFnId::Log => "log",
            RuntimeFnId::Log10 => "log10",
            RuntimeFnId::Log2 => "log2",
            RuntimeFnId::Max => "max",
            RuntimeFnId::Min => "min",
            RuntimeFnId::MtRand => "mt_rand",
            RuntimeFnId::Pi => "pi",
            RuntimeFnId::Pow => "pow",
            RuntimeFnId::Rad2deg => "rad2deg",
            RuntimeFnId::Rand => "rand",
            RuntimeFnId::RandomInt => "random_int",
            RuntimeFnId::Round => "round",
            RuntimeFnId::Sin => "sin",
            RuntimeFnId::Sinh => "sinh",
            RuntimeFnId::Sqrt => "sqrt",
            RuntimeFnId::Tan => "tan",
            RuntimeFnId::Tanh => "tanh",
            RuntimeFnId::ElephcObjectIsEnum => "__elephc_object_is_enum",
            RuntimeFnId::ElephcObjectPropCount => "__elephc_object_prop_count",
            RuntimeFnId::ElephcObjectPropName => "__elephc_object_prop_name",
            RuntimeFnId::ElephcObjectPropValue => "__elephc_object_prop_value",
            RuntimeFnId::ElephcPtrIsNull => "__elephc_ptr_is_null",
            RuntimeFnId::ElephcPtrReadString => "__elephc_ptr_read_string",
            RuntimeFnId::ElephcPtrWriteString => "__elephc_ptr_write_string",
            RuntimeFnId::BufferFree => "buffer_free",
            RuntimeFnId::BufferLen => "buffer_len",
            RuntimeFnId::Ptr => "ptr",
            RuntimeFnId::PtrGet => "ptr_get",
            RuntimeFnId::PtrIsNull => "ptr_is_null",
            RuntimeFnId::PtrNull => "ptr_null",
            RuntimeFnId::PtrOffset => "ptr_offset",
            RuntimeFnId::PtrRead16 => "ptr_read16",
            RuntimeFnId::PtrRead32 => "ptr_read32",
            RuntimeFnId::PtrRead8 => "ptr_read8",
            RuntimeFnId::PtrReadString => "ptr_read_string",
            RuntimeFnId::PtrSet => "ptr_set",
            RuntimeFnId::PtrSizeof => "ptr_sizeof",
            RuntimeFnId::PtrWrite16 => "ptr_write16",
            RuntimeFnId::PtrWrite32 => "ptr_write32",
            RuntimeFnId::PtrWrite8 => "ptr_write8",
            RuntimeFnId::PtrWriteString => "ptr_write_string",
            RuntimeFnId::ZvalFree => "zval_free",
            RuntimeFnId::ZvalPack => "zval_pack",
            RuntimeFnId::ZvalType => "zval_type",
            RuntimeFnId::ZvalUnpack => "zval_unpack",
            RuntimeFnId::IteratorApply => "iterator_apply",
            RuntimeFnId::IteratorCount => "iterator_count",
            RuntimeFnId::IteratorToArray => "iterator_to_array",
            RuntimeFnId::SplAutoload => "spl_autoload",
            RuntimeFnId::SplAutoloadCall => "spl_autoload_call",
            RuntimeFnId::SplAutoloadExtensions => "spl_autoload_extensions",
            RuntimeFnId::SplAutoloadFunctions => "spl_autoload_functions",
            RuntimeFnId::SplAutoloadRegister => "spl_autoload_register",
            RuntimeFnId::SplAutoloadUnregister => "spl_autoload_unregister",
            RuntimeFnId::SplClasses => "spl_classes",
            RuntimeFnId::SplObjectHash => "spl_object_hash",
            RuntimeFnId::SplObjectId => "spl_object_id",
            RuntimeFnId::Base64Decode => "base64_decode",
            RuntimeFnId::Chop => "chop",
            RuntimeFnId::Chr => "chr",
            RuntimeFnId::ChunkSplit => "chunk_split",
            RuntimeFnId::CountChars => "count_chars",
            RuntimeFnId::Crc32 => "crc32",
            RuntimeFnId::CtypeAlnum => "ctype_alnum",
            RuntimeFnId::CtypeAlpha => "ctype_alpha",
            RuntimeFnId::CtypeDigit => "ctype_digit",
            RuntimeFnId::CtypeSpace => "ctype_space",
            RuntimeFnId::Explode => "explode",
            RuntimeFnId::GraphemeStrrev => "grapheme_strrev",
            RuntimeFnId::Gzcompress => "gzcompress",
            RuntimeFnId::Gzdeflate => "gzdeflate",
            RuntimeFnId::Gzinflate => "gzinflate",
            RuntimeFnId::Gzuncompress => "gzuncompress",
            RuntimeFnId::Hash => "hash",
            RuntimeFnId::HashAlgos => "hash_algos",
            RuntimeFnId::HashCopy => "__elephc_hash_ctx_copy",
            RuntimeFnId::HashEquals => "hash_equals",
            RuntimeFnId::HashFinal => "__elephc_hash_ctx_final",
            RuntimeFnId::HashHmac => "hash_hmac",
            RuntimeFnId::HashInit => "__elephc_hash_ctx_init",
            RuntimeFnId::HashUpdate => "__elephc_hash_ctx_update",
            RuntimeFnId::OpensslCipherIvLength => "openssl_cipher_iv_length",
            RuntimeFnId::OpensslDecrypt => "openssl_decrypt",
            RuntimeFnId::OpensslEncrypt => "openssl_encrypt",
            RuntimeFnId::OpensslGetCipherMethods => "openssl_get_cipher_methods",
            RuntimeFnId::Htmlentities => "htmlentities",
            RuntimeFnId::Htmlspecialchars => "htmlspecialchars",
            RuntimeFnId::Implode => "implode",
            RuntimeFnId::InetNtop => "inet_ntop",
            RuntimeFnId::InetPton => "inet_pton",
            RuntimeFnId::Ip2long => "ip2long",
            RuntimeFnId::Lcfirst => "lcfirst",
            RuntimeFnId::Long2ip => "long2ip",
            RuntimeFnId::Ltrim => "ltrim",
            RuntimeFnId::MbEregMatch => "mb_ereg_match",
            RuntimeFnId::MbStrlen => "mb_strlen",
            RuntimeFnId::Md5 => "md5",
            RuntimeFnId::NumberFormat => "number_format",
            RuntimeFnId::Octdec => "octdec",
            RuntimeFnId::Ord => "ord",
            RuntimeFnId::ParseUrl => "parse_url",
            RuntimeFnId::Printf => "printf",
            RuntimeFnId::Rtrim => "rtrim",
            RuntimeFnId::Sha1 => "sha1",
            RuntimeFnId::Sprintf => "sprintf",
            RuntimeFnId::StrContains => "str_contains",
            RuntimeFnId::StrGetcsv => "str_getcsv",
            RuntimeFnId::StrEndsWith => "str_ends_with",
            RuntimeFnId::StrIreplace => "str_ireplace",
            RuntimeFnId::StrPad => "str_pad",
            RuntimeFnId::StrRepeat => "str_repeat",
            RuntimeFnId::StrReplace => "str_replace",
            RuntimeFnId::StrSplit => "str_split",
            RuntimeFnId::StrStartsWith => "str_starts_with",
            RuntimeFnId::StrWordCount => "str_word_count",
            RuntimeFnId::Strcasecmp => "strcasecmp",
            RuntimeFnId::Strcmp => "strcmp",
            RuntimeFnId::Strncasecmp => "strncasecmp",
            RuntimeFnId::Strncmp => "strncmp",
            RuntimeFnId::Stripos => "stripos",
            RuntimeFnId::Strpos => "strpos",
            RuntimeFnId::Strripos => "strripos",
            RuntimeFnId::Strrpos => "strrpos",
            RuntimeFnId::Strtr => "strtr",
            RuntimeFnId::Strstr => "strstr",
            RuntimeFnId::Substr => "substr",
            RuntimeFnId::SubstrCount => "substr_count",
            RuntimeFnId::SubstrReplace => "substr_replace",
            RuntimeFnId::Trim => "trim",
            RuntimeFnId::Ucfirst => "ucfirst",
            RuntimeFnId::Ucwords => "ucwords",
            RuntimeFnId::Vprintf => "vprintf",
            RuntimeFnId::Vsprintf => "vsprintf",
            RuntimeFnId::Wordwrap => "wordwrap",
            RuntimeFnId::ElephcGmmktimeRaw => "__elephc_gmmktime_raw",
            RuntimeFnId::ElephcMktimeRaw => "__elephc_mktime_raw",
            RuntimeFnId::ElephcStrtotimeRaw => "__elephc_strtotime_raw",
            RuntimeFnId::Checkdate => "checkdate",
            RuntimeFnId::ClassAttributeArgs => "class_attribute_args",
            RuntimeFnId::ClassAttributeNames => "class_attribute_names",
            RuntimeFnId::ClassGetAttributes => "class_get_attributes",
            RuntimeFnId::Date => "date",
            RuntimeFnId::DateDefaultTimezoneGet => "date_default_timezone_get",
            RuntimeFnId::DateDefaultTimezoneSet => "date_default_timezone_set",
            RuntimeFnId::Define => "define",
            RuntimeFnId::Defined => "defined",
            RuntimeFnId::Exec => "exec",
            RuntimeFnId::ExtensionLoaded => "extension_loaded",
            RuntimeFnId::Getdate => "getdate",
            RuntimeFnId::Getenv => "getenv",
            RuntimeFnId::Gmdate => "gmdate",
            RuntimeFnId::Gmmktime => "gmmktime",
            RuntimeFnId::Header => "header",
            RuntimeFnId::Hrtime => "hrtime",
            RuntimeFnId::HttpClearLastResponseHeaders => "http_clear_last_response_headers",
            RuntimeFnId::HttpGetLastResponseHeaders => "http_get_last_response_headers",
            RuntimeFnId::HttpResponseCode => "http_response_code",
            RuntimeFnId::JsonDecode => "json_decode",
            RuntimeFnId::JsonEncode => "json_encode",
            RuntimeFnId::JsonLastError => "json_last_error",
            RuntimeFnId::JsonLastErrorMsg => "json_last_error_msg",
            RuntimeFnId::JsonValidate => "json_validate",
            RuntimeFnId::Localtime => "localtime",
            RuntimeFnId::Microtime => "microtime",
            RuntimeFnId::Mktime => "mktime",
            RuntimeFnId::Passthru => "passthru",
            RuntimeFnId::PhpUname => "php_uname",
            RuntimeFnId::Phpversion => "phpversion",
            RuntimeFnId::PregMatch => "preg_match",
            RuntimeFnId::PregMatchAll => "preg_match_all",
            RuntimeFnId::PregReplace => "preg_replace",
            RuntimeFnId::PregSplit => "preg_split",
            RuntimeFnId::Putenv => "putenv",
            RuntimeFnId::Serialize => "serialize",
            RuntimeFnId::ShellExec => "shell_exec",
            RuntimeFnId::Sleep => "sleep",
            RuntimeFnId::Strtotime => "strtotime",
            RuntimeFnId::System => "system",
            RuntimeFnId::Time => "time",
            RuntimeFnId::Unserialize => "unserialize",
            RuntimeFnId::Usleep => "usleep",
            RuntimeFnId::GetResourceId => "get_resource_id",
            RuntimeFnId::GetResourceType => "get_resource_type",
            RuntimeFnId::Gettype => "gettype",
            RuntimeFnId::IntvalBase => "intval_base",
            RuntimeFnId::IsCallable => "is_callable",
            RuntimeFnId::IsFinite => "is_finite",
            RuntimeFnId::IsInfinite => "is_infinite",
            RuntimeFnId::IsNan => "is_nan",
            RuntimeFnId::IsNumeric => "is_numeric",
            RuntimeFnId::Settype => "settype",
        }
    }
}

/// Truncates a runtime callable signature while keeping all parameter metadata aligned.
fn truncate_callable_params(sig: &mut crate::types::FunctionSig, count: usize) {
    sig.params.truncate(count);
    sig.defaults.truncate(count);
    sig.ref_params.truncate(count);
    sig.declared_params.truncate(count);
    if sig
        .variadic
        .as_deref()
        .is_some_and(|name| !sig.params.iter().any(|(param_name, _)| param_name == name))
    {
        sig.variadic = None;
    }
}

/// Replaces one runtime callable parameter type when the parameter exists.
fn set_callable_param_type(
    sig: &mut crate::types::FunctionSig,
    index: usize,
    php_type: crate::types::PhpType,
) {
    if let Some((_, param_ty)) = sig.params.get_mut(index) {
        *param_ty = php_type;
    }
}
