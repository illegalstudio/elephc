//! Purpose:
//! Visits eager operands that do not need a specialized conditional-effects join.
//!
//! Called from:
//! - The expression effects walker and contextual builtin callback argument evaluation.
//!
//! Key details:
//! - Closure bodies remain deferred; creating a bound callable still evaluates its receiver.
//! - Exhaustive coverage forces new AST variants to choose an effects-walk strategy.

use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget};
use crate::types::TypeEnv;

use super::super::super::Checker;

impl Checker {
    /// Keeps callback literals available for contextual typing while visiting other argument effects.
    pub(super) fn infer_contextual_callback_effects(&mut self, arg: &Expr, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &arg.kind {
            ExprKind::Closure { .. } => Ok(()),
            ExprKind::FirstClassCallable(target) => self.infer_callable_receiver_effects(target, env),
            ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) =>
                self.infer_contextual_callback_effects(value, env),
            _ => self.infer_type_with_assignment_effects(arg, env).map(|_| ()),
        }
    }

    /// Evaluates the bound object at callable creation, without checking its contextual signature.
    fn infer_callable_receiver_effects(&mut self, target: &CallableTarget, env: &mut TypeEnv) -> Result<(), CompileError> {
        match target {
            CallableTarget::Method { object, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
            }
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => {},
        }
        Ok(())
    }

    /// Visits remaining eager children once, before the parent performs its pure type validation.
    pub(super) fn infer_fallback_operand_effects(&mut self, expr: &Expr, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &expr.kind {
            ExprKind::InstanceOf { value, target } => {
                self.infer_type_with_assignment_effects(value, env)?;
                if let InstanceOfTarget::Expr(target) = target {
                    self.infer_type_with_assignment_effects(target, env)?;
                }
            }
            ExprKind::NewDynamic { name_expr, args }
            | ExprKind::NewDynamicObject { class_name: name_expr, args, .. } => {
                self.infer_type_with_assignment_effects(name_expr, env)?;
                self.infer_constructor_operand_effects(args, env)?;
            }
            ExprKind::NewGeneric { args, .. } => self.infer_constructor_operand_effects(args, env)?,
            ExprKind::Clone(value) | ExprKind::YieldFrom(value)
            | ExprKind::ObjectClassName { object: value }
            | ExprKind::IncludeValue { path: value, .. } => {
                self.infer_type_with_assignment_effects(value, env)?;
            }
            ExprKind::Yield { key, value } => {
                if let Some(key) = key { self.infer_type_with_assignment_effects(key, env)?; }
                if let Some(value) = value { self.infer_type_with_assignment_effects(value, env)?; }
            }
            ExprKind::FirstClassCallable(target) => self.infer_callable_receiver_effects(target, env)?,
            // Leaf values and compile-time receivers have no evaluated children. Every other
            // expression below has a dedicated arm in the main effects walker.
            ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_) | ExprKind::BoolLiteral(_) | ExprKind::Null
            | ExprKind::ConstRef(_) | ExprKind::This | ExprKind::MagicConstant(_)
            | ExprKind::ClassConstant { .. } | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ScopedConstantAccess { .. }
            | ExprKind::BinaryOp { .. } | ExprKind::Negate(_) | ExprKind::Not(_)
            | ExprKind::BitNot(_) | ExprKind::Throw(_) | ExprKind::ErrorSuppress(_)
            | ExprKind::Print(_) | ExprKind::NullCoalesce { .. } | ExprKind::Pipe { .. }
            | ExprKind::Assignment { .. } | ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_) | ExprKind::PostDecrement(_) | ExprKind::FunctionCall { .. }
            | ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_)
            | ExprKind::Match { .. } | ExprKind::ArrayAccess { .. } | ExprKind::Ternary { .. }
            | ExprKind::ShortTernary { .. } | ExprKind::Cast { .. } | ExprKind::Closure { .. }
            | ExprKind::NamedArg { .. } | ExprKind::Spread(_) | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. } | ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. }
            | ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafePropertyAccess { .. } | ExprKind::NullsafeDynamicPropertyAccess { .. }
            | ExprKind::MethodCall { .. } | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::NullsafeDynamicMethodCall { .. } | ExprKind::StaticMethodCall { .. }
            | ExprKind::PtrCast { .. } | ExprKind::BufferNew { .. } => {},
        }
        Ok(())
    }

    /// Preserves named/spread source order for every constructor form.
    fn infer_constructor_operand_effects(&mut self, args: &[Expr], env: &mut TypeEnv) -> Result<(), CompileError> {
        for arg in &crate::types::call_args::expand_static_assoc_spread_args(args) {
            self.infer_type_with_assignment_effects(arg, env)?;
        }
        Ok(())
    }
}
