//! Purpose:
//! Recognizes a closed numeric closure expression and emits a private scalar-argument variant.
//!
//! Called from:
//! - `ir_passes::numeric_closure_calls` after exact call argument types are available.
//!
//! Key details:
//! - The accepted graph contains no calls, references, globals, diagnostics or observable cleanup.
//! - Numeric expressions retain checked overflow and a boxed Mixed return ABI.

use std::collections::HashMap;
use crate::ir::{Builder, Function, Immediate, IrHeapKind, IrType, LocalKind, MixedNumericOp,
    Op, Ownership, Terminator, ValueId};
use crate::types::PhpType;

/// Numeric expression nodes share indices instead of expanding repeated subexpressions.
enum Node { Parameter(usize), Integer(i64), Float(f64), Binary(MixedNumericOp, usize, usize) }

/// A bounded numeric body whose value semantics are independent of Mixed wrapper identity.
pub(super) struct Template {
    pub(super) index: u32,
    pub(super) params: Vec<PhpType>,
    names: Vec<String>,
    source_name: String,
    lexical_class: Option<String>,
    flags: crate::ir::FunctionFlags,
    nodes: Vec<Node>,
    spans: Vec<Option<crate::span::Span>>,
    result: usize,
    reset: bool,
}

impl Template {
    /// Rejects all bodies whose semantics cannot be reconstructed from scalar value expressions.
    pub(super) fn analyze(function: &Function) -> Option<Self> {
        if !function.flags.is_closure || function.flags.is_synthetic || function.flags.by_ref_return
            || function.flags.is_main || function.flags.is_method
            || function.flags.is_generator || function.flags.is_fiber_wrapper
            || function.blocks.len() != 1 || !function.blocks[0].params.is_empty()
            || function.instructions.len() > 64 || function.params.is_empty() || function.params.len() > 16
            || function.return_php_type != PhpType::Mixed
            || function.return_type != IrType::Heap(IrHeapKind::Mixed)
            || function.params.iter().any(|param| param.by_ref || param.variadic
                || !matches!(param.php_type, PhpType::Int | PhpType::Float | PhpType::Mixed))
            || !function.params.iter().any(|param| param.php_type == PhpType::Mixed)
            || function.locals.iter().any(|local| local.kind != LocalKind::PhpLocal)
            || !function.attribute_names.is_empty()
            || function.signature.as_ref().is_some_and(|signature| signature.deprecation.is_some()
                || signature.param_attributes.iter().any(|attributes| !attributes.is_empty()))
        {
            return None;
        }
        let mut nodes: Vec<_> = (0..function.params.len()).map(Node::Parameter).collect();
        let mut spans = vec![None; nodes.len()];
        let mut locals = HashMap::new();
        for (index, param) in function.params.iter().enumerate() {
            let local = function.locals.iter().find(|local| local.name.as_ref() == Some(&param.name))?;
            locals.insert(local.id, index);
        }
        let mut values: HashMap<ValueId, usize> = HashMap::new();
        let mut reset = false;
        for &id in &function.blocks[0].instructions {
            let inst = function.instruction(id)?;
            let node = match inst.op {
                Op::Nop => continue,
                Op::ConcatReset => { reset = true; continue; }
                Op::LoadLocal => {
                    let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
                    let local = function.locals.get(slot.as_raw() as usize)?;
                    if local.php_type.codegen_repr() != inst.result_php_type.codegen_repr() { return None; }
                    *locals.get(&slot)?
                }
                Op::StoreLocal => {
                    let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
                    // Incoming parameters are immutable in this first specialization family.
                    if (slot.as_raw() as usize) < function.params.len() { return None; }
                    let local = function.locals.get(slot.as_raw() as usize)?;
                    let value = function.value(*inst.operands.first()?)?;
                    if local.php_type.codegen_repr() != PhpType::Mixed
                        && local.php_type.codegen_repr() != value.php_type.codegen_repr()
                    { return None; }
                    locals.insert(slot, *values.get(inst.operands.first()?)?);
                    continue;
                }
                Op::Release | Op::ReleaseLocalSlot => {
                    if inst.op == Op::Release { values.get(inst.operands.first()?)?; }
                    else {
                        let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
                        locals.get(&slot)?;
                    }
                    continue;
                }
                Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::MixedBox | Op::MixedClone => {
                    if inst.immediate.is_some() { return None; }
                    *values.get(inst.operands.first()?)?
                }
                Op::ConstI64 => {
                    let Some(Immediate::I64(value)) = inst.immediate else { return None; };
                    nodes.push(Node::Integer(value)); spans.push(inst.span); nodes.len() - 1
                }
                Op::ConstF64 => {
                    let Some(Immediate::F64(value)) = inst.immediate else { return None; };
                    nodes.push(Node::Float(value)); spans.push(inst.span); nodes.len() - 1
                }
                Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::MixedNumericBinop => {
                    let op = match inst.op {
                        Op::ICheckedAdd => MixedNumericOp::Add,
                        Op::ICheckedSub => MixedNumericOp::Sub,
                        Op::ICheckedMul => MixedNumericOp::Mul,
                        _ => match inst.immediate {
                            Some(Immediate::MixedNumericOp(op @ (MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) => op,
                            _ => return None,
                        },
                    };
                    if inst.operands.len() != 2 { return None; }
                    nodes.push(Node::Binary(op, *values.get(&inst.operands[0])?, *values.get(&inst.operands[1])?));
                    spans.push(inst.span);
                    nodes.len() - 1
                }
                _ => return None,
            };
            if spans[node].is_none() { spans[node] = inst.span; }
            values.insert(inst.result?, node);
        }
        let Some(Terminator::Return { value: Some(result) }) = function.blocks[0].terminator else { return None; };
        Some(Self { index: function.id.as_raw(), params: function.params.iter().map(|param| param.php_type.clone()).collect(),
            names: function.params.iter().map(|param| param.name.clone()).collect(),
            source_name: function.display_name().to_owned(), lexical_class: function.lexical_class.clone(),
            flags: function.flags.clone(),
            result: *values.get(&result)?, nodes, spans, reset })
    }

    /// Recognizes this original closure's private entry after intervening optimizer rounds.
    pub(super) fn reusable_tag(&self, function: &Function) -> Option<String> {
        let mut flags = self.flags.clone();
        flags.is_synthetic = true;
        if function.source_name.as_deref() != Some(self.source_name.as_str())
            || function.flags != flags || function.lexical_class != self.lexical_class
            || function.signature.is_some() || function.params.len() != self.params.len()
            || function.return_type != IrType::Heap(IrHeapKind::Mixed)
            || function.return_php_type != PhpType::Mixed
        { return None; }
        let tag: String = function.params.iter().zip(&self.params).map(|(actual, original)| {
            if actual.by_ref || actual.variadic
                || (*original != PhpType::Mixed && *original != actual.php_type)
            { return None; }
            match (&actual.ir_type, &actual.php_type) {
                (IrType::I64, PhpType::Int) => Some('i'),
                (IrType::F64, PhpType::Float) => Some('f'),
                _ => None,
            }
        }).collect::<Option<_>>()?;
        let prefix = format!("__eir_numeric_closure_{}_{}_", self.index, tag);
        function.name.strip_prefix(&prefix)?.parse::<usize>().ok()?;
        Some(tag)
    }

    /// Emits a scalar entry ABI and the same owned Mixed result expected by existing callers.
    pub(super) fn emit(&self, name: &str, types: &[PhpType]) -> Function {
        let mut function = Function::new(name.to_owned(), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        function.flags = self.flags.clone();
        function.flags.is_synthetic = true;
        function.source_name = Some(self.source_name.clone());
        function.lexical_class = self.lexical_class.clone();
        for (name, ty) in self.names.iter().zip(types) {
            let ir_type = if *ty == PhpType::Int { IrType::I64 } else { IrType::F64 };
            function.params.push(crate::ir::FunctionParam { name: name.clone(), ir_type, php_type: ty.clone(), by_ref: false, variadic: false });
            function.add_local(Some(name.clone()), ir_type, ty.clone(), LocalKind::PhpLocal);
        }
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        if self.reset { builder.emit(Op::ConcatReset, Vec::new(), None, IrType::Void, PhpType::Void, Ownership::NonHeap); }
        let mut values = Vec::new();
        let mut origins = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let value = match *node {
                Node::Parameter(index) => {
                    let ty = types[index].clone();
                    builder.emit_load_local(crate::ir::LocalSlotId::from_raw(index as u32),
                        if ty == PhpType::Int { IrType::I64 } else { IrType::F64 }, ty)
                }
                Node::Integer(value) => builder.emit_const_i64(value),
                Node::Float(value) => builder.emit_const_f64(value),
                Node::Binary(op, left, right) => emit_binary(&mut builder, op, values[left], values[right]),
            };
            if let crate::ir::ValueDef::Instruction { inst, .. } = builder.function().value(value).unwrap().def {
                origins.push((inst, self.spans[index]));
            }
            values.push(value);
        }
        let mut result = values[self.result];
        if builder.value_php_type(result) != PhpType::Mixed {
            result = builder.emit(Op::MixedBox, vec![result], None, IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed, Ownership::Owned).unwrap();
        }
        // Every expression node is emitted once; keep reused intermediates alive until
        // the final scalar result has been boxed, then retire each independent owner.
        for &value in &values {
            if value != result && builder.function().value(value).is_some_and(|value| value.ownership == Ownership::Owned) {
                builder.emit(Op::Release, vec![value], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
        }
        builder.terminate(Terminator::Return { value: Some(result) });
        for (inst, span) in origins { function.instructions[inst.as_raw() as usize].span = span; }
        function
    }
}

/// Selects checked integer or generic numeric arithmetic without changing either operand's value.
fn emit_binary(builder: &mut Builder<'_>, op: MixedNumericOp, left: ValueId, right: ValueId) -> ValueId {
    let left_type = builder.value_php_type(left);
    let right_type = builder.value_php_type(right);
    if (left_type == PhpType::Float || right_type == PhpType::Float)
        && matches!(left_type, PhpType::Int | PhpType::Float)
        && matches!(right_type, PhpType::Int | PhpType::Float)
    {
        let left = if left_type == PhpType::Int {
            builder.emit(Op::IToF, vec![left], None, IrType::F64, PhpType::Float, Ownership::NonHeap).unwrap()
        } else { left };
        let right = if right_type == PhpType::Int {
            builder.emit(Op::IToF, vec![right], None, IrType::F64, PhpType::Float, Ownership::NonHeap).unwrap()
        } else { right };
        let opcode = match op { MixedNumericOp::Add => Op::FAdd, MixedNumericOp::Sub => Op::FSub,
            MixedNumericOp::Mul => Op::FMul, _ => unreachable!() };
        return builder.emit(opcode, vec![left, right], None, IrType::F64, PhpType::Float, Ownership::NonHeap).unwrap();
    }
    let (opcode, immediate) = if builder.value_php_type(left) == PhpType::Int
        && builder.value_php_type(right) == PhpType::Int
    {
        (match op { MixedNumericOp::Add => Op::ICheckedAdd, MixedNumericOp::Sub => Op::ICheckedSub,
            MixedNumericOp::Mul => Op::ICheckedMul, _ => unreachable!() }, None)
    } else { (Op::MixedNumericBinop, Some(Immediate::MixedNumericOp(op))) };
    builder.emit(opcode, vec![left, right], immediate, IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, Ownership::Owned).unwrap()
}
