//! Purpose:
//! Defines EIR instructions, opcodes, immediates, and instruction identifiers.
//!
//! Called from:
//! - `crate::ir::builder`, `crate::ir::validator`, `crate::ir::print`, and
//!   future lowering/codegen passes.
//!
//! Key details:
//! - Each opcode exposes a conservative default effect set. Call-like opcodes
//!   may be refined by builders once semantic metadata is available.

use crate::ir::effects::Effects;
use crate::ir::function::{FunctionId, LocalSlotId};
use crate::ir::module::DataId;
use crate::ir::runtime_call::RuntimeCallTarget;
use crate::ir::types::{IrHeapKind, IrType};
use crate::ir::value::{Ownership, ValueId};
use crate::span::Span;
use crate::types::PhpType;

/// Function-local identifier for an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstId(u32);

impl InstId {
    /// Creates an instruction identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Instruction payload stored in a function-level instruction table.
#[derive(Debug, Clone)]
pub struct Instruction {
    pub op: Op,
    pub operands: Vec<ValueId>,
    pub immediate: Option<Immediate>,
    pub result: Option<ValueId>,
    pub result_type: IrType,
    pub result_php_type: PhpType,
    pub result_ownership: Ownership,
    pub effects: Effects,
    pub span: Option<Span>,
    /// Optimization-pass provenance: set when a pass rewrote this instruction
    /// (const-fold) or moved it (LICM), so source maps can explain assembly
    /// that no longer matches the source shape. `None` for instructions
    /// lowered directly from the AST. A one-byte enum rather than a string:
    /// `Instruction` sits in the recursive lowering paths' stack frames, and
    /// growing it measurably shrinks the headroom before test threads overflow.
    pub origin: Option<PassOrigin>,
}

/// Optimization pass recorded as an instruction's provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassOrigin {
    ConstFold,
    Licm,
}

impl PassOrigin {
    /// Returns the lower-case spelling used by source maps and the EIR printer.
    pub fn name(self) -> &'static str {
        match self {
            PassOrigin::ConstFold => "const_fold",
            PassOrigin::Licm => "licm",
        }
    }
}

impl Instruction {
    /// Creates a new instruction payload with all semantic metadata attached.
    pub fn new(
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        result: Option<ValueId>,
        result_type: IrType,
        result_php_type: PhpType,
        result_ownership: Ownership,
        effects: Effects,
        span: Option<Span>,
    ) -> Self {
        Self {
            op,
            operands,
            immediate,
            result,
            result_type,
            result_php_type,
            result_ownership,
            effects,
            span,
            origin: None,
        }
    }

    /// Returns true when this instruction has no SSA result value.
    pub fn is_void(&self) -> bool {
        self.result.is_none() || self.result_type.is_void()
    }
}

/// Literal or metadata operand attached to an opcode.
#[derive(Debug, Clone, PartialEq)]
pub enum Immediate {
    I64(i64),
    F64(f64),
    Bool(bool),
    Data(DataId),
    /// Data-pool reference carrying the strict-PHP profile of its physical call site.
    ProfiledData {
        /// Referenced string or name data.
        data: DataId,
        /// Whether strict PHP is effective at this call site.
        strict_php: bool,
    },
    LocalSlot(LocalSlotId),
    LocalSlotPair {
        first: LocalSlotId,
        second: LocalSlotId,
    },
    GlobalName(DataId),
    FunctionRef(FunctionId),
    BuiltinRef(BuiltinId),
    RuntimeRef(RuntimeId),
    RuntimeCall(RuntimeCallTarget),
    ExternRef(u32),
    ClassRef(u32),
    EnumCaseRef {
        enum_id: u32,
        case_id: u32,
    },
    MethodRef {
        class: u32,
        method: u32,
    },
    PropertyRef {
        class: u32,
        property: u32,
    },
    FieldRef {
        layout: u32,
        field: u32,
    },
    FunctionVariantRef {
        group: u32,
        variant: u32,
    },
    HeapKind(IrHeapKind),
    MixedTag(u8),
    TypePredicate(PhpTypePredicate),
    MixedNumericOp(MixedNumericOp),
    /// Ordered add/sub/mul operations in an unboxed checked numeric chain.
    CheckedNumericChain(Box<CheckedNumericChainImmediate>),
    CmpPredicate(CmpPredicate),
    CastTarget(IrType),
    TypeName(DataId),
    Capacity(u32),
    WidthBytes(u8),
}

/// Heap-backed operation sequence carried by a fused checked numeric chain immediate.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedNumericChainImmediate {
    operations: Vec<MixedNumericOp>,
}

impl CheckedNumericChainImmediate {
    /// Creates a compact immediate from its ordered left-associated operations.
    pub fn new(operations: Vec<MixedNumericOp>) -> Self {
        Self { operations }
    }

    /// Returns the ordered operations evaluated by the fused chain.
    pub fn operations(&self) -> &[MixedNumericOp] {
        &self.operations
    }
}

/// Runtime arithmetic operation carried by `Op::MixedNumericBinop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixedNumericOp {
    Add,
    Sub,
    Mul,
    Pow,
}

/// PHP runtime type category tested by the backend-neutral `TypePredicate` opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhpTypePredicate {
    Array,
    Bool,
    Float,
    Int,
    Iterable,
    Object,
    Resource,
    Scalar,
    String,
}

impl PhpTypePredicate {
    /// Returns the stable textual spelling used by the EIR printer.
    pub const fn as_eir(self) -> &'static str {
        match self {
            Self::Array => "array",
            Self::Bool => "bool",
            Self::Float => "float",
            Self::Int => "int",
            Self::Iterable => "iterable",
            Self::Object => "object",
            Self::Resource => "resource",
            Self::Scalar => "scalar",
            Self::String => "string",
        }
    }
}

impl MixedNumericOp {
    /// Returns the lower-case textual spelling used by the EIR printer.
    pub fn as_eir(self) -> &'static str {
        match self {
            MixedNumericOp::Add => "add",
            MixedNumericOp::Sub => "sub",
            MixedNumericOp::Mul => "mul",
            MixedNumericOp::Pow => "pow",
        }
    }
}

/// Comparison predicate for integer and floating-point compare opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpPredicate {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Olt,
    Ole,
    Ogt,
    Oge,
}

/// Stable identifier for a builtin entry in the future IR metadata table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuiltinId(pub u32);

/// Stable identifier for a runtime helper entry in the future IR metadata table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeId(pub u32);

/// EIR opcode family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    ConstI64,
    ConstF64,
    ConstStr,
    ConstNull,
    ConstBool,
    ConstClassName,
    ConstEnumCase,
    LoadCalledClassId,
    DataAddr,
    LoadLocal,
    /// Reads a local that no store definitely reached: warns and answers `null`.
    ///
    /// PHP does not refuse these programs. `zval_undefined_cv` (php-src
    /// `Zend/zend_execute.c:280`) raises `Warning: Undefined variable $name` and returns
    /// `&EG(uninitialized_zval)`, so the read IS null and execution continues. Lowering a read of
    /// a never-stored slot as an ordinary `LoadLocal` reads uninitialized stack instead, which
    /// segfaults the moment the value is dereferenced.
    ///
    /// The immediate is the variable NAME; the whole diagnostic is composed at compile time from
    /// it. `MAY_WARN` is what gives the instruction its source line, through the same publisher
    /// every other warning uses.
    WarnedNull,
    StoreLocal,
    UnsetLocal,
    ZeroLocalSlot,
    LoadRefCell,
    StoreRefCell,
    PromoteLocalRefCell,
    AliasLocalRefCell,
    ReleaseLocalRefCell,
    ReleaseLocalSlot,
    LoadGlobal,
    StoreGlobal,
    LoadStaticLocal,
    StoreStaticLocal,
    InitStaticLocal,
    LoadStaticProperty,
    StoreStaticProperty,
    StaticPropInitialized,
    LoadReflectionStaticProperty,
    StoreReflectionStaticProperty,
    ReflectionStaticPropertyInitialized,
    IAdd,
    ISub,
    IMul,
    ICheckedAdd,
    ICheckedSub,
    ICheckedMul,
    /// Adds two integers with PHP overflow promotion, then applies PHP's integer cast
    /// without materializing the intermediate boxed `Mixed` value.
    ICheckedAddToInt,
    /// Subtracts two integers with PHP overflow promotion, then applies PHP's integer
    /// cast without materializing the intermediate boxed `Mixed` value.
    ICheckedSubToInt,
    /// Multiplies two integers with PHP overflow promotion, then applies PHP's integer
    /// cast without materializing the intermediate boxed `Mixed` value.
    ICheckedMulToInt,
    /// Evaluates a left-associated integer add/sub/mul chain in registers, promotes the
    /// remaining suffix to PHP float semantics on first overflow, then casts to `int`.
    ICheckedNumericChainToInt,
    ICheckedPow,
    IDiv,
    ISDiv,
    ISMod,
    IPow,
    INeg,
    IBitAnd,
    IBitOr,
    IBitXor,
    IBitNot,
    IShl,
    IShrA,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FPow,
    FNeg,
    MixedNumericBinop,
    StrIncDec,
    ICmp,
    FCmp,
    StrEq,
    StrCmp,
    StrLooseEq,
    StrictEq,
    StrictNotEq,
    LooseEq,
    LooseNotEq,
    PhpRelCmp,
    Spaceship,
    IsNull,
    IsTruthy,
    TypePredicate,
    IsEmpty,
    InstanceOf,
    IToF,
    FToI,
    IToStr,
    FToStr,
    BoolToStr,
    StrToI,
    StrToF,
    StrToNumber,
    ResourceToStr,
    Cast,
    MixedBox,
    /// Copies a boxed Mixed zval cell while retaining its nested payload for value semantics.
    MixedClone,
    InvokerRefArg,
    MixedUnbox,
    MixedTagOf,
    ArrayToMixed,
    HashToMixed,
    MixedCastBool,
    MixedCastInt,
    MixedCastFloat,
    MixedCastString,
    StrConcat,
    StrLen,
    StrPersist,
    StrCharAt,
    StrInterpolate,
    ConcatReset,
    WriteStrStdout,
    ArrayNew,
    HashNew,
    ArrayLen,
    HashLen,
    ArrayGet,
    ArrayGetSilent,
    /// Prepares an indexed element for mutation: boxed Mixed reads retain the owning cell, while
    /// typed container reads copy-on-write separate and republish the child in its parent slot.
    ArrayGetForWrite,
    HashGet,
    HashGetSilent,
    /// Prepares an associative element for mutation: boxed Mixed reads retain the owning cell,
    /// while typed container reads copy-on-write separate and republish the child in its entry.
    HashGetForWrite,
    ArrayIsset,
    HashIsset,
    ArrayElemAddr,
    ArraySet,
    HashSet,
    HashUnset,
    /// Writes PHP null into `container[key]`, releasing whatever was there.
    ///
    /// Used by the nested-append lowering to hand a bucket's *only other* reference over to
    /// the temporary that is about to be appended to: after the read the bucket is owned by
    /// both the slot and the temp (refcount 2), which would make the append copy-on-write
    /// clone it — O(length) on every push, hence O(n^2) over a growing bucket. Nulling the
    /// slot drops it back to 1, so the append mutates in place, and the write-back then
    /// re-publishes the bucket into the very same slot.
    ///
    /// It can never free the bucket: it only ever runs *after* the read has taken its
    /// reference, so the refcount it decrements is at least 2.
    SlotDetach,
    ArrayPush,
    MixedArrayAppend,
    HashAppend,
    ArrayEnsureUnique,
    HashEnsureUnique,
    ArrayCloneShallow,
    HashCloneShallow,
    ArrayUnion,
    HashUnion,
    ArrayHashUnion,
    HashArrayUnion,
    HashSpread,
    ArrayToHash,
    ArraySetMixedKey,
    ArrayGetMixedKey,
    ArrayGetMixedKeySilent,
    ArrayKeyExists,
    OffsetExists,
    OffsetUnset,
    ListUnpack,
    IterStart,
    IterCurrentKey,
    IterCurrentValue,
    IterCurrentValueRef,
    IterNext,
    IterEnd,
    IteratorMethodCall,
    SplRuntimeCall,
    ObjectNew,
    EvalObjectNew,
    ObjectCloneShallow,
    DynamicObjectNew,
    DynamicObjectNewMixed,
    DynamicObjectNewWithoutConstructorMixed,
    /// Reinterprets one runtime callable descriptor as an opaque bridge pointer.
    CallablePtr,
    /// Normalizes any supported PHP callable form into an owned descriptor.
    NormalizeCallable,
    /// Returns the address of one compiler-emitted PDO callback adapter.
    PdoAdapterAddr,
    /// Reports whether an AOT class selected by runtime name has a constructor.
    DynamicClassHasConstructor,
    /// Classifies a runtime class name for PDO statement construction.
    DynamicPdoStatementClassStatus,
    /// Classifies a runtime late-static class name for `PDO::connect()`.
    DynamicPdoCalledClassStatus,
    /// Invokes a PDO statement subclass constructor from a boxed argument container.
    DynamicPdoStatementConstructorCall,
    /// Initializes the private base state of a PDO statement subclass.
    DynamicPdoStatementInitialize,
    PropGet,
    PropGetForWrite,
    PropInitialized,
    PropSet,
    /// Clears a declared instance-property slot for `unset($obj->prop)`: releases the
    /// refcounted payload the slot owned and stamps the uninitialized-typed-property
    /// marker, so the property stops being reported by `isset()` and by the
    /// descriptor walkers. Operand: object; immediate: property name data id.
    PropUnset,
    /// Loads the raw reference-cell pointer stored in a reference property's slot,
    /// without dereferencing it. Used to alias a local to `$obj->prop` and to return
    /// `$this->prop` by reference. Operand: object; immediate: property name data id.
    LoadPropRefCell,
    /// Promotes an indexed-array element to a reference cell and returns the cell
    /// pointer. Used to alias a local to `$a[idx]` (`$b =& $a[0]`). The returned pointer
    /// addresses the element's inline storage within the array; the local aliases it
    /// non-owning (the array owns the storage). Operands: array, index. No immediate.
    LoadArrayElemRefCell,
    /// Binds a local slot as a non-owning reference alias to a ref-cell pointer value.
    /// Operand: the cell pointer (SSA value); immediate: target local slot. The local
    /// does not own the cell (no release at scope exit); the owner is the object/source.
    BindRefCellPtr,
    DynamicPropGet,
    DynamicPropSet,
    NullsafePropGet,
    NullsafeMethodCall,
    MethodLookup,
    MethodCall,
    StaticMethodCall,
    EvalStaticMethodCall,
    /// Coerces a PHP numeric string operand to its integer value for an int-backed enum
    /// `from()`/`tryFrom()` call. Operand: the string. Immediate: data id of the PHP
    /// `TypeError` message thrown when the string is not numeric. Result: `I64`.
    EnumBackingStringToInt,
    /// Coerces a `Mixed` (dynamically-typed) operand to the integer backing value for an
    /// int-backed enum `from()`/`tryFrom()` call, dispatching on the runtime tag: int/bool
    /// forward the payload, float truncates, null becomes 0, a numeric string coerces (a
    /// non-numeric string throws `TypeError`), and array/object/resource/callable throw
    /// `TypeError`. Operand: the Mixed value. Immediate: data id of the PHP `TypeError`
    /// message prefix (`"E::from(): Argument #1 ($value) must be of type int, "`), to which
    /// codegen appends the runtime type word. Result: `I64`.
    EnumBackingMixedToInt,
    /// Narrows a `Mixed` value to the raw `I64` payload a packed `int` field stores, WITHOUT
    /// coercion: only the int tag passes; every other runtime tag throws `TypeError`. A packed
    /// field is a fixed-layout systems extension, so the PHP coercions `EnumBackingMixedToInt`
    /// performs (float truncation, numeric strings, null-to-0) would silently corrupt the very
    /// overflow the boxed value exists to report. Operand: the Mixed value. Immediate: data id
    /// of the `TypeError` message prefix (`"Packed field C::$f must be of type int, "`), to
    /// which codegen appends the runtime type word. Result: `I64`.
    PackedFieldMixedToInt,
    /// Narrows a value reaching a DECLARED `int` return boundary with PHP's coercive-mode
    /// verification, replacing the silent truncation the plain int coercion performs.
    /// Matching `php -n` 8.5: int/bool forward the payload, a numeric string coerces, an
    /// in-range float truncates, and everything else — a non-numeric string, null, array,
    /// object, resource, Closure, or a float outside `[-2^63, 2^63)` (NaN included) — throws
    /// a catchable `TypeError`. Operand: the value (boxed Mixed or raw F64 after constant
    /// folding). Immediate: data id of the message prefix
    /// (`"f(): Return value must be of type int, "`), to which codegen appends the runtime
    /// type word and `" returned"`. Result: `I64`.
    ReturnBoundaryMixedToInt,
    ClassConstant,
    ScopedConstantGet,
    ClassAttrNames,
    ClassAttrArgs,
    ClassGetAttributes,
    InstanceOfDynamic,
    Call,
    FunctionVariantCall,
    ClosureBind,
    LanguageConstructCall,
    EvalLiteralCall,
    EvalScopeGet,
    EvalScopeSet,
    EvalFunctionCall,
    EvalFunctionCallArray,
    EvalFunctionExists,
    EvalClassExists,
    EvalConstantExists,
    EvalConstantFetch,
    RuntimeCall,
    /// Reads through a boxed Mixed/ArrayAccess receiver for an imminent nested write.
    MixedArrayGetForWrite,
    ExternCall,
    ClosureNew,
    ClosureCapture,
    ClosureCall,
    ExprCall,
    FirstClassCallableNew,
    CallableArrayNew,
    CallableDescriptorInvoke,
    PipeCall,
    PtrCast,
    PtrRead,
    PtrWrite,
    PtrReadString,
    PtrWriteString,
    PtrOffset,
    PtrCheckNonnull,
    BufferNew,
    BufferLen,
    BufferGet,
    BufferSet,
    BufferFree,
    PackedFieldGet,
    PackedFieldSet,
    ExternGlobalLoad,
    ExternGlobalStore,
    EchoValue,
    PrintValue,
    WriteStdout,
    VarDump,
    PrintR,
    ErrorSuppressBegin,
    ErrorSuppressEnd,
    Warn,
    ThrowException,
    ThrowError,
    ThrowErrorValue,
    TryPushHandler,
    TryPopHandler,
    CatchCurrent,
    CatchBind,
    FinallyEnter,
    FinallyExit,
    FiberRuntimeCall,
    GeneratorNew,
    GeneratorYield,
    GeneratorYieldFrom,
    GeneratorReturn,
    IncludeOnceMark,
    IncludeOnceGuard,
    FunctionVariantMark,
    FunctionVariantDispatch,
    Acquire,
    Release,
    ReleaseUnlessAliases,
    GcCollect,
    Move,
    Borrow,
    EnsureOwned,
    Nop,
}

impl Op {
    /// Returns the conservative default effect set for this opcode.
    pub fn default_effects(self) -> Effects {
        use Effects as E;
        use Op::*;
        match self {
            ConstI64
            | ConstF64
            | ConstStr
            | ConstNull
            | ConstBool
            | ConstClassName
            | DataAddr
            | IAdd
            | ISub
            | IMul
            | ICheckedAddToInt
            | ICheckedSubToInt
            | ICheckedMulToInt
            | ICheckedNumericChainToInt
            | IPow
            | INeg
            | IBitAnd
            | IBitOr
            | IBitXor
            | IBitNot
            | FAdd
            | FSub
            | FMul
            | FPow
            | FNeg
            | ICmp
            | FCmp
            | StrLen
            | IToF
            | FToI
            | BoolToStr
            | StrToI
            | StrToF
            | StrToNumber
            | MixedTagOf
            | IsEmpty
            | FunctionVariantDispatch
            | PtrCast
            | PtrOffset
            | CallablePtr
            | PdoAdapterAddr
            | DynamicClassHasConstructor
            | DynamicPdoStatementClassStatus
            | DynamicPdoCalledClassStatus
            | Move
            | Borrow
            | Nop => E::PURE,
            // PHP 8 raises catchable errors here, so these are never removable, hoistable,
            // or CSE-able: `/` and `%` throw `DivisionByZeroError` for a zero divisor and
            // `<<` / `>>` throw `ArithmeticError` for a negative shift count.
            IDiv | ISDiv | ISMod => E::MAY_FATAL | E::MAY_THROW,
            IShl | IShrA | FDiv => E::MAY_THROW,
            PtrCheckNonnull => E::MAY_FATAL,
            ICheckedAdd | ICheckedSub | ICheckedMul | ICheckedPow => E::ALLOC_HEAP | E::READS_HEAP,
            ConstEnumCase => E::ALLOC_HEAP,
            LoadCalledClassId => E::READS_LOCAL,
            LoadLocal | LoadRefCell | LoadStaticLocal | ClosureCapture => E::READS_LOCAL,
            // Reads nothing — the point is that there is nothing to read — but it WARNS, and
            // `MAY_WARN` is the gate the per-instruction location publisher runs on.
            WarnedNull => E::MAY_WARN,
            StoreLocal | UnsetLocal | ZeroLocalSlot | StoreRefCell | ListUnpack | FinallyEnter
            | FinallyExit => E::WRITES_LOCAL,
            PromoteLocalRefCell => {
                E::READS_LOCAL | E::WRITES_LOCAL | E::ALLOC_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP
            }
            AliasLocalRefCell => E::READS_LOCAL | E::WRITES_LOCAL,
            ReleaseLocalRefCell => {
                E::READS_LOCAL | E::WRITES_LOCAL | E::WRITES_HEAP | E::REFCOUNT_OP
            }
            ReleaseLocalSlot => E::READS_LOCAL | E::WRITES_HEAP | E::REFCOUNT_OP,
            LoadGlobal
            | LoadStaticProperty
            | StaticPropInitialized
            | LoadReflectionStaticProperty
            | ReflectionStaticPropertyInitialized
            | ScopedConstantGet
            | ClassAttrNames
            | ClassAttrArgs
            | ClassGetAttributes
            | CatchCurrent => E::READS_GLOBAL,
            CatchBind => E::READS_GLOBAL | E::WRITES_GLOBAL,
            StoreGlobal
            | StoreStaticLocal
            | StoreStaticProperty
            | StoreReflectionStaticProperty
            | InitStaticLocal
            | IncludeOnceMark
            | FunctionVariantMark
            | TryPushHandler
            | TryPopHandler => E::WRITES_GLOBAL,
            IncludeOnceGuard => E::READS_GLOBAL | E::WRITES_GLOBAL,
            IToStr | ResourceToStr | StrConcat | StrCharAt | StrInterpolate | VarDump | PrintR => {
                E::ALLOC_CONCAT
            }
            // A float reaching a string coercion can be NaN, and PHP warns when it is
            // (`unexpected NAN value was coerced to string`, MEASURED on `php -n` 8.5.6 — with the
            // ` in FILE on line N` tail every diagnostic carries). `__rt_ftoa` raises it from the
            // runtime, so the only thing that can supply the line is this instruction declaring
            // that it may warn. Without it the warning still printed, and printed WITHOUT a
            // location — which read as program output rather than as a diagnostic.
            //
            // Deliberately NOT extended to `StrConcat`: it joins strings that are already
            // formatted, so no NaN can reach it, and `MAY_WARN` costs four stores at every site.
            FToStr | MixedCastString => E::ALLOC_CONCAT | E::MAY_WARN,
            ConcatReset => E::WRITES_GLOBAL,
            Cast => {
                E::READS_HEAP | E::ALLOC_HEAP | E::ALLOC_CONCAT | E::MAY_WARN | E::MAY_FATAL
            }
            InvokerRefArg => E::READS_LOCAL | E::ALLOC_HEAP,
            MixedBox | MixedClone | ArrayToMixed | HashToMixed | ArrayNew | HashNew | ObjectNew
            | ClosureNew | FirstClassCallableNew | CallableArrayNew | NormalizeCallable | BufferNew
            | GeneratorNew => {
                E::ALLOC_HEAP
            }
            IsNull | IsTruthy | TypePredicate | MixedUnbox | MixedCastBool | MixedCastInt
            | MixedCastFloat | BufferGet | BufferLen | PackedFieldGet | PtrRead
            | PtrReadString => {
                E::READS_HEAP | E::MAY_FATAL
            }
            ArrayGetSilent | HashGetSilent | ArrayIsset | HashIsset => E::READS_HEAP,
            ArrayGet | HashGet => E::READS_HEAP | E::MAY_WARN,
            // Not a pure read despite the name: the copy-on-write split rewrites the receiver's
            // element slot (and the receiver's own local slot), so it must never be treated as
            // reorderable or redundant against the plain reads around it.
            ArrayGetForWrite | HashGetForWrite => {
                E::READS_HEAP | E::WRITES_HEAP | E::WRITES_LOCAL | E::ALLOC_HEAP
                    | E::REFCOUNT_OP | E::MAY_WARN | E::MAY_FATAL
            }
            StrPersist | ArrayEnsureUnique | HashEnsureUnique | ArrayCloneShallow
            | HashCloneShallow | ObjectCloneShallow => {
                E::READS_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP
            }
            ArrayLen | HashLen => E::READS_HEAP,
            ArrayKeyExists | OffsetExists | PropInitialized | LoadPropRefCell => {
                E::READS_HEAP
            }
            PropGet | NullsafePropGet => {
                E::READS_HEAP | E::MAY_THROW | E::MAY_WARN | E::MAY_DEOPT
            }
            // Not a pure read despite the name, exactly like `ArrayGetForWrite`: the
            // copy-on-write split rewrites the receiver's PROPERTY slot, so it must never be
            // treated as reorderable or redundant against the plain property reads around it.
            PropGetForWrite => {
                E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP
                    | E::MAY_THROW | E::MAY_WARN | E::MAY_DEOPT
            }
            DynamicPropGet => {
                E::READS_HEAP | E::MAY_THROW | E::MAY_WARN | E::MAY_DEOPT
            }
            LoadArrayElemRefCell => E::READS_HEAP | E::MAY_FATAL,
            BindRefCellPtr => E::WRITES_LOCAL,
            ArraySet | HashSet | HashUnset | ArrayPush | HashAppend | OffsetUnset | PropSet
            | PropUnset | DynamicPropSet | BufferSet | BufferFree | PackedFieldSet | PtrWrite
            | PtrWriteString => E::WRITES_HEAP | E::MAY_FATAL | E::REFCOUNT_OP,
            MixedArrayAppend => E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::REFCOUNT_OP,
            // ALLOC_HEAP because the hash-storage lowering goes through `__rt_hash_set`, which
            // checks its load factor and may grow/rehash the table before it even knows whether
            // the key is already present.
            SlotDetach => E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::REFCOUNT_OP,
            ArrayElemAddr | ArraySetMixedKey => {
                E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::REFCOUNT_OP
            }
            ArrayGetMixedKey => E::READS_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::MAY_WARN,
            ArrayGetMixedKeySilent => E::READS_HEAP | E::ALLOC_HEAP | E::MAY_FATAL,
            ArrayUnion | HashUnion | ArrayHashUnion | HashArrayUnion | ArrayToHash => {
                E::READS_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP
            }
            HashSpread => E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP,
            MethodCall | NullsafeMethodCall => {
                E::READS_HEAP | E::MAY_THROW | E::MAY_DEOPT
            }
            IterStart | IterCurrentKey | IterCurrentValue | IteratorMethodCall
            | SplRuntimeCall | DynamicObjectNew | DynamicObjectNewMixed
            | DynamicObjectNewWithoutConstructorMixed | MethodLookup | StaticMethodCall
            | InstanceOfDynamic | MixedNumericBinop | LooseEq | LooseNotEq | PhpRelCmp
            | Spaceship => {
                E::READS_HEAP | E::MAY_DEOPT
            }
            // `++`/`--` on a string reads the operand's payload, may write the shared
            // concat scratch while building the carried result, and always allocates the
            // boxed Mixed cell the new value is returned in.
            StrIncDec => E::READS_HEAP | E::ALLOC_CONCAT | E::ALLOC_HEAP | E::MAY_DEOPT,
            IterCurrentValueRef | IterNext | IterEnd | GeneratorYield | GeneratorYieldFrom | GeneratorReturn => {
                E::READS_HEAP | E::WRITES_HEAP | E::MAY_DEOPT
            }
            StrEq | StrCmp | StrLooseEq | StrictEq | StrictNotEq | InstanceOf => E::READS_HEAP,
            EnumBackingStringToInt | EnumBackingMixedToInt | PackedFieldMixedToInt
            | ReturnBoundaryMixedToInt => {
                E::READS_HEAP | E::ALLOC_HEAP | E::MAY_THROW
            }
            EvalFunctionExists | EvalClassExists | EvalConstantExists => E::READS_GLOBAL,
            EvalScopeGet => E::READS_HEAP | E::MAY_FATAL,
            EvalScopeSet => E::READS_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP | E::MAY_FATAL,
            EvalConstantFetch => {
                E::READS_GLOBAL | E::READS_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP | E::MAY_FATAL
            }
            Call
            | FunctionVariantCall
            | ClosureBind
            | LanguageConstructCall
            | EvalLiteralCall
            | EvalFunctionCall
            | EvalFunctionCallArray
            | EvalObjectNew
            | EvalStaticMethodCall
            | RuntimeCall
            | MixedArrayGetForWrite
            | DynamicPdoStatementConstructorCall
            | DynamicPdoStatementInitialize
            | ClosureCall
            | ExprCall
            | CallableDescriptorInvoke
            | PipeCall
            | FiberRuntimeCall => E::all().difference(E::REFCOUNT_OP),
            ExternCall | ExternGlobalLoad | ExternGlobalStore => {
                E::READS_HEAP | E::WRITES_HEAP | E::READS_PROCESS | E::WRITES_PROCESS | E::MAY_THROW
            }
            EchoValue | WriteStrStdout | WriteStdout | Warn => E::OUTPUT,
            PrintValue => E::OUTPUT,
            ErrorSuppressBegin | ErrorSuppressEnd => E::READS_GLOBAL | E::WRITES_GLOBAL,
            ThrowException => E::MAY_THROW | E::WRITES_GLOBAL,
            ThrowError | ThrowErrorValue => {
                E::MAY_THROW
                    | E::READS_GLOBAL
                    | E::WRITES_GLOBAL
                    | E::ALLOC_HEAP
                    | E::WRITES_HEAP
            }
            Acquire | Release | EnsureOwned => E::REFCOUNT_OP | E::WRITES_HEAP,
            ReleaseUnlessAliases => E::REFCOUNT_OP | E::WRITES_HEAP | E::READS_HEAP,
            GcCollect => E::READS_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP,
            ClassConstant => E::MAY_DEOPT,
        }
    }

    /// Returns true when the builder may replace the conservative default effects.
    ///
    /// The arithmetic opcodes below default to `MAY_THROW` because PHP raises a catchable
    /// `DivisionByZeroError` / `ArithmeticError` for a zero divisor or a negative shift count.
    /// `ir_lower::expr::arithmetic_effects()` drops that bit when the right operand is a literal
    /// that rules the error out, so `$x << 3` and `$x / 2` stay removable, hoistable, and
    /// CSE-able exactly as they were before the guards existed.
    pub fn allows_effect_refinement(self) -> bool {
        matches!(
            self,
            // Whether an echo can warn depends on WHAT is echoed, not on the opcode: a float
            // reaching the formatter can be NaN, which PHP warns about, and an array prints
            // `Array` with a conversion warning, while a string literal cannot warn at all.
            // Declaring `MAY_WARN` unconditionally would make every `echo "..."` in every program
            // pay for the location stores; refining it per site keeps that pay-for-use, which is
            // the same reason the call opcodes below refine theirs. `print` renders through the
            // same path and refines for the same reason.
            Op::EchoValue
                | Op::PrintValue
                | Op::IDiv
                | Op::ISDiv
                | Op::ISMod
                | Op::FDiv
                | Op::IShl
                | Op::IShrA
                | Op::Call
                | Op::FunctionVariantCall
                | Op::ClosureBind
                | Op::LanguageConstructCall
                | Op::EvalLiteralCall
                | Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::EvalObjectNew
                | Op::EvalStaticMethodCall
                | Op::RuntimeCall
                | Op::ExternCall
                | Op::MethodCall
                | Op::StaticMethodCall
                | Op::PropGet
                | Op::NullsafePropGet
                | Op::DynamicPropGet
                | Op::ClosureCall
                | Op::ExprCall
                | Op::CallableDescriptorInvoke
                | Op::PipeCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::FiberRuntimeCall
        )
    }

    /// Returns the lower-case textual opcode spelling.
    pub fn name(self) -> &'static str {
        use Op::*;
        match self {
            ConstI64 => "const_i64",
            ConstF64 => "const_f64",
            ConstStr => "const_str",
            ConstNull => "const_null",
            ConstBool => "const_bool",
            ConstClassName => "const_class_name",
            ConstEnumCase => "const_enum_case",
            LoadCalledClassId => "load_called_class_id",
            DataAddr => "data_addr",
            LoadLocal => "load_local",
            WarnedNull => "warned_null",
            StoreLocal => "store_local",
            UnsetLocal => "unset_local",
            ZeroLocalSlot => "zero_local_slot",
            LoadRefCell => "load_ref_cell",
            StoreRefCell => "store_ref_cell",
            PromoteLocalRefCell => "promote_local_ref_cell",
            AliasLocalRefCell => "alias_local_ref_cell",
            ReleaseLocalRefCell => "release_local_ref_cell",
            ReleaseLocalSlot => "release_local_slot",
            LoadGlobal => "load_global",
            StoreGlobal => "store_global",
            LoadStaticLocal => "load_static_local",
            StoreStaticLocal => "store_static_local",
            InitStaticLocal => "init_static_local",
            LoadStaticProperty => "load_static_property",
            StoreStaticProperty => "store_static_property",
            StaticPropInitialized => "static_prop_initialized",
            LoadReflectionStaticProperty => "load_reflection_static_property",
            StoreReflectionStaticProperty => "store_reflection_static_property",
            ReflectionStaticPropertyInitialized => "reflection_static_property_initialized",
            IAdd => "iadd",
            ISub => "isub",
            IMul => "imul",
            ICheckedAdd => "ichecked_add",
            ICheckedSub => "ichecked_sub",
            ICheckedMul => "ichecked_mul",
            ICheckedAddToInt => "ichecked_add_to_int",
            ICheckedSubToInt => "ichecked_sub_to_int",
            ICheckedMulToInt => "ichecked_mul_to_int",
            ICheckedNumericChainToInt => "ichecked_numeric_chain_to_int",
            ICheckedPow => "ichecked_pow",
            IDiv => "idiv",
            ISDiv => "isdiv",
            ISMod => "ismod",
            IPow => "ipow",
            INeg => "ineg",
            IBitAnd => "ibit_and",
            IBitOr => "ibit_or",
            IBitXor => "ibit_xor",
            IBitNot => "ibit_not",
            IShl => "ishl",
            IShrA => "ishr_a",
            FAdd => "fadd",
            FSub => "fsub",
            FMul => "fmul",
            FDiv => "fdiv",
            FPow => "fpow",
            FNeg => "fneg",
            MixedNumericBinop => "mixed_numeric_binop",
            StrIncDec => "str_inc_dec",
            ICmp => "icmp",
            FCmp => "fcmp",
            StrEq => "str_eq",
            StrCmp => "str_cmp",
            StrLooseEq => "str_loose_eq",
            StrictEq => "strict_eq",
            StrictNotEq => "strict_not_eq",
            LooseEq => "loose_eq",
            LooseNotEq => "loose_not_eq",
            PhpRelCmp => "php_rel_cmp",
            Spaceship => "spaceship",
            IsNull => "is_null",
            IsTruthy => "is_truthy",
            TypePredicate => "type_predicate",
            IsEmpty => "is_empty",
            InstanceOf => "instance_of",
            IToF => "i_to_f",
            FToI => "f_to_i",
            IToStr => "i_to_str",
            FToStr => "f_to_str",
            BoolToStr => "bool_to_str",
            StrToI => "str_to_i",
            StrToF => "str_to_f",
            StrToNumber => "str_to_number",
            ResourceToStr => "resource_to_str",
            Cast => "cast",
            MixedBox => "mixed_box",
            MixedClone => "mixed_clone",
            InvokerRefArg => "invoker_ref_arg",
            MixedUnbox => "mixed_unbox",
            MixedTagOf => "mixed_tag_of",
            ArrayToMixed => "array_to_mixed",
            HashToMixed => "hash_to_mixed",
            MixedCastBool => "mixed_cast_bool",
            MixedCastInt => "mixed_cast_int",
            MixedCastFloat => "mixed_cast_float",
            MixedCastString => "mixed_cast_string",
            StrConcat => "str_concat",
            StrLen => "str_len",
            StrPersist => "str_persist",
            StrCharAt => "str_char_at",
            StrInterpolate => "str_interpolate",
            ConcatReset => "concat_reset",
            WriteStrStdout => "write_str_stdout",
            ArrayNew => "array_new",
            HashNew => "hash_new",
            ArrayLen => "array_len",
            HashLen => "hash_len",
            ArrayGet => "array_get",
            ArrayGetSilent => "array_get_silent",
            ArrayGetForWrite => "array_get_for_write",
            HashGet => "hash_get",
            HashGetSilent => "hash_get_silent",
            HashGetForWrite => "hash_get_for_write",
            ArrayIsset => "array_isset",
            HashIsset => "hash_isset",
            ArrayElemAddr => "array_elem_addr",
            ArraySet => "array_set",
            HashSet => "hash_set",
            HashUnset => "hash_unset",
            SlotDetach => "slot_detach",
            ArrayPush => "array_push",
            MixedArrayAppend => "mixed_array_append",
            HashAppend => "hash_append",
            ArrayEnsureUnique => "array_ensure_unique",
            HashEnsureUnique => "hash_ensure_unique",
            ArrayCloneShallow => "array_clone_shallow",
            HashCloneShallow => "hash_clone_shallow",
            ArrayUnion => "array_union",
            HashUnion => "hash_union",
            ArrayHashUnion => "array_hash_union",
            HashArrayUnion => "hash_array_union",
            HashSpread => "hash_spread",
            ArrayToHash => "array_to_hash",
            ArraySetMixedKey => "array_set_mixed_key",
            ArrayGetMixedKey => "array_get_mixed_key",
            ArrayGetMixedKeySilent => "array_get_mixed_key_silent",
            ArrayKeyExists => "array_key_exists",
            OffsetExists => "offset_exists",
            OffsetUnset => "offset_unset",
            ListUnpack => "list_unpack",
            IterStart => "iter_start",
            IterCurrentKey => "iter_current_key",
            IterCurrentValue => "iter_current_value",
            IterCurrentValueRef => "iter_current_value_ref",
            IterNext => "iter_next",
            IterEnd => "iter_end",
            IteratorMethodCall => "iterator_method_call",
            SplRuntimeCall => "spl_runtime_call",
            ObjectNew => "object_new",
            EvalObjectNew => "eval_object_new",
            ObjectCloneShallow => "object_clone_shallow",
            DynamicObjectNew => "dynamic_object_new",
            DynamicObjectNewMixed => "dynamic_object_new_mixed",
            DynamicObjectNewWithoutConstructorMixed => {
                "dynamic_object_new_without_constructor_mixed"
            }
            CallablePtr => "callable_ptr",
            NormalizeCallable => "normalize_callable",
            PdoAdapterAddr => "pdo_adapter_addr",
            DynamicClassHasConstructor => "dynamic_class_has_constructor",
            DynamicPdoStatementClassStatus => "dynamic_pdo_statement_class_status",
            DynamicPdoCalledClassStatus => "dynamic_pdo_called_class_status",
            DynamicPdoStatementConstructorCall => "dynamic_pdo_statement_constructor_call",
            DynamicPdoStatementInitialize => "dynamic_pdo_statement_initialize",
            PropGet => "prop_get",
            PropGetForWrite => "prop_get_for_write",
            PropInitialized => "prop_initialized",
            PropSet => "prop_set",
            PropUnset => "prop_unset",
            LoadPropRefCell => "load_prop_ref_cell",
            LoadArrayElemRefCell => "load_array_elem_ref_cell",
            BindRefCellPtr => "bind_ref_cell_ptr",
            DynamicPropGet => "dynamic_prop_get",
            DynamicPropSet => "dynamic_prop_set",
            NullsafePropGet => "nullsafe_prop_get",
            NullsafeMethodCall => "nullsafe_method_call",
            MethodLookup => "method_lookup",
            MethodCall => "method_call",
            StaticMethodCall => "static_method_call",
            EvalStaticMethodCall => "eval_static_method_call",
            EnumBackingStringToInt => "enum_backing_string_to_int",
            EnumBackingMixedToInt => "enum_backing_mixed_to_int",
            PackedFieldMixedToInt => "packed_field_mixed_to_int",
            ReturnBoundaryMixedToInt => "return_boundary_mixed_to_int",
            ClassConstant => "class_constant",
            ScopedConstantGet => "scoped_constant_get",
            ClassAttrNames => "class_attr_names",
            ClassAttrArgs => "class_attr_args",
            ClassGetAttributes => "class_get_attributes",
            InstanceOfDynamic => "instance_of_dynamic",
            Call => "call",
            FunctionVariantCall => "function_variant_call",
            ClosureBind => "closure_bind",
            LanguageConstructCall => "language_construct_call",
            EvalLiteralCall => "eval_literal_call",
            EvalScopeGet => "eval_scope_get",
            EvalScopeSet => "eval_scope_set",
            EvalFunctionCall => "eval_function_call",
            EvalFunctionCallArray => "eval_function_call_array",
            EvalFunctionExists => "eval_function_exists",
            EvalClassExists => "eval_class_exists",
            EvalConstantExists => "eval_constant_exists",
            EvalConstantFetch => "eval_constant_fetch",
            RuntimeCall => "runtime_call",
            MixedArrayGetForWrite => "mixed_array_get_for_write",
            ExternCall => "extern_call",
            ClosureNew => "closure_new",
            ClosureCapture => "closure_capture",
            ClosureCall => "closure_call",
            ExprCall => "expr_call",
            FirstClassCallableNew => "first_class_callable_new",
            CallableArrayNew => "callable_array_new",
            CallableDescriptorInvoke => "callable_descriptor_invoke",
            PipeCall => "pipe_call",
            PtrCast => "ptr_cast",
            PtrRead => "ptr_read",
            PtrWrite => "ptr_write",
            PtrReadString => "ptr_read_string",
            PtrWriteString => "ptr_write_string",
            PtrOffset => "ptr_offset",
            PtrCheckNonnull => "ptr_check_nonnull",
            BufferNew => "buffer_new",
            BufferLen => "buffer_len",
            BufferGet => "buffer_get",
            BufferSet => "buffer_set",
            BufferFree => "buffer_free",
            PackedFieldGet => "packed_field_get",
            PackedFieldSet => "packed_field_set",
            ExternGlobalLoad => "extern_global_load",
            ExternGlobalStore => "extern_global_store",
            EchoValue => "echo_value",
            PrintValue => "print_value",
            WriteStdout => "write_stdout",
            VarDump => "var_dump",
            PrintR => "print_r",
            ErrorSuppressBegin => "error_suppress_begin",
            ErrorSuppressEnd => "error_suppress_end",
            Warn => "warn",
            ThrowException => "throw_exception",
            ThrowError => "throw_error",
            ThrowErrorValue => "throw_error_value",
            TryPushHandler => "try_push_handler",
            TryPopHandler => "try_pop_handler",
            CatchCurrent => "catch_current",
            CatchBind => "catch_bind",
            FinallyEnter => "finally_enter",
            FinallyExit => "finally_exit",
            FiberRuntimeCall => "fiber_runtime_call",
            GeneratorNew => "generator_new",
            GeneratorYield => "generator_yield",
            GeneratorYieldFrom => "generator_yield_from",
            GeneratorReturn => "generator_return",
            IncludeOnceMark => "include_once_mark",
            IncludeOnceGuard => "include_once_guard",
            FunctionVariantMark => "function_variant_mark",
            FunctionVariantDispatch => "function_variant_dispatch",
            Acquire => "acquire",
            Release => "release",
            ReleaseUnlessAliases => "release_unless_aliases",
            GcCollect => "gc_collect",
            Move => "move",
            Borrow => "borrow",
            EnsureOwned => "ensure_owned",
            Nop => "nop",
        }
    }
}

#[cfg(test)]
mod tests {
    /// `Instruction` is built by value inside the recursive AST->EIR lowering
    /// paths, so its size feeds every lowering stack frame. Growing it past
    /// main's 112 bytes shrank the headroom enough that 2 MiB test threads
    /// overflowed on linux-aarch64. Keep provenance and future metadata inside
    /// the existing padding.
    #[test]
    fn instruction_stays_112_bytes() {
        let size = std::mem::size_of::<super::Instruction>();
        assert!(size <= 112, "Instruction grew to {size} bytes");
    }
}
