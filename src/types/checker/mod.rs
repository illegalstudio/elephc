pub(crate) mod builtins;
mod builtin_types;
mod callables;
mod driver;
mod extern_decl;
mod functions;
mod inference;
mod method_pass;
mod schema;
mod stmt_check;
mod type_compat;

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, Program, TypeExpr,
};
use crate::types::{
    CheckResult, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
pub(crate) use builtin_types::InterfaceDeclInfo;
use builtin_types::validate_magic_method_contracts;
use schema::propagate_abstract_return_types;

pub(crate) struct Checker {
    pub target_platform: Platform,
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Tracks known callable signatures assigned to variables.
    pub callable_sigs: HashMap<String, FunctionSig>,
    /// Tracks first-class callable targets assigned to variables.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// Interface definitions collected during first pass.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Class definitions collected during first pass.
    pub classes: HashMap<String, ClassInfo>,
    /// Canonical class names declared in the program, available for forward references.
    pub declared_classes: HashSet<String>,
    /// Enum definitions collected during first pass.
    pub enums: HashMap<String, EnumInfo>,
    /// Canonical interface names declared in the program, available for forward references.
    pub declared_interfaces: HashSet<String>,
    /// Name of the class currently being type-checked (for $this).
    pub current_class: Option<String>,
    /// Name of the current method, when type-checking a class method body.
    pub current_method: Option<String>,
    /// Whether the current class method is static.
    pub current_method_is_static: bool,
    /// Extern function declarations.
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Packed layout-only records.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Extern global variable declarations.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by extern blocks.
    pub required_libraries: Vec<String>,
    /// Best-known top-level variable types visible to `global` statements.
    pub top_level_env: TypeEnv,
    /// Names that are by-ref parameters in the current local scope.
    pub active_ref_params: HashSet<String>,
    /// Names introduced via `global` in the current local scope.
    pub active_globals: HashSet<String>,
    /// Names introduced via `static` in the current local scope.
    pub active_statics: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    pub return_type: Option<TypeExpr>,
    pub span: crate::span::Span,
    pub body: Vec<crate::parser::ast::Stmt>,
}

pub fn check_types(program: &Program, target_platform: Platform) -> Result<CheckResult, CompileError> {
    let (mut checker, global_env) = driver::check_types_impl(program, target_platform)?;

    propagate_abstract_return_types(&mut checker);
    validate_magic_method_contracts(&checker)?;

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        interfaces: checker.interfaces,
        classes: checker.classes,
        enums: checker.enums,
        packed_classes: checker.packed_classes,
        extern_functions: checker.extern_functions,
        extern_classes: checker.extern_classes,
        extern_globals: checker.extern_globals,
        required_libraries: checker.required_libraries,
        warnings: crate::types::warnings::collect_warnings(program),
    })
}
