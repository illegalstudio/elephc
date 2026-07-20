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
    GetDeclaredClasses,
    GetDeclaredInterfaces,
    GetDeclaredTraits,
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
    Fscanf,
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
    Ceil,
    Clamp,
    Cos,
    Cosh,
    Deg2rad,
    Exp,
    Fdiv,
    Floor,
    Fmod,
    Hypot,
    Intdiv,
    Log,
    Log10,
    Log2,
    Max,
    Min,
    MtRand,
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
    Chop,
    Chr,
    Crc32,
    CtypeAlnum,
    CtypeAlpha,
    CtypeDigit,
    CtypeSpace,
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
    Printf,
    Rtrim,
    Sha1,
    Sprintf,
    Sscanf,
    StrContains,
    StrEndsWith,
    StrIreplace,
    StrPad,
    StrRepeat,
    StrReplace,
    StrSplit,
    StrStartsWith,
    Strcasecmp,
    Strcmp,
    Strpos,
    Strrpos,
    Strstr,
    Substr,
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
    Getdate,
    Getenv,
    Gmdate,
    Gmmktime,
    Header,
    Hrtime,
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
        let logical_signature = crate::builtins::registry::runtime_fn_arity_bounds(self).map(
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
            RuntimeFnId::ArrayValues => match arg_types.first().map(PhpType::codegen_repr) {
                Some(PhpType::Array(element)) => PhpType::Array(element),
                Some(PhpType::AssocArray { value, .. }) => PhpType::Array(value),
                Some(other) => other,
                None => declared.clone(),
            },
            RuntimeFnId::ClassAttributeArgs => PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
            RuntimeFnId::ClassAttributeNames
            | RuntimeFnId::Explode
            | RuntimeFnId::File
            | RuntimeFnId::Glob
            | RuntimeFnId::Scandir
            | RuntimeFnId::SplClasses => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::ClassGetAttributes => PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            ))),
            RuntimeFnId::ElephcPharListEntries => PhpType::Array(Box::new(PhpType::Str)),
            RuntimeFnId::PregSplit => PhpType::Array(Box::new(PhpType::Mixed)),
            RuntimeFnId::Range => PhpType::Array(Box::new(PhpType::Int)),
            _ => declared.clone(),
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
            RuntimeFnId::Abs |
            RuntimeFnId::Acos |
            RuntimeFnId::ArrayChunk |
            RuntimeFnId::ArrayColumn |
            RuntimeFnId::ArrayCombine |
            RuntimeFnId::ArrayDiff |
            RuntimeFnId::ArrayDiffAssoc |
            RuntimeFnId::ArrayDiffKey |
            RuntimeFnId::ArrayFill |
            RuntimeFnId::ArrayFillKeys |
            RuntimeFnId::ArrayFlip |
            RuntimeFnId::ArrayIntersect |
            RuntimeFnId::ArrayIntersectAssoc |
            RuntimeFnId::ArrayIntersectKey |
            RuntimeFnId::ArrayIsList |
            RuntimeFnId::ArrayKeyExists |
            RuntimeFnId::ArrayKeyFirst |
            RuntimeFnId::ArrayKeyLast |
            RuntimeFnId::ArrayKeys |
            RuntimeFnId::ArrayMerge |
            RuntimeFnId::ArrayMergeRecursive |
            RuntimeFnId::ArrayPad |
            RuntimeFnId::ArrayProduct |
            RuntimeFnId::ArrayReplace |
            RuntimeFnId::ArrayReplaceRecursive |
            RuntimeFnId::ArrayReverse |
            RuntimeFnId::ArraySearch |
            RuntimeFnId::ArraySlice |
            RuntimeFnId::ArraySum |
            RuntimeFnId::ArrayUnique |
            RuntimeFnId::ArrayValues |
            RuntimeFnId::Asin |
            RuntimeFnId::Atan |
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
            RuntimeFnId::Deg2rad |
            RuntimeFnId::Exp |
            RuntimeFnId::Explode |
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
            RuntimeFnId::Hypot |
            RuntimeFnId::Implode |
            RuntimeFnId::InetNtop |
            RuntimeFnId::InetPton |
            RuntimeFnId::Ip2long |
            RuntimeFnId::IsNumeric |
            RuntimeFnId::Lcfirst |
            RuntimeFnId::Log |
            RuntimeFnId::Log10 |
            RuntimeFnId::Log2 |
            RuntimeFnId::Long2ip |
            RuntimeFnId::Ltrim |
            RuntimeFnId::Max |
            RuntimeFnId::Md5 |
            RuntimeFnId::Min |
            RuntimeFnId::NumberFormat |
            RuntimeFnId::Ord |
            RuntimeFnId::Pi |
            RuntimeFnId::Pow |
            RuntimeFnId::Rad2deg |
            RuntimeFnId::Range |
            RuntimeFnId::Round |
            RuntimeFnId::Rtrim |
            RuntimeFnId::Sha1 |
            RuntimeFnId::Sin |
            RuntimeFnId::Sinh |
            RuntimeFnId::Sqrt |
            RuntimeFnId::StrContains |
            RuntimeFnId::StrEndsWith |
            RuntimeFnId::StrIreplace |
            RuntimeFnId::StrPad |
            RuntimeFnId::StrRepeat |
            RuntimeFnId::StrReplace |
            RuntimeFnId::StrSplit |
            RuntimeFnId::StrStartsWith |
            RuntimeFnId::Strcasecmp |
            RuntimeFnId::Strcmp |
            RuntimeFnId::Strpos |
            RuntimeFnId::Strrpos |
            RuntimeFnId::Strstr |
            RuntimeFnId::Substr |
            RuntimeFnId::SubstrReplace |
            RuntimeFnId::Tan |
            RuntimeFnId::Tanh |
            RuntimeFnId::Trim |
            RuntimeFnId::Ucfirst |
            RuntimeFnId::Ucwords |
            RuntimeFnId::Wordwrap => crate::ir::Effects::empty(),
            _ => crate::ir::Effects::from_bits_retain(
                crate::ir::Effects::all().bits()
                    & !crate::ir::Effects::REFCOUNT_OP.bits()
                    & !crate::ir::Effects::WRITES_GLOBAL.bits(),
            ),
        }
    }

    /// Returns runtime and linker requirements declared by this typed operation.
    pub const fn requirements(
        self,
    ) -> &'static [crate::builtins::semantics::BuiltinRequirement] {
        use crate::builtins::semantics::BuiltinRequirement;
        match self {
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

    /// Returns whether this operation can publish PHAR bridge helper symbols.
    pub const fn publishes_phar_symbols(self) -> bool {
        matches!(
            self,
            RuntimeFnId::ElephcPharListEntries
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
            RuntimeFnId::ArrayMap => Some(0),
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
        if matches!(
            self,
            RuntimeFnId::ArrayChunk
                | RuntimeFnId::ArrayColumn
                | RuntimeFnId::ArrayCombine
                | RuntimeFnId::ArrayDiff
                | RuntimeFnId::ArrayFill
                | RuntimeFnId::ArrayFillKeys
                | RuntimeFnId::ArrayIntersect
                | RuntimeFnId::ArrayKeys
                | RuntimeFnId::ArrayMap
                | RuntimeFnId::ArrayMerge
                | RuntimeFnId::ArrayPad
                | RuntimeFnId::ArrayPop
                | RuntimeFnId::ArrayReplace
                | RuntimeFnId::ArrayReplaceRecursive
                | RuntimeFnId::ArrayReverse
                | RuntimeFnId::ArrayShift
                | RuntimeFnId::ArraySlice
                | RuntimeFnId::ArrayUnique
                | RuntimeFnId::ArrayValues
                | RuntimeFnId::Explode
                | RuntimeFnId::FileGetContents
                | RuntimeFnId::IteratorToArray
                | RuntimeFnId::ObGetClean
                | RuntimeFnId::ObGetContents
                | RuntimeFnId::ObGetFlush
                | RuntimeFnId::ObGetLength
                | RuntimeFnId::ObGetStatus
                | RuntimeFnId::ObListHandlers
                | RuntimeFnId::PregSplit
                | RuntimeFnId::PtrReadString
                | RuntimeFnId::Range
                | RuntimeFnId::StrSplit
                | RuntimeFnId::Strpos
                | RuntimeFnId::Strrpos
                | RuntimeFnId::ZvalUnpack
        ) {
            BuiltinResultOwnership::Fresh
        } else if matches!(
            self,
            RuntimeFnId::Htmlentities
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
            RuntimeFnId::GetDeclaredClasses => "get_declared_classes",
            RuntimeFnId::GetDeclaredInterfaces => "get_declared_interfaces",
            RuntimeFnId::GetDeclaredTraits => "get_declared_traits",
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
            RuntimeFnId::Fscanf => "fscanf",
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
            RuntimeFnId::Ceil => "ceil",
            RuntimeFnId::Clamp => "clamp",
            RuntimeFnId::Cos => "cos",
            RuntimeFnId::Cosh => "cosh",
            RuntimeFnId::Deg2rad => "deg2rad",
            RuntimeFnId::Exp => "exp",
            RuntimeFnId::Fdiv => "fdiv",
            RuntimeFnId::Floor => "floor",
            RuntimeFnId::Fmod => "fmod",
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
            RuntimeFnId::Chop => "chop",
            RuntimeFnId::Chr => "chr",
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
            RuntimeFnId::HashCopy => "hash_copy",
            RuntimeFnId::HashEquals => "hash_equals",
            RuntimeFnId::HashFinal => "hash_final",
            RuntimeFnId::HashHmac => "hash_hmac",
            RuntimeFnId::HashInit => "hash_init",
            RuntimeFnId::HashUpdate => "hash_update",
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
            RuntimeFnId::Ord => "ord",
            RuntimeFnId::Printf => "printf",
            RuntimeFnId::Rtrim => "rtrim",
            RuntimeFnId::Sha1 => "sha1",
            RuntimeFnId::Sprintf => "sprintf",
            RuntimeFnId::Sscanf => "sscanf",
            RuntimeFnId::StrContains => "str_contains",
            RuntimeFnId::StrEndsWith => "str_ends_with",
            RuntimeFnId::StrIreplace => "str_ireplace",
            RuntimeFnId::StrPad => "str_pad",
            RuntimeFnId::StrRepeat => "str_repeat",
            RuntimeFnId::StrReplace => "str_replace",
            RuntimeFnId::StrSplit => "str_split",
            RuntimeFnId::StrStartsWith => "str_starts_with",
            RuntimeFnId::Strcasecmp => "strcasecmp",
            RuntimeFnId::Strcmp => "strcmp",
            RuntimeFnId::Strpos => "strpos",
            RuntimeFnId::Strrpos => "strrpos",
            RuntimeFnId::Strstr => "strstr",
            RuntimeFnId::Substr => "substr",
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
            RuntimeFnId::Getdate => "getdate",
            RuntimeFnId::Getenv => "getenv",
            RuntimeFnId::Gmdate => "gmdate",
            RuntimeFnId::Gmmktime => "gmmktime",
            RuntimeFnId::Header => "header",
            RuntimeFnId::Hrtime => "hrtime",
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
