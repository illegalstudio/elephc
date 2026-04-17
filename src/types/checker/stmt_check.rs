mod assignments;
mod control_flow;

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::TypeEnv;

use super::Checker;

impl Checker {
    pub fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::IfDef { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved ifdef statement"))
            }
            StmtKind::NamespaceDecl { .. }
            | StmtKind::NamespaceBlock { .. }
            | StmtKind::UseDecl { .. } => Err(CompileError::new(
                stmt.span,
                "Unresolved namespace/use statement",
            )),
            StmtKind::Echo(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { .. }
            | StmtKind::ArrayAssign { .. }
            | StmtKind::ArrayPush { .. }
            | StmtKind::TypedAssign { .. }
            | StmtKind::ConstDecl { .. }
            | StmtKind::ListUnpack { .. }
            | StmtKind::Global { .. }
            | StmtKind::StaticVar { .. }
            | StmtKind::PropertyAssign { .. }
            | StmtKind::PropertyArrayPush { .. }
            | StmtKind::PropertyArrayAssign { .. } => self.check_assignment_like_stmt(stmt, env),
            StmtKind::Foreach { .. }
            | StmtKind::Switch { .. }
            | StmtKind::If { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::While { .. }
            | StmtKind::For { .. }
            | StmtKind::Throw(..)
            | StmtKind::Try { .. } => self.check_control_flow_stmt(stmt, env),
            StmtKind::Include { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved include statement"))
            }
            StmtKind::PackedClassDecl { .. } => Ok(()),
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.infer_type(e, env)?;
                }
                Ok(())
            }
            StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => Ok(()),
            StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => Ok(()),
        }
    }
}
