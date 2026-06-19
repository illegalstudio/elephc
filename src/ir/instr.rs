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
    LocalSlot(LocalSlotId),
    LocalSlotPair { first: LocalSlotId, second: LocalSlotId },
    GlobalName(DataId),
    FunctionRef(FunctionId),
    BuiltinRef(BuiltinId),
    RuntimeRef(RuntimeId),
    ExternRef(u32),
    ClassRef(u32),
    EnumCaseRef { enum_id: u32, case_id: u32 },
    MethodRef { class: u32, method: u32 },
    PropertyRef { class: u32, property: u32 },
    FieldRef { layout: u32, field: u32 },
    FunctionVariantRef { group: u32, variant: u32 },
    HeapKind(IrHeapKind),
    MixedTag(u8),
    MixedNumericOp(MixedNumericOp),
    CmpPredicate(CmpPredicate),
    CastTarget(IrType),
    TypeName(DataId),
    Capacity(u32),
    WidthBytes(u8),
}

/// Runtime arithmetic operation carried by `Op::MixedNumericBinop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixedNumericOp {
    Add,
    Sub,
    Mul,
}

impl MixedNumericOp {
    /// Returns the lower-case textual spelling used by the EIR printer.
    pub fn as_eir(self) -> &'static str {
        match self {
            MixedNumericOp::Add => "add",
            MixedNumericOp::Sub => "sub",
            MixedNumericOp::Mul => "mul",
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
    StoreLocal,
    UnsetLocal,
    LoadRefCell,
    StoreRefCell,
    PromoteLocalRefCell,
    AliasLocalRefCell,
    ReleaseLocalRefCell,
    LoadGlobal,
    StoreGlobal,
    LoadStaticLocal,
    StoreStaticLocal,
    InitStaticLocal,
    LoadStaticProperty,
    StoreStaticProperty,
    IAdd,
    ISub,
    IMul,
    ICheckedAdd,
    ICheckedSub,
    ICheckedMul,
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
    ICmp,
    FCmp,
    StrEq,
    StrCmp,
    StrLooseEq,
    StrictEq,
    StrictNotEq,
    LooseEq,
    LooseNotEq,
    Spaceship,
    IsNull,
    IsTruthy,
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
    HashGet,
    ArrayIsset,
    HashIsset,
    ArrayElemAddr,
    ArraySet,
    HashSet,
    HashUnset,
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
    ObjectCloneShallow,
    DynamicObjectNew,
    DynamicObjectNewMixed,
    PropGet,
    PropSet,
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
    ClassConstant,
    ScopedConstantGet,
    ClassAttrNames,
    ClassAttrArgs,
    ClassGetAttributes,
    InstanceOfDynamic,
    Call,
    FunctionVariantCall,
    BuiltinCall,
    EvalFunctionCall,
    EvalFunctionCallArray,
    EvalFunctionExists,
    EvalClassExists,
    EvalConstantExists,
    EvalConstantFetch,
    RuntimeCall,
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
            ConstI64 | ConstF64 | ConstStr | ConstNull | ConstBool | ConstClassName
            | DataAddr | IAdd | ISub | IMul | IPow | INeg | IBitAnd | IBitOr | IBitXor
            | IBitNot | IShl | IShrA | FAdd | FSub | FMul | FDiv | FPow | FNeg | ICmp
            | FCmp | StrLen | IToF | FToI | BoolToStr | StrToI | StrToF | StrToNumber
            | MixedTagOf | IsNull | IsTruthy | IsEmpty | FunctionVariantDispatch | PtrCast
            | PtrOffset | Move | Borrow | Nop => E::PURE,
            IDiv | ISDiv | ISMod | PtrCheckNonnull => E::MAY_FATAL,
            ICheckedAdd | ICheckedSub | ICheckedMul => E::ALLOC_HEAP | E::READS_HEAP,
            ConstEnumCase => E::ALLOC_HEAP,
            LoadCalledClassId => E::READS_LOCAL,
            LoadLocal | LoadRefCell | LoadStaticLocal | ClosureCapture => E::READS_LOCAL,
            StoreLocal | UnsetLocal | StoreRefCell | ListUnpack | CatchBind | FinallyEnter
            | FinallyExit => E::WRITES_LOCAL,
            PromoteLocalRefCell => {
                E::READS_LOCAL | E::WRITES_LOCAL | E::ALLOC_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP
            },
            AliasLocalRefCell => E::READS_LOCAL | E::WRITES_LOCAL,
            ReleaseLocalRefCell => E::READS_LOCAL | E::WRITES_LOCAL | E::WRITES_HEAP | E::REFCOUNT_OP,
            LoadGlobal | LoadStaticProperty | ScopedConstantGet | ClassAttrNames
            | ClassAttrArgs | ClassGetAttributes | CatchCurrent => E::READS_GLOBAL,
            StoreGlobal | StoreStaticLocal | StoreStaticProperty | InitStaticLocal | IncludeOnceMark
            | FunctionVariantMark | TryPushHandler | TryPopHandler => E::WRITES_GLOBAL,
            IncludeOnceGuard => E::READS_GLOBAL | E::WRITES_GLOBAL,
            IToStr | FToStr | ResourceToStr | StrConcat | StrCharAt | StrInterpolate
            | MixedCastString | VarDump | PrintR => E::ALLOC_CONCAT,
            ConcatReset => E::WRITES_GLOBAL,
            Cast => E::READS_HEAP | E::ALLOC_CONCAT | E::MAY_WARN | E::MAY_FATAL,
            InvokerRefArg => E::READS_LOCAL | E::ALLOC_HEAP,
            MixedBox | ArrayToMixed | HashToMixed | ArrayNew | HashNew | ObjectNew
            | ClosureNew | FirstClassCallableNew | CallableArrayNew | BufferNew | GeneratorNew => {
                E::ALLOC_HEAP
            }
            MixedUnbox | MixedCastBool | MixedCastInt | MixedCastFloat | ArrayGetSilent | HashGet
            | ArrayIsset | HashIsset | BufferGet | BufferLen | PackedFieldGet | PtrRead
            | PtrReadString => {
                E::READS_HEAP | E::MAY_FATAL
            }
            ArrayGet => E::READS_HEAP | E::MAY_FATAL | E::MAY_WARN,
            StrPersist | ArrayEnsureUnique | HashEnsureUnique | ArrayCloneShallow
            | HashCloneShallow | ObjectCloneShallow => {
                E::READS_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP
            }
            ArrayLen | HashLen | ArrayKeyExists | OffsetExists | PropGet | LoadPropRefCell => {
                E::READS_HEAP
            }
            LoadArrayElemRefCell => E::READS_HEAP | E::MAY_FATAL,
            BindRefCellPtr => E::WRITES_LOCAL,
            ArraySet | HashSet | HashUnset | ArrayPush | HashAppend | OffsetUnset | PropSet
            | DynamicPropSet | BufferSet | BufferFree | PackedFieldSet | PtrWrite
            | PtrWriteString => E::WRITES_HEAP | E::MAY_FATAL | E::REFCOUNT_OP,
            MixedArrayAppend => E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::REFCOUNT_OP,
            ArrayElemAddr | ArraySetMixedKey => {
                E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::REFCOUNT_OP
            }
            ArrayGetMixedKey => E::READS_HEAP | E::ALLOC_HEAP | E::MAY_FATAL | E::MAY_WARN,
            ArrayGetMixedKeySilent => E::READS_HEAP | E::ALLOC_HEAP | E::MAY_FATAL,
            ArrayUnion | HashUnion | ArrayHashUnion | HashArrayUnion | ArrayToHash => {
                E::READS_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP
            }
            HashSpread => E::READS_HEAP | E::WRITES_HEAP | E::ALLOC_HEAP | E::REFCOUNT_OP,
            IterStart | IterCurrentKey | IterCurrentValue | IteratorMethodCall
            | SplRuntimeCall | DynamicObjectNew | DynamicObjectNewMixed | DynamicPropGet | NullsafePropGet
            | NullsafeMethodCall | MethodLookup | MethodCall | StaticMethodCall
            | InstanceOfDynamic | MixedNumericBinop | LooseEq | LooseNotEq | Spaceship => {
                E::READS_HEAP | E::MAY_DEOPT
            }
            IterCurrentValueRef | IterNext | IterEnd | GeneratorYield | GeneratorYieldFrom | GeneratorReturn => {
                E::READS_HEAP | E::WRITES_HEAP | E::MAY_DEOPT
            }
            StrEq | StrCmp | StrLooseEq | StrictEq | StrictNotEq | InstanceOf => E::READS_HEAP,
            EnumBackingStringToInt | EnumBackingMixedToInt => {
                E::READS_HEAP | E::ALLOC_HEAP | E::MAY_THROW
            }
            EvalFunctionExists | EvalClassExists | EvalConstantExists => E::READS_GLOBAL,
            EvalConstantFetch => {
                E::READS_GLOBAL | E::READS_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP | E::MAY_FATAL
            }
            Call | FunctionVariantCall | BuiltinCall | EvalFunctionCall | EvalFunctionCallArray | RuntimeCall
            | ClosureCall | ExprCall | CallableDescriptorInvoke | PipeCall | FiberRuntimeCall => {
                E::all().difference(E::REFCOUNT_OP)
            }
            ExternCall | ExternGlobalLoad | ExternGlobalStore => {
                E::READS_HEAP | E::WRITES_HEAP | E::READS_PROCESS | E::WRITES_PROCESS | E::MAY_THROW
            }
            EchoValue | WriteStrStdout | WriteStdout | Warn => E::OUTPUT,
            PrintValue => E::OUTPUT,
            ErrorSuppressBegin | ErrorSuppressEnd => E::READS_GLOBAL | E::WRITES_GLOBAL,
            ThrowException => E::MAY_THROW | E::WRITES_GLOBAL,
            Acquire | Release | EnsureOwned => E::REFCOUNT_OP | E::WRITES_HEAP,
            GcCollect => E::READS_HEAP | E::WRITES_HEAP | E::REFCOUNT_OP,
            ClassConstant => E::MAY_DEOPT,
        }
    }

    /// Returns true when the builder may replace the conservative default effects.
    pub fn allows_effect_refinement(self) -> bool {
        matches!(
            self,
            Op::Call
                | Op::FunctionVariantCall
                | Op::BuiltinCall
                | Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::RuntimeCall
                | Op::ExternCall
                | Op::MethodCall
                | Op::StaticMethodCall
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
            StoreLocal => "store_local",
            UnsetLocal => "unset_local",
            LoadRefCell => "load_ref_cell",
            StoreRefCell => "store_ref_cell",
            PromoteLocalRefCell => "promote_local_ref_cell",
            AliasLocalRefCell => "alias_local_ref_cell",
            ReleaseLocalRefCell => "release_local_ref_cell",
            LoadGlobal => "load_global",
            StoreGlobal => "store_global",
            LoadStaticLocal => "load_static_local",
            StoreStaticLocal => "store_static_local",
            InitStaticLocal => "init_static_local",
            LoadStaticProperty => "load_static_property",
            StoreStaticProperty => "store_static_property",
            IAdd => "iadd",
            ISub => "isub",
            IMul => "imul",
            ICheckedAdd => "ichecked_add",
            ICheckedSub => "ichecked_sub",
            ICheckedMul => "ichecked_mul",
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
            ICmp => "icmp",
            FCmp => "fcmp",
            StrEq => "str_eq",
            StrCmp => "str_cmp",
            StrLooseEq => "str_loose_eq",
            StrictEq => "strict_eq",
            StrictNotEq => "strict_not_eq",
            LooseEq => "loose_eq",
            LooseNotEq => "loose_not_eq",
            Spaceship => "spaceship",
            IsNull => "is_null",
            IsTruthy => "is_truthy",
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
            HashGet => "hash_get",
            ArrayIsset => "array_isset",
            HashIsset => "hash_isset",
            ArrayElemAddr => "array_elem_addr",
            ArraySet => "array_set",
            HashSet => "hash_set",
            HashUnset => "hash_unset",
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
            ObjectCloneShallow => "object_clone_shallow",
            DynamicObjectNew => "dynamic_object_new",
            DynamicObjectNewMixed => "dynamic_object_new_mixed",
            PropGet => "prop_get",
            PropSet => "prop_set",
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
            EnumBackingStringToInt => "enum_backing_string_to_int",
            EnumBackingMixedToInt => "enum_backing_mixed_to_int",
            ClassConstant => "class_constant",
            ScopedConstantGet => "scoped_constant_get",
            ClassAttrNames => "class_attr_names",
            ClassAttrArgs => "class_attr_args",
            ClassGetAttributes => "class_get_attributes",
            InstanceOfDynamic => "instance_of_dynamic",
            Call => "call",
            FunctionVariantCall => "function_variant_call",
            BuiltinCall => "builtin_call",
            EvalFunctionCall => "eval_function_call",
            EvalFunctionCallArray => "eval_function_call_array",
            EvalFunctionExists => "eval_function_exists",
            EvalClassExists => "eval_class_exists",
            EvalConstantExists => "eval_constant_exists",
            EvalConstantFetch => "eval_constant_fetch",
            RuntimeCall => "runtime_call",
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
        assert!(std::mem::size_of::<super::Instruction>() <= 112);
    }
}
