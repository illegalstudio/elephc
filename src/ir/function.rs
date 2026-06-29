//! Purpose:
//! Defines EIR functions, parameters, local slots, and per-function flags.
//!
//! Called from:
//! - `crate::ir::module`, `crate::ir::builder`, `crate::ir::validator`, and
//!   future IR codegen.
//!
//! Key details:
//! - Function-local tables own blocks, instructions, values, and locals. IDs
//!   are zero-based indices into those tables.

use crate::ir::block::{BasicBlock, BlockId};
use crate::ir::instr::{InstId, Instruction};
use crate::ir::types::IrType;
use crate::ir::value::{Value, ValueId};
use crate::parser::ast::Stmt;
use crate::types::{FunctionSig, PhpType};

/// Module-local identifier for an EIR function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(u32);

impl FunctionId {
    /// Creates a function identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Function-local identifier for an addressable local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalSlotId(u32);

impl LocalSlotId {
    /// Creates a local-slot identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// EIR function body and its function-local tables.
#[derive(Debug, Clone)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    pub locals: Vec<LocalSlot>,
    pub blocks: Vec<BasicBlock>,
    pub values: Vec<Value>,
    pub instructions: Vec<Instruction>,
    pub entry: BlockId,
    pub source_signature: Option<String>,
    pub signature: Option<FunctionSig>,
    pub generator_source: Option<GeneratorSource>,
    pub flags: FunctionFlags,
}

impl Function {
    /// Creates an empty function with no blocks or locals.
    pub fn new(name: String, return_type: IrType, return_php_type: PhpType) -> Self {
        Self {
            id: FunctionId::from_raw(0),
            name,
            params: Vec::new(),
            return_type,
            return_php_type,
            locals: Vec::new(),
            blocks: Vec::new(),
            values: Vec::new(),
            instructions: Vec::new(),
            entry: BlockId::from_raw(0),
            source_signature: None,
            signature: None,
            generator_source: None,
            flags: FunctionFlags::default(),
        }
    }

    /// Adds a local slot and returns its function-local identifier.
    pub fn add_local(&mut self, name: Option<String>, ir_type: IrType, php_type: PhpType, kind: LocalKind) -> LocalSlotId {
        let id = LocalSlotId::from_raw(self.locals.len() as u32);
        self.locals.push(LocalSlot {
            id,
            name,
            ir_type,
            php_type,
            kind,
        });
        id
    }

    /// Returns a block by identifier.
    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(id.as_raw() as usize)
    }

    /// Returns a mutable block by identifier.
    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(id.as_raw() as usize)
    }

    /// Returns an instruction by identifier.
    pub fn instruction(&self, id: InstId) -> Option<&Instruction> {
        self.instructions.get(id.as_raw() as usize)
    }

    /// Returns a mutable instruction by identifier, used by in-place IR passes.
    pub fn instruction_mut(&mut self, id: InstId) -> Option<&mut Instruction> {
        self.instructions.get_mut(id.as_raw() as usize)
    }

    /// Returns a value by identifier.
    pub fn value(&self, id: ValueId) -> Option<&Value> {
        self.values.get(id.as_raw() as usize)
    }

    /// Assigns the module-local function identifier.
    pub fn set_id(&mut self, id: FunctionId) {
        self.id = id;
    }
}

/// Source metadata retained for generator functions during the Phase 04 EIR backend bridge.
#[derive(Debug, Clone)]
pub struct GeneratorSource {
    pub body: Vec<Stmt>,
    pub visible_param_count: usize,
}

/// Caller-visible function parameter metadata.
#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub name: String,
    pub ir_type: IrType,
    pub php_type: PhpType,
    pub by_ref: bool,
    pub variadic: bool,
}

/// Addressable local slot metadata.
#[derive(Debug, Clone)]
pub struct LocalSlot {
    pub id: LocalSlotId,
    pub name: Option<String>,
    pub ir_type: IrType,
    pub php_type: PhpType,
    pub kind: LocalKind,
}

/// Role of a local slot in PHP/runtime semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    PhpLocal,
    GlobalAlias,
    StaticLocal,
    RefCell,
    HiddenTemp,
    OwnedTemp,
    TryHandler,
    ClosureCapture,
    NamedArgTemp,
    IteratorState,
    GeneratorState,
}

/// Function-level shape flags used by lowering and later codegen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionFlags {
    pub is_main: bool,
    pub is_method: bool,
    pub is_closure: bool,
    pub is_generator: bool,
    pub is_fiber_wrapper: bool,
    pub is_callback_wrapper: bool,
    pub is_runtime_callable_invoker: bool,
    pub is_static: bool,
    /// `true` when the function/closure returns by reference (`function &f()` / `fn &()`).
    pub by_ref_return: bool,
}
