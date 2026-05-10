mod branches;
mod exprs;
mod includes;
mod members;
mod output;
mod stmts;

use std::collections::HashSet;
use std::path::Path;

use crate::errors::CompileError;
use crate::parser::ast::Stmt;

use super::state::ResolveState;
use output::{DiscoveryOutput, IncludeDiscovery};
use stmts::discover_stmts;

pub(in crate::resolver) use output::{
    DiscoveryEntry, FunctionVariantInfo, FunctionVariantKey, FunctionVariantRegistry,
};

pub(super) fn discover_include_declarations(
    stmts: &[Stmt],
    base_dir: &Path,
) -> Result<IncludeDiscovery, CompileError> {
    let mut output = DiscoveryOutput::default();
    let mut loaded_paths = HashSet::new();
    let mut include_chain = Vec::new();
    let mut state = ResolveState::default();

    discover_stmts(
        stmts,
        base_dir,
        &mut loaded_paths,
        &mut include_chain,
        &mut state,
        &mut output,
    )?;

    output.into_include_discovery()
}
