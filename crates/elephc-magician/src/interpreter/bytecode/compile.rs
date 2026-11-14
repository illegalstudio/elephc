//! Purpose:
//! Compiles the supported EvalIR slice to a flat opcode body.
//! Every rejected construct is a named gap so the tree walker keeps PHP's behavior.
//!
//! Called from:
//! - `crate::interpreter::bytecode::try_execute()` for a whole fragment.
//! - `crate::interpreter::statements::execute_stmt()` when a simple function is declared.
//!
//! Key details:
//! - Calls compile only for exact positional arity against functions admitted in this
//!   compilation. A builtin, a sibling outside that set, or a wrong arity is a gap.
//! - Function declarations become `DefineFn`. The body runs only after that instruction,
//!   matching Magician's rule that a declaration takes effect when it is reached.

use crate::eval_ir::{
    CompiledBody, CompiledCall, CompiledModule, EvalBinOp, EvalCastType, EvalConst, EvalExpr, EvalFunction,
    EvalProgram, EvalSourceLocation, EvalStmt, Op,
};
use crate::interpreter::static_var_names;
use std::collections::HashMap;
use std::sync::Arc;

/// A construct the bytecode slice refuses to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompileGap {
    what: &'static str,
}

impl CompileGap {
    /// Names one unsupported EvalIR construct.
    const fn new(what: &'static str) -> Self {
        Self { what }
    }

    /// Returns the variant or rule that kept this fragment on the tree walker.
    #[cfg(test)]
    pub(crate) const fn what(self) -> &'static str {
        self.what
    }
}

impl std::fmt::Display for CompileGap {
    /// Renders the gap as a stable diagnostic prefix plus the construct name.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "bytecode compile: unsupported {}", self.what)
    }
}

/// One function gathered before any body is compiled.
struct Decl {
    name: String,
    params: Vec<String>,
    body: Vec<EvalStmt>,
    location: Option<EvalSourceLocation>,
    children: Vec<String>,
}

/// Jump sites that a loop has not patched yet.
#[derive(Default)]
struct OpenLoop {
    continue_at: Option<u32>,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

/// Statement and expression compiler for one body.
struct Compiler<'a> {
    locals: Vec<String>,
    local_index: HashMap<String, u16>,
    strings: Vec<String>,
    string_index: HashMap<String, u16>,
    calls: Vec<CompiledCall>,
    ops: Vec<Op>,
    loops: Vec<OpenLoop>,
    shapes: &'a HashMap<String, usize>,
    defined_names: Vec<String>,
    next_defined: usize,
}

impl<'a> Compiler<'a> {
    /// Seeds parameter slots and the functions this body is allowed to define.
    fn new(
        params: &[String],
        defined: &[Arc<CompiledBody>],
        shapes: &'a HashMap<String, usize>,
    ) -> Result<Self, CompileGap> {
        if defined.len() > u16::MAX as usize {
            return Err(CompileGap::new("TooManyFunctions"));
        }
        let mut locals = Vec::new();
        let mut local_index = HashMap::new();
        for name in params {
            let index = u16::try_from(locals.len()).map_err(|_| CompileGap::new("TooManyLocals"))?;
            locals.push(name.clone());
            local_index.insert(name.clone(), index);
        }
        Ok(Self {
            locals,
            local_index,
            strings: Vec::new(),
            string_index: HashMap::new(),
            calls: Vec::new(),
            ops: Vec::new(),
            loops: Vec::new(),
            shapes,
            defined_names: defined.iter().map(|body| body.name.clone()).collect(),
            next_defined: 0,
        })
    }

    /// Appends one instruction, rejecting a body that cannot address its own jumps.
    fn emit(&mut self, op: Op) -> Result<(), CompileGap> {
        if self.ops.len() >= u32::MAX as usize {
            return Err(CompileGap::new("TooManyOps"));
        }
        self.ops.push(op);
        Ok(())
    }

    /// Returns the instruction index a jump can target.
    fn here(&self) -> u32 {
        self.ops.len() as u32
    }

    /// Fills a previously emitted jump with its absolute target.
    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.ops[at] {
            Op::Jump(slot) | Op::JumpIfFalse(slot) | Op::PrepareCall(_, slot) => *slot = target,
            _ => unreachable!("patch site is a jump"),
        }
    }

    /// Emits a forward unconditional jump and returns its patch index.
    fn jump(&mut self) -> Result<usize, CompileGap> {
        let at = self.ops.len();
        self.emit(Op::Jump(0))?;
        Ok(at)
    }

    /// Emits a forward conditional jump and returns its patch index.
    fn jump_if_false(&mut self) -> Result<usize, CompileGap> {
        let at = self.ops.len();
        self.emit(Op::JumpIfFalse(0))?;
        Ok(at)
    }

    /// Reserves a local slot for a name, reusing a parameter or earlier assignment.
    fn local(&mut self, name: &str) -> Result<u16, CompileGap> {
        if let Some(index) = self.local_index.get(name) {
            return Ok(*index);
        }
        let index = u16::try_from(self.locals.len()).map_err(|_| CompileGap::new("TooManyLocals"))?;
        self.locals.push(name.to_string());
        self.local_index.insert(name.to_string(), index);
        Ok(index)
    }

    /// Interns a string literal.
    fn intern(&mut self, value: &str) -> Result<u16, CompileGap> {
        if let Some(index) = self.string_index.get(value) {
            return Ok(*index);
        }
        let index = u16::try_from(self.strings.len()).map_err(|_| CompileGap::new("TooManyStrings"))?;
        self.strings.push(value.to_string());
        self.string_index.insert(value.to_string(), index);
        Ok(index)
    }

    /// Emits a break to the innermost open loop.
    fn emit_break(&mut self) -> Result<(), CompileGap> {
        if self.loops.is_empty() {
            return Err(CompileGap::new("Break"));
        }
        let at = self.ops.len();
        self.emit(Op::Jump(0))?;
        self.loops.last_mut().unwrap().breaks.push(at);
        Ok(())
    }

    /// Emits a continue to the innermost loop's continue point.
    fn emit_continue(&mut self) -> Result<(), CompileGap> {
        let Some(target) = self.loops.last().map(|loop_| loop_.continue_at) else {
            return Err(CompileGap::new("Continue"));
        };
        if let Some(target) = target {
            self.emit(Op::Jump(target))?;
            return Ok(());
        }
        let at = self.ops.len();
        self.emit(Op::Jump(0))?;
        self.loops.last_mut().unwrap().continues.push(at);
        Ok(())
    }

    /// Records the continue target and patches continues emitted before it.
    fn bind_continue(&mut self, target: u32) {
        let sites = {
            let loop_ = self.loops.last_mut().unwrap();
            loop_.continue_at = Some(target);
            std::mem::take(&mut loop_.continues)
        };
        for at in sites {
            self.patch(at, target);
        }
    }

    /// Patches breaks to the first instruction after the loop.
    fn close_loop(&mut self, end: u32) {
        let loop_ = self.loops.pop().unwrap();
        for at in loop_.breaks {
            self.patch(at, end);
        }
    }

    /// Compiles a statement list in source order.
    fn compile_stmts(&mut self, stmts: &[EvalStmt]) -> Result<(), CompileGap> {
        for stmt in stmts {
            self.emit(Op::StatementBoundary)?;
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    /// Compiles one statement or names the variant the slice does not run.
    fn compile_stmt(&mut self, stmt: &EvalStmt) -> Result<(), CompileGap> {
        match stmt {
            EvalStmt::Break => self.emit_break(),
            EvalStmt::Continue => self.emit_continue(),
            EvalStmt::DoWhile { body, condition } => {
                let start = self.here();
                self.loops.push(OpenLoop::default());
                self.compile_stmts(body)?;
                self.bind_continue(self.here());
                self.compile_expr(condition)?;
                let skip = self.jump_if_false()?;
                self.loops.last_mut().unwrap().breaks.push(skip);
                self.emit(Op::Jump(start))?;
                self.close_loop(self.here());
                Ok(())
            }
            EvalStmt::Echo(expr) => {
                self.compile_expr(expr)?;
                self.emit(Op::Echo)
            }
            EvalStmt::For { init, condition, update, body } => {
                self.compile_stmts(init)?;
                let start = self.here();
                self.loops.push(OpenLoop::default());
                if let Some(condition) = condition {
                    self.compile_expr(condition)?;
                    let skip = self.jump_if_false()?;
                    self.loops.last_mut().unwrap().breaks.push(skip);
                }
                self.compile_stmts(body)?;
                self.bind_continue(self.here());
                self.compile_stmts(update)?;
                self.emit(Op::Jump(start))?;
                self.close_loop(self.here());
                Ok(())
            }
            EvalStmt::FunctionDecl { name, .. } => {
                let Some(expected) = self.defined_names.get(self.next_defined) else {
                    return Err(CompileGap::new("FunctionDecl"));
                };
                if !expected.eq_ignore_ascii_case(name) {
                    return Err(CompileGap::new("FunctionDecl"));
                }
                let index = u16::try_from(self.next_defined).map_err(|_| CompileGap::new("TooManyFunctions"))?;
                self.next_defined += 1;
                self.emit(Op::DefineFn(index))
            }
            EvalStmt::If { condition, then_branch, else_branch } => {
                self.compile_expr(condition)?;
                let else_jump = self.jump_if_false()?;
                self.compile_stmts(then_branch)?;
                let end_jump = self.jump()?;
                self.patch(else_jump, self.here());
                self.compile_stmts(else_branch)?;
                self.patch(end_jump, self.here());
                Ok(())
            }
            EvalStmt::Return(expr) => match expr {
                Some(expr) => {
                    self.compile_expr(expr)?;
                    self.emit(Op::Return)
                }
                None => self.emit(Op::ReturnVoid),
            },
            EvalStmt::StoreVar { name, value } => {
                self.compile_expr(value)?;
                let slot = self.local(name)?;
                self.emit(Op::Store(slot))
            }
            EvalStmt::IncDecVar { name, increment } => {
                let slot = self.local(name)?;
                self.emit(Op::IncDec(slot, *increment))
            }
            EvalStmt::While { condition, body } => {
                let start = self.here();
                self.loops.push(OpenLoop { continue_at: Some(start), ..OpenLoop::default() });
                self.compile_expr(condition)?;
                let skip = self.jump_if_false()?;
                self.loops.last_mut().unwrap().breaks.push(skip);
                self.compile_stmts(body)?;
                self.emit(Op::Jump(start))?;
                self.close_loop(self.here());
                Ok(())
            }
            EvalStmt::Expr(expr) => self.compile_discarded(expr),
            EvalStmt::GenericDecl(_) => Err(CompileGap::new("GenericDecl")),
            EvalStmt::TypedVar { .. } => Err(CompileGap::new("TypedVar")),
            EvalStmt::ArrayAppendVar { .. } => Err(CompileGap::new("ArrayAppendVar")),
            EvalStmt::ArraySetVar { .. } => Err(CompileGap::new("ArraySetVar")),
            EvalStmt::DimensionWrite { .. } => Err(CompileGap::new("DimensionWrite")),
            EvalStmt::ClassDecl(_) => Err(CompileGap::new("ClassDecl")),
            EvalStmt::EnumDecl(_) => Err(CompileGap::new("EnumDecl")),
            EvalStmt::InterfaceDecl(_) => Err(CompileGap::new("InterfaceDecl")),
            EvalStmt::TraitDecl(_) => Err(CompileGap::new("TraitDecl")),
            EvalStmt::Foreach { .. } => Err(CompileGap::new("Foreach")),
            EvalStmt::Global { .. } => Err(CompileGap::new("Global")),
            EvalStmt::ReferenceAssign { .. } => Err(CompileGap::new("ReferenceAssign")),
            EvalStmt::PropertyReferenceBind { .. } => Err(CompileGap::new("PropertyReferenceBind")),
            EvalStmt::DynamicPropertyReferenceBind { .. } => Err(CompileGap::new("DynamicPropertyReferenceBind")),
            EvalStmt::DynamicPropertySet { .. } => Err(CompileGap::new("DynamicPropertySet")),
            EvalStmt::DynamicPropertyArrayAppend { .. } => Err(CompileGap::new("DynamicPropertyArrayAppend")),
            EvalStmt::DynamicPropertyArraySet { .. } => Err(CompileGap::new("DynamicPropertyArraySet")),
            EvalStmt::DynamicPropertyCompoundAssign { .. } => Err(CompileGap::new("DynamicPropertyCompoundAssign")),
            EvalStmt::DynamicPropertyIncDec { .. } => Err(CompileGap::new("DynamicPropertyIncDec")),
            EvalStmt::PropertySet { .. } => Err(CompileGap::new("PropertySet")),
            EvalStmt::PropertyArrayAppend { .. } => Err(CompileGap::new("PropertyArrayAppend")),
            EvalStmt::PropertyArraySet { .. } => Err(CompileGap::new("PropertyArraySet")),
            EvalStmt::PropertyCompoundAssign { .. } => Err(CompileGap::new("PropertyCompoundAssign")),
            EvalStmt::PropertyIncDec { .. } => Err(CompileGap::new("PropertyIncDec")),
            EvalStmt::StaticPropertySet { .. } => Err(CompileGap::new("StaticPropertySet")),
            EvalStmt::StaticPropertyReferenceBind { .. } => Err(CompileGap::new("StaticPropertyReferenceBind")),
            EvalStmt::StaticPropertyArrayAppend { .. } => Err(CompileGap::new("StaticPropertyArrayAppend")),
            EvalStmt::StaticPropertyArraySet { .. } => Err(CompileGap::new("StaticPropertyArraySet")),
            EvalStmt::StaticPropertyIncDec { .. } => Err(CompileGap::new("StaticPropertyIncDec")),
            EvalStmt::DynamicStaticPropertySet { .. } => Err(CompileGap::new("DynamicStaticPropertySet")),
            EvalStmt::DynamicStaticPropertyReferenceBind { .. } => Err(CompileGap::new("DynamicStaticPropertyReferenceBind")),
            EvalStmt::DynamicStaticPropertyArrayAppend { .. } => Err(CompileGap::new("DynamicStaticPropertyArrayAppend")),
            EvalStmt::DynamicStaticPropertyArraySet { .. } => Err(CompileGap::new("DynamicStaticPropertyArraySet")),
            EvalStmt::DynamicStaticPropertyIncDec { .. } => Err(CompileGap::new("DynamicStaticPropertyIncDec")),
            EvalStmt::DynamicStaticPropertyNameSet { .. } => Err(CompileGap::new("DynamicStaticPropertyNameSet")),
            EvalStmt::DynamicStaticPropertyNameReferenceBind { .. } => Err(CompileGap::new("DynamicStaticPropertyNameReferenceBind")),
            EvalStmt::DynamicStaticPropertyNameArrayAppend { .. } => Err(CompileGap::new("DynamicStaticPropertyNameArrayAppend")),
            EvalStmt::DynamicStaticPropertyNameArraySet { .. } => Err(CompileGap::new("DynamicStaticPropertyNameArraySet")),
            EvalStmt::DynamicStaticPropertyNameIncDec { .. } => Err(CompileGap::new("DynamicStaticPropertyNameIncDec")),
            EvalStmt::StaticVar { .. } => Err(CompileGap::new("StaticVar")),
            EvalStmt::Switch { .. } => Err(CompileGap::new("Switch")),
            EvalStmt::Throw(_) => Err(CompileGap::new("Throw")),
            EvalStmt::Try { .. } => Err(CompileGap::new("Try")),
            EvalStmt::UnsetArrayElement { .. } => Err(CompileGap::new("UnsetArrayElement")),
            EvalStmt::UnsetProperty { .. } => Err(CompileGap::new("UnsetProperty")),
            EvalStmt::UnsetDynamicProperty { .. } => Err(CompileGap::new("UnsetDynamicProperty")),
            EvalStmt::UnsetStaticProperty { .. } => Err(CompileGap::new("UnsetStaticProperty")),
            EvalStmt::UnsetDynamicStaticProperty { .. } => Err(CompileGap::new("UnsetDynamicStaticProperty")),
            EvalStmt::UnsetDynamicStaticPropertyName { .. } => Err(CompileGap::new("UnsetDynamicStaticPropertyName")),
            EvalStmt::UnsetVar { .. } => Err(CompileGap::new("UnsetVar")),
            EvalStmt::GcCollect => Err(CompileGap::new("GcCollect")),
        }
    }

    /// Compiles an expression whose value is discarded, without retaining an assignment.
    fn compile_discarded(&mut self, expr: &EvalExpr) -> Result<(), CompileGap> {
        if let EvalExpr::Call { name, args } = expr {
            let index = u16::try_from(self.calls.len()).map_err(|_| CompileGap::new("TooManyCalls"))?;
            self.compile_call(name, args)?;
            return self.emit(Op::PopDiscardedCall(index));
        }
        if let EvalExpr::Assign { name, value } = expr {
            self.compile_expr(value)?;
            let slot = self.local(name)?;
            return self.emit(Op::Store(slot));
        }
        self.compile_expr(expr)?;
        self.emit(Op::Pop)
    }

    /// Compiles an expression that leaves one value on the stack.
    fn compile_expr(&mut self, expr: &EvalExpr) -> Result<(), CompileGap> {
        match expr {
            EvalExpr::Call { name, args } => self.compile_call(name, args),
            EvalExpr::Cast { target, expr } => {
                self.compile_expr(expr)?;
                match target {
                    EvalCastType::Array => Err(CompileGap::new("CastArray")),
                    EvalCastType::Int => self.emit(Op::CastInt),
                    EvalCastType::Float => self.emit(Op::CastFloat),
                    EvalCastType::String => self.emit(Op::CastString),
                    EvalCastType::Bool => self.emit(Op::CastBool),
                }
            }
            EvalExpr::Const(constant) => match constant {
                EvalConst::Null => self.emit(Op::Null),
                EvalConst::Bool(value) => self.emit(Op::Bool(*value)),
                EvalConst::Int(value) => self.emit(Op::Int(*value)),
                EvalConst::Float(value) => self.emit(Op::Float(value.to_bits())),
                EvalConst::String(value) => {
                    let index = self.intern(value)?;
                    self.emit(Op::String(index))
                }
                EvalConst::Bytes(_) => Err(CompileGap::new("Bytes")),
            },
            EvalExpr::LoadVar(name) => {
                let slot = self.local(name)?;
                self.emit(Op::Load(slot))
            }
            EvalExpr::Assign { name, value } => {
                self.compile_expr(value)?;
                let slot = self.local(name)?;
                self.emit(Op::Store(slot))?;
                self.emit(Op::Load(slot))
            }
            EvalExpr::Print(expr) => {
                self.compile_expr(expr)?;
                self.emit(Op::Print)
            }
            EvalExpr::Ternary { condition, then_branch, else_branch } => {
                self.compile_expr(condition)?;
                if let Some(then_branch) = then_branch {
                    let else_jump = self.jump_if_false()?;
                    self.compile_expr(then_branch)?;
                    let end_jump = self.jump()?;
                    self.patch(else_jump, self.here());
                    self.compile_expr(else_branch)?;
                    self.patch(end_jump, self.here());
                } else {
                    self.emit(Op::Dup)?;
                    let else_jump = self.jump_if_false()?;
                    let end_jump = self.jump()?;
                    self.patch(else_jump, self.here());
                    self.emit(Op::Pop)?;
                    self.compile_expr(else_branch)?;
                    self.patch(end_jump, self.here());
                }
                Ok(())
            }
            EvalExpr::Unary { op, expr } => {
                self.compile_expr(expr)?;
                self.emit(Op::Unary(*op))
            }
            EvalExpr::Binary { op, left, right } => self.compile_binary(*op, left, right),
            EvalExpr::DeferredDefaultError(_) => Err(CompileGap::new("DeferredDefaultError")),
            EvalExpr::Array(_) => Err(CompileGap::new("Array")),
            EvalExpr::ArrayGet { .. } => Err(CompileGap::new("ArrayGet")),
            EvalExpr::ConstFetch(_) => Err(CompileGap::new("ConstFetch")),
            EvalExpr::Closure { .. } => Err(CompileGap::new("Closure")),
            EvalExpr::FunctionCallable { .. } => Err(CompileGap::new("FunctionCallable")),
            EvalExpr::InvokableCallable { .. } => Err(CompileGap::new("InvokableCallable")),
            EvalExpr::MethodCallable { .. } => Err(CompileGap::new("MethodCallable")),
            EvalExpr::StaticMethodCallable { .. } => Err(CompileGap::new("StaticMethodCallable")),
            EvalExpr::DynamicStaticMethodCallable { .. } => Err(CompileGap::new("DynamicStaticMethodCallable")),
            EvalExpr::DynamicCall { .. } => Err(CompileGap::new("DynamicCall")),
            EvalExpr::DynamicMethodCall { .. } => Err(CompileGap::new("DynamicMethodCall")),
            EvalExpr::DynamicNewObject { .. } => Err(CompileGap::new("DynamicNewObject")),
            EvalExpr::DynamicPropertyGet { .. } => Err(CompileGap::new("DynamicPropertyGet")),
            EvalExpr::DynamicStaticMethodCall { .. } => Err(CompileGap::new("DynamicStaticMethodCall")),
            EvalExpr::DynamicStaticPropertyGet { .. } => Err(CompileGap::new("DynamicStaticPropertyGet")),
            EvalExpr::DynamicStaticPropertyNameGet { .. } => Err(CompileGap::new("DynamicStaticPropertyNameGet")),
            EvalExpr::DynamicClassConstantFetch { .. } => Err(CompileGap::new("DynamicClassConstantFetch")),
            EvalExpr::DynamicClassConstantNameFetch { .. } => Err(CompileGap::new("DynamicClassConstantNameFetch")),
            EvalExpr::DynamicClassNameFetch { .. } => Err(CompileGap::new("DynamicClassNameFetch")),
            EvalExpr::Include { .. } => Err(CompileGap::new("Include")),
            EvalExpr::InstanceOf { .. } => Err(CompileGap::new("InstanceOf")),
            EvalExpr::PropertyCompoundAssign { .. } => Err(CompileGap::new("PropertyCompoundAssign")),
            EvalExpr::PropertyIncDec { .. } => Err(CompileGap::new("PropertyIncDec")),
            EvalExpr::Match { .. } => Err(CompileGap::new("Match")),
            EvalExpr::Clone(_) => Err(CompileGap::new("Clone")),
            EvalExpr::NamespacedCall { .. } => Err(CompileGap::new("NamespacedCall")),
            EvalExpr::NamespacedConstFetch { .. } => Err(CompileGap::new("NamespacedConstFetch")),
            EvalExpr::MethodCall { .. } => Err(CompileGap::new("MethodCall")),
            EvalExpr::NullsafeMethodCall { .. } => Err(CompileGap::new("NullsafeMethodCall")),
            EvalExpr::NullsafeDynamicMethodCall { .. } => Err(CompileGap::new("NullsafeDynamicMethodCall")),
            EvalExpr::Magic(_) => Err(CompileGap::new("Magic")),
            EvalExpr::NewObject { .. } => Err(CompileGap::new("NewObject")),
            EvalExpr::NewAnonymousClass { .. } => Err(CompileGap::new("NewAnonymousClass")),
            EvalExpr::StaticMethodCall { .. } => Err(CompileGap::new("StaticMethodCall")),
            EvalExpr::StaticPropertyGet { .. } => Err(CompileGap::new("StaticPropertyGet")),
            EvalExpr::ClassConstantFetch { .. } => Err(CompileGap::new("ClassConstantFetch")),
            EvalExpr::ClassNameFetch { .. } => Err(CompileGap::new("ClassNameFetch")),
            EvalExpr::NullCoalesce { .. } => Err(CompileGap::new("NullCoalesce")),
            EvalExpr::NullsafePropertyGet { .. } => Err(CompileGap::new("NullsafePropertyGet")),
            EvalExpr::NullsafeDynamicPropertyGet { .. } => Err(CompileGap::new("NullsafeDynamicPropertyGet")),
            EvalExpr::PropertyGet { .. } => Err(CompileGap::new("PropertyGet")),
        }
    }

    /// Compiles a positional call to a function admitted by this compilation.
    fn compile_call(&mut self, name: &str, args: &[crate::eval_ir::EvalCallArg]) -> Result<(), CompileGap> {
        for arg in args {
            if arg.name().is_some() {
                return Err(CompileGap::new("NamedCall"));
            }
            if arg.is_spread() {
                return Err(CompileGap::new("SpreadCall"));
            }
        }
        let key = name.to_ascii_lowercase();
        let Some(params) = self.shapes.get(&key).copied() else {
            return Err(CompileGap::new("UnresolvedCall"));
        };
        if params != args.len() {
            return Err(CompileGap::new("CallArity"));
        }
        let argc = u16::try_from(args.len()).map_err(|_| CompileGap::new("CallArity"))?;
        let index = u16::try_from(self.calls.len()).map_err(|_| CompileGap::new("TooManyCalls"))?;
        self.calls.push(CompiledCall { name: name.to_string(), args: args.to_vec() });
        let guard = self.ops.len();
        self.emit(Op::PrepareCall(index, 0))?;
        for arg in args {
            self.compile_expr(arg.value())?;
        }
        self.emit(Op::Call(argc))?;
        self.patch(guard, self.here());
        Ok(())
    }

    /// Compiles a binary operator, using jumps for the short-circuit forms.
    fn compile_binary(&mut self, op: EvalBinOp, left: &EvalExpr, right: &EvalExpr) -> Result<(), CompileGap> {
        if op == EvalBinOp::LogicalAnd {
            self.compile_expr(left)?;
            let fail_left = self.jump_if_false()?;
            self.compile_expr(right)?;
            let fail_right = self.jump_if_false()?;
            self.emit(Op::Bool(true))?;
            let done = self.jump()?;
            let false_at = self.here();
            self.patch(fail_left, false_at);
            self.patch(fail_right, false_at);
            self.emit(Op::Bool(false))?;
            self.patch(done, self.here());
            return Ok(());
        }
        if op == EvalBinOp::LogicalOr {
            self.compile_expr(left)?;
            let right_at = self.jump_if_false()?;
            self.emit(Op::Bool(true))?;
            let done_true = self.jump()?;
            self.patch(right_at, self.here());
            self.compile_expr(right)?;
            let fail = self.jump_if_false()?;
            self.emit(Op::Bool(true))?;
            let done = self.jump()?;
            let false_at = self.here();
            self.patch(fail, false_at);
            self.emit(Op::Bool(false))?;
            let end = self.here();
            self.patch(done_true, end);
            self.patch(done, end);
            return Ok(());
        }
        self.compile_expr(left)?;
        self.compile_expr(right)?;
        self.emit(Op::Binary(op))
    }

    /// Builds the body after a trailing void return so falling off the end yields null.
    fn finish(
        mut self,
        name: &str,
        params: &[String],
        fast: bool,
        defined: Vec<Arc<CompiledBody>>,
        source_body: Vec<EvalStmt>,
        source_location: Option<EvalSourceLocation>,
    ) -> Result<Arc<CompiledBody>, CompileGap> {
        self.emit(Op::ReturnVoid)?;
        Ok(Arc::new(CompiledBody {
            name: name.to_string(),
            params: params.to_vec(),
            locals: self.locals,
            strings: self.strings,
            calls: self.calls,
            ops: self.ops,
            fast_positional: fast,
            defined,
            source_body,
            source_location,
        }))
    }
}

/// Returns the admission failure for a function the positional slice cannot enter directly.
fn admission_gap(
    attributes: &[crate::eval_ir::EvalAttribute],
    parameter_attributes: &[Vec<crate::eval_ir::EvalAttribute>],
    parameter_types: &[Option<crate::eval_ir::EvalParameterType>],
    parameter_defaults: &[Option<EvalExpr>],
    parameter_is_by_ref: &[bool],
    parameter_is_variadic: &[bool],
    return_type: Option<&crate::eval_ir::EvalParameterType>,
    body: &[EvalStmt],
) -> Option<&'static str> {
    if !attributes.is_empty() {
        return Some("FunctionAttributes");
    }
    if parameter_attributes.iter().any(|attributes| !attributes.is_empty()) {
        return Some("ParameterAttributes");
    }
    if parameter_types.iter().any(Option::is_some) {
        return Some("ParameterType");
    }
    if parameter_defaults.iter().any(Option::is_some) {
        return Some("ParameterDefault");
    }
    if parameter_is_by_ref.iter().any(|flag| *flag) {
        return Some("ByRefParameter");
    }
    if parameter_is_variadic.iter().any(|flag| *flag) {
        return Some("VariadicParameter");
    }
    if return_type.is_some() {
        return Some("ReturnType");
    }
    if !static_var_names(body).is_empty() {
        return Some("StaticVar");
    }
    None
}

/// Collects admitted functions declared in the containers this compiler enters.
fn collect(
    stmts: &[EvalStmt],
    children: &mut Vec<String>,
    decls: &mut HashMap<String, Decl>,
) -> Result<(), CompileGap> {
    for stmt in stmts {
        match stmt {
            EvalStmt::If { then_branch, else_branch, .. } => {
                collect(then_branch, children, decls)?;
                collect(else_branch, children, decls)?;
            }
            EvalStmt::While { body, .. } | EvalStmt::DoWhile { body, .. } => {
                collect(body, children, decls)?;
            }
            EvalStmt::For { init, body, update, .. } => {
                collect(init, children, decls)?;
                collect(body, children, decls)?;
                collect(update, children, decls)?;
            }
            EvalStmt::FunctionDecl {
                name,
                source_location,
                attributes,
                params,
                parameter_attributes,
                parameter_types,
                parameter_defaults,
                parameter_is_by_ref,
                parameter_is_variadic,
                return_type,
                body,
            } => {
                if let Some(what) = admission_gap(
                    attributes,
                    parameter_attributes,
                    parameter_types,
                    parameter_defaults,
                    parameter_is_by_ref,
                    parameter_is_variadic,
                    return_type.as_ref(),
                    body,
                ) {
                    return Err(CompileGap::new(what));
                }
                let key = name.to_ascii_lowercase();
                if decls.contains_key(&key) {
                    return Err(CompileGap::new("DuplicateFunction"));
                }
                decls.insert(key.clone(), Decl {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    location: *source_location,
                    children: Vec::new(),
                });
                let mut nested = Vec::new();
                collect(body, &mut nested, decls)?;
                decls.get_mut(&key).unwrap().children = nested;
                children.push(key);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Compiles one gathered function after its nested functions.
fn compile_key(
    key: &str,
    decls: &HashMap<String, Decl>,
    shapes: &HashMap<String, usize>,
    memo: &mut HashMap<String, Arc<CompiledBody>>,
) -> Result<Arc<CompiledBody>, CompileGap> {
    if let Some(existing) = memo.get(key) {
        return Ok(Arc::clone(existing));
    }
    let decl = decls.get(key).ok_or(CompileGap::new("FunctionDecl"))?;
    let children = decl.children.clone();
    let name = decl.name.clone();
    let params = decl.params.clone();
    let body = decl.body.clone();
    let location = decl.location;
    let mut defined = Vec::new();
    for child in &children {
        defined.push(compile_key(child, decls, shapes, memo)?);
    }
    let compiled = compile_body(&name, &params, &body, &defined, shapes, true, location, body.clone())?;
    memo.insert(key.to_string(), Arc::clone(&compiled));
    Ok(compiled)
}

/// Compiles one body against a fixed set of callable function shapes.
fn compile_body(
    name: &str,
    params: &[String],
    statements: &[EvalStmt],
    defined: &[Arc<CompiledBody>],
    shapes: &HashMap<String, usize>,
    fast: bool,
    source_location: Option<EvalSourceLocation>,
    source_body: Vec<EvalStmt>,
) -> Result<Arc<CompiledBody>, CompileGap> {
    let mut compiler = Compiler::new(params, defined, shapes)?;
    compiler.compile_stmts(statements)?;
    compiler.finish(name, params, fast, defined.to_vec(), source_body, source_location)
}

/// Compiles a fragment, or names the first construct the slice will not run.
pub(in crate::interpreter) fn compile_program(program: &EvalProgram) -> Result<Arc<CompiledModule>, CompileGap> {
    if program.native_generic_syntax() {
        return Err(CompileGap::new("GenericDecl"));
    }
    if !program.cli_superglobals().is_empty() {
        return Err(CompileGap::new("CliSuperglobal"));
    }
    let mut top = Vec::new();
    let mut decls = HashMap::new();
    collect(program.statements(), &mut top, &mut decls)?;
    let mut shapes = HashMap::new();
    for (key, decl) in &decls {
        shapes.insert(key.clone(), decl.params.len());
    }
    let mut memo = HashMap::new();
    let mut defined = Vec::new();
    for key in &top {
        defined.push(compile_key(key, &decls, &shapes, &mut memo)?);
    }
    let main = compile_body("", &[], program.statements(), &defined, &shapes, false, None, Vec::new())?;
    Ok(Arc::new(CompiledModule { main }))
}

/// Compiles one function and the simple functions nested inside it.
fn compile_attached(function: &EvalFunction) -> Result<Arc<CompiledBody>, CompileGap> {
    if let Some(what) = admission_gap(
        function.attributes(),
        function.parameter_attributes(),
        function.parameter_types(),
        function.parameter_defaults(),
        function.parameter_is_by_ref(),
        function.parameter_is_variadic(),
        function.return_type(),
        function.body(),
    ) {
        return Err(CompileGap::new(what));
    }
    let key = function.name().to_ascii_lowercase();
    let mut nested = Vec::new();
    let mut decls = HashMap::new();
    collect(function.body(), &mut nested, &mut decls)?;
    if decls.contains_key(&key) {
        return Err(CompileGap::new("DuplicateFunction"));
    }
    let mut shapes = HashMap::new();
    shapes.insert(key, function.params().len());
    for (name, decl) in &decls {
        shapes.insert(name.clone(), decl.params.len());
    }
    let mut memo = HashMap::new();
    let mut defined = Vec::new();
    for child in &nested {
        defined.push(compile_key(child, &decls, &shapes, &mut memo)?);
    }
    compile_body(
        function.name(),
        function.params(),
        function.body(),
        &defined,
        &shapes,
        true,
        function.source_location(),
        function.body().to_vec(),
    )
}

/// Attaches a compiled body when this function and its nested functions fit the slice.
pub(in crate::interpreter) fn attach_simple_body(function: &mut EvalFunction) {
    if let Ok(body) = compile_attached(function) {
        function.set_bytecode(body);
    }
}
