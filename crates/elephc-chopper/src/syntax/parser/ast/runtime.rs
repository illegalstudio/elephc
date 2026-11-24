//! Purpose:
//! Retains source-level runtime expressions before native desugaring.
//!
//! Called from:
//! - The shared parser in runtime fragment mode and the Magician AST lowering.
//!
//! Key details:
//! - Computed members keep receiver and member evaluation separate.

use crate::syntax::names::Name;

use super::{BinOp, Expr, StaticReceiver, TypeExpr};

/// Runtime syntax that requires evaluation rules distinct from compiler desugaring.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeExpr {
    UnaryPlus(Expr),
    ObjectMemberCall {
        object: Expr,
        member: Expr,
        arguments: MemberArguments,
        nullsafe: bool,
    },
    StaticMember {
        class: RuntimeClassReceiver,
        member: RuntimeClassMember,
    },
    GenericFunction {
        name: Name,
        type_args: Vec<TypeExpr>,
        arguments: MemberArguments,
    },
    GenericObjectMethod {
        object: Expr,
        method: String,
        type_args: Vec<TypeExpr>,
        arguments: MemberArguments,
        nullsafe: bool,
    },
    Write {
        target: Expr,
        operation: RuntimeWrite,
    },
}

/// A statement-position write before target capture or update desugaring.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeWrite {
    Assign { op: Option<BinOp>, value: Expr },
    Append(Expr),
    BindReference(String),
    IncDec { increment: bool, prefix: bool },
}

/// An invocation or first-class callable formed from a member access.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberArguments {
    Call(Vec<Expr>),
    Callable,
}

/// Class selection for static runtime syntax.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeClassReceiver {
    Named(StaticReceiver),
    Computed(Expr),
}

/// A member selected by the static access operator.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeClassMember {
    ClassName,
    Property(String),
    ComputedProperty(Expr),
    Constant(String),
    ComputedConstant(Expr),
    Method { name: Expr, arguments: MemberArguments },
    GenericMethod { method: String, type_args: Vec<TypeExpr>, arguments: MemberArguments },
}

impl RuntimeExpr {
    /// Returns operand expressions in source evaluation order.
    pub fn exprs(&self) -> std::vec::IntoIter<&Expr> {
        let mut operands = Vec::new();
        let arguments = match self {
            Self::GenericFunction { arguments, .. } => Some(arguments),
            Self::GenericObjectMethod { object, arguments, .. } => {
                operands.push(object);
                Some(arguments)
            }
            Self::UnaryPlus(value) => {
                operands.push(value);
                None
            }
            Self::ObjectMemberCall { object, member, arguments, .. } => {
                operands.extend([object, member]);
                Some(arguments)
            }
            Self::StaticMember { class, member } => {
                if let RuntimeClassReceiver::Computed(value) = class {
                    operands.push(value);
                }
                match member {
                    RuntimeClassMember::ComputedProperty(value) | RuntimeClassMember::ComputedConstant(value) => {
                        operands.push(value);
                        None
                    }
                    RuntimeClassMember::Method { name, arguments } => {
                        operands.push(name);
                        Some(arguments)
                    }
                    RuntimeClassMember::GenericMethod { arguments, .. } => Some(arguments),
                    _ => None,
                }
            }
            Self::Write { target, operation } => {
                operands.push(target);
                if let RuntimeWrite::Assign { value, .. } | RuntimeWrite::Append(value) = operation {
                    operands.push(value);
                }
                None
            }
        };
        if let Some(MemberArguments::Call(args)) = arguments {
            operands.extend(args);
        }
        operands.into_iter()
    }

    /// Returns mutable operand expressions in source evaluation order.
    pub fn exprs_mut(&mut self) -> std::vec::IntoIter<&mut Expr> {
        let mut operands = Vec::new();
        let arguments = match self {
            Self::GenericFunction { arguments, .. } => Some(arguments),
            Self::GenericObjectMethod { object, arguments, .. } => {
                operands.push(object);
                Some(arguments)
            }
            Self::UnaryPlus(value) => {
                operands.push(value);
                None
            }
            Self::ObjectMemberCall { object, member, arguments, .. } => {
                operands.extend([object, member]);
                Some(arguments)
            }
            Self::StaticMember { class, member } => {
                if let RuntimeClassReceiver::Computed(value) = class {
                    operands.push(value);
                }
                match member {
                    RuntimeClassMember::ComputedProperty(value) | RuntimeClassMember::ComputedConstant(value) => {
                        operands.push(value);
                        None
                    }
                    RuntimeClassMember::Method { name, arguments } => {
                        operands.push(name);
                        Some(arguments)
                    }
                    RuntimeClassMember::GenericMethod { arguments, .. } => Some(arguments),
                    _ => None,
                }
            }
            Self::Write { target, operation } => {
                operands.push(target);
                if let RuntimeWrite::Assign { value, .. } | RuntimeWrite::Append(value) = operation {
                    operands.push(value);
                }
                None
            }
        };
        if let Some(MemberArguments::Call(args)) = arguments {
            operands.extend(args);
        }
        operands.into_iter()
    }
}
