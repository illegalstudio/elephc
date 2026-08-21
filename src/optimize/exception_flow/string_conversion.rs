//! Purpose:
//! Models catchable throws from PHP's implicit string conversions for exception-flow DCE.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::expr_throws()`
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::stmt_throws()`
//!
//! Key details:
//! - Checked callable return types preserve scalar precision while `mixed` and open object paths
//!   remain conservative.
//! - Known object types reuse their exact `__toString()` throw summaries.

use super::*;

impl ExceptionFlowAnalysis {
    /// Computes exceptions raised by PHP's implicit conversion of an expression to string.
    pub(super) fn string_conversion_throws(
        &self,
        expr: &Expr,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &expr.kind {
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ArrayLiteral(_)
            | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::BinaryOp { .. }
            | ExprKind::InstanceOf { .. }
            | ExprKind::Negate(_)
            | ExprKind::Not(_)
            | ExprKind::BitNot(_)
            | ExprKind::Print(_)
            | ExprKind::Cast { .. }
            | ExprKind::ConstRef(_)
            | ExprKind::ClassConstant { .. }
            | ExprKind::ObjectClassName { .. }
            | ExprKind::MagicConstant(_)
            | ExprKind::Throw(_) => ThrownTypes::default(),
            ExprKind::FunctionCall { name, .. } => self
                .function_returns
                .get(name.as_str())
                .or_else(|| {
                    crate::builtins::registry::lookup(name.as_str())
                        .map(|definition| &definition.return_type)
                })
                .map(|return_type| self.php_type_string_conversion_throws(return_type))
                .unwrap_or_else(ThrownTypes::unknown),
            ExprKind::StaticMethodCall {
                receiver, method, ..
            } => resolve_exception_receiver(receiver, class_context)
                .and_then(|class_name| {
                    self.resolve_method_value(
                        &class_name,
                        method,
                        &self.static_method_returns,
                    )
                })
                .map(|return_type| self.php_type_string_conversion_throws(&return_type))
                .unwrap_or_else(ThrownTypes::unknown),
            ExprKind::MethodCall { object, method, .. }
            | ExprKind::NullsafeMethodCall { object, method, .. } => {
                exact_receiver_class(object, class_context)
                    .and_then(|class_name| {
                        self.resolve_method_value(
                            &class_name,
                            method,
                            &self.instance_method_returns,
                        )
                    })
                    .map(|return_type| self.php_type_string_conversion_throws(&return_type))
                    .unwrap_or_else(ThrownTypes::unknown)
            }
            ExprKind::NewObject { class_name, .. } => self.php_type_string_conversion_throws(
                &PhpType::Object(class_name.as_str().to_string()),
            ),
            ExprKind::NewScopedObject { receiver, .. } => {
                resolve_exception_receiver(receiver, class_context)
                    .map(|class_name| {
                        self.php_type_string_conversion_throws(&PhpType::Object(class_name))
                    })
                    .unwrap_or_else(ThrownTypes::unknown)
            }
            ExprKind::ErrorSuppress(inner) | ExprKind::Spread(inner) => {
                self.string_conversion_throws(inner, class_context)
            }
            ExprKind::NamedArg { value, .. } => {
                self.string_conversion_throws(value, class_context)
            }
            ExprKind::Assignment { value, .. } => {
                self.string_conversion_throws(value, class_context)
            }
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self
                .string_conversion_throws(then_expr, class_context)
                .combined(self.string_conversion_throws(else_expr, class_context)),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => self
                .string_conversion_throws(value, class_context)
                .combined(self.string_conversion_throws(default, class_context)),
            ExprKind::Match { arms, default, .. } => {
                let mut thrown = arms.iter().fold(
                    ThrownTypes::default(),
                    |thrown, (_, value)| {
                        thrown.combined(self.string_conversion_throws(value, class_context))
                    },
                );
                if let Some(default) = default {
                    thrown =
                        thrown.combined(self.string_conversion_throws(default, class_context));
                }
                thrown
            }
            _ => ThrownTypes::unknown(),
        }
    }

    /// Converts a checked PHP result type into exceptions its string conversion may raise.
    fn php_type_string_conversion_throws(&self, php_type: &PhpType) -> ThrownTypes {
        match php_type {
            PhpType::Object(class_name) => self
                .resolve_method_value(
                    class_name,
                    "__toString",
                    &self.instance_method_throws,
                )
                .unwrap_or_else(ThrownTypes::unknown),
            PhpType::Union(members) => members.iter().fold(
                ThrownTypes::default(),
                |thrown, member| {
                    thrown.combined(self.php_type_string_conversion_throws(member))
                },
            ),
            PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Callable
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => ThrownTypes::unknown(),
            PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Resource(_)
            | PhpType::TaggedScalar => ThrownTypes::default(),
        }
    }
}
