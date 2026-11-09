//! Purpose:
//! Tells the method fixpoint which bodies a pass still has to walk.
//!
//! Called from:
//! - `method_pass::type_check_methods_until_stable`, around every method of every pass.
//!
//! Key details:
//! - The first pass moves every signature and later ones almost none: on Symfony Demo pass one
//!   moved 23 468 methods, passes two to four 49, 7 and 0, and each walked every body again.
//! - A body is kept when no member that moved is named anywhere in it. Names are read from the
//!   AST's `Debug` text, so a new node kind cannot be missed, and matched without case.
//! - What a kept body contributed is replayed: its errors and its unspecialized parameter seeds.
//!   What it wrote into the class table is still there.
//! - Anything that cannot be traced to a name walks everything: a member reached implicitly
//!   (`offsetGet`, `__toString`, ...), a dynamic member access, a class that is new, or a class
//!   field outside its methods and properties.
//! - `ELEPHC_CHECK_VERIFY=1` repeats a full pass after convergence and reports a difference.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};

use crate::errors::CompileError;
use crate::parser::ast::ClassMethod;
use crate::types::ClassInfo;

/// Members php reaches with no name at the site, so that a body cannot be seen to depend on them.
const IMPLICIT_MEMBERS: &[&str] = &[
    "__construct", "__destruct", "__clone", "__get", "__set", "__isset", "__unset", "__call",
    "__callstatic", "__invoke", "__tostring", "__debuginfo", "__serialize", "__unserialize",
    "__sleep", "__wakeup", "__set_state", "offsetget", "offsetset", "offsetexists", "offsetunset",
    "getiterator", "current", "key", "next", "rewind", "valid", "count", "jsonserialize",
];

/// AST nodes that name a member at run time: such a body can reach any member.
const DYNAMIC_NODES: &[&str] = &[
    "dynamicmethodcall", "dynamicstaticmethodcall", "nullsafedynamicmethodcall",
    "dynamicpropertyaccess", "dynamicstaticpropertyaccess", "nullsafedynamicpropertyaccess",
    "dynamicpropertyassign", "dynamicstaticpropertywrite", "exprcall", "newdynamic",
];

fn token_hash(token: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

/// Collects every identifier of formatted text, lowercased and hashed.
#[derive(Default)]
struct TokenScan {
    current: String,
    tokens: HashSet<u64>,
}

impl TokenScan {
    fn flush(&mut self) {
        if !self.current.is_empty() && !self.current.as_bytes()[0].is_ascii_digit() {
            self.tokens.insert(token_hash(&self.current));
        }
        self.current.clear();
    }
}

impl Write for TokenScan {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                self.current.push(byte.to_ascii_lowercase() as char);
            } else {
                self.flush();
            }
        }
        Ok(())
    }
}

/// What one body gave its last walk.
pub(super) struct MethodResult {
    pub(super) errors: Vec<CompileError>,
    pub(super) seed_params: Vec<usize>,
    tokens: HashSet<u64>,
}

/// The checker's class table.
type ClassTable = crate::fast_hash::FastMap<String, ClassInfo>;

/// One class's position in the flattened list and one method's in that class.
type Unit = (usize, usize);

/// The state of one run of the fixpoint.
#[derive(Default)]
pub(super) struct MethodWorklist {
    results: HashMap<Unit, MethodResult>,
    /// Hashes of the names the last pass moved, or `None` to walk everything.
    moved: Option<HashSet<u64>>,
    kept_any: bool,
}

impl MethodWorklist {
    /// The result to replay for a body nothing can have changed, if there is one.
    pub(super) fn kept_result(&mut self, unit: Unit, method_key: &str) -> Option<&MethodResult> {
        let moved = self.moved.as_ref()?;
        let result = self.results.get(&unit)?;
        if moved.contains(&token_hash(method_key)) || !moved.is_disjoint(&result.tokens) {
            return None;
        }
        self.kept_any = true;
        self.results.get(&unit)
    }

    /// Records a walked body. Its tokens are read once, on its first walk.
    pub(super) fn record(
        &mut self,
        unit: Unit,
        method: &ClassMethod,
        errors: Vec<CompileError>,
        seed_params: Vec<usize>,
    ) {
        let tokens = match self.results.remove(&unit) {
            Some(previous) => previous.tokens,
            None => {
                let mut scan = TokenScan::default();
                let _ = write!(scan, "{:?} {:?}", method.params, method.body);
                scan.flush();
                scan.tokens
            }
        };
        self.results.insert(unit, MethodResult { errors, seed_params, tokens });
    }

    /// Works out what the next pass must walk from what this one moved.
    pub(super) fn prepare_next_pass(
        &mut self,
        before: &ClassTable,
        after: &ClassTable,
    ) {
        self.kept_any = false;
        self.moved = moved_names(before, after).filter(|names| !names.iter().any(|name| is_implicit(name))).map(|names| {
            let mut hashes: HashSet<u64> = names.iter().map(|name| token_hash(name)).collect();
            if !hashes.is_empty() {
                hashes.extend(DYNAMIC_NODES.iter().map(|node| token_hash(node)));
            }
            hashes
        });
    }

    /// Limits the next run of the fixpoint to the bodies that name a function that moved.
    ///
    /// A function is called by its last name segment, which is how a body's tokens read it.
    pub(super) fn functions_moved<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        if self.results.is_empty() {
            return;
        }
        let mut hashes: HashSet<u64> = names
            .map(|name| token_hash(&name.rsplit('\\').next().unwrap_or(name).to_ascii_lowercase()))
            .collect();
        if !hashes.is_empty() {
            hashes.extend(DYNAMIC_NODES.iter().map(|node| token_hash(node)));
        }
        self.moved = Some(hashes);
        self.kept_any = false;
    }

    /// Whether the pass that just ended replayed at least one body.
    pub(super) fn kept_any(&self) -> bool {
        self.kept_any
    }

    /// Makes the next pass walk every body.
    pub(super) fn check_everything(&mut self) {
        self.moved = None;
        self.kept_any = false;
    }
}

fn is_implicit(name: &str) -> bool {
    IMPLICIT_MEMBERS.contains(&name)
}

/// The lowercased member names that differ between two tables, or `None` when a change cannot be
/// reduced to names.
fn moved_names(before: &ClassTable, after: &ClassTable) -> Option<HashSet<String>> {
    if before.len() != after.len() {
        return None;
    }
    let mut names = HashSet::default();
    for (class_name, after_class) in after {
        let before_class = before.get(class_name)?;
        if before_class == after_class {
            continue;
        }
        for (method, signature) in &after_class.methods {
            if before_class.methods.get(method) != Some(signature) {
                names.insert(method.to_ascii_lowercase());
            }
        }
        for (method, signature) in &after_class.static_methods {
            if before_class.static_methods.get(method) != Some(signature) {
                names.insert(method.to_ascii_lowercase());
            }
        }
        if before_class.properties.len() != after_class.properties.len() {
            return None;
        }
        for (old, new) in before_class.properties.iter().zip(&after_class.properties) {
            if old != new {
                names.insert(new.0.to_ascii_lowercase());
            }
        }
        // Everything else a pass could have written, compared by taking the members above out.
        let mut rest = before_class.clone();
        rest.methods = after_class.methods.clone();
        rest.static_methods = after_class.static_methods.clone();
        rest.properties = after_class.properties.clone();
        if rest != *after_class {
            return None;
        }
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> HashSet<u64> {
        let mut scan = TokenScan::default();
        scan.write_str(text).unwrap();
        scan.flush();
        scan.tokens
    }

    /// Verifies identifiers are matched without case, and numbers are not kept.
    #[test]
    fn tokens_ignore_case_and_numbers() {
        let scan = tokens("MethodCall { method: \"getName\", span: 12 }");
        assert!(scan.contains(&token_hash("getname")));
        assert!(scan.contains(&token_hash("methodcall")));
        assert!(!scan.contains(&token_hash("12")));
    }

    /// Verifies a body is kept only when no moved name appears in it, or as its own name.
    #[test]
    fn a_body_is_kept_when_nothing_it_names_moved() {
        let mut worklist = MethodWorklist::default();
        let unit = (0, 0);
        worklist.results.insert(unit, MethodResult {
            errors: Vec::new(),
            seed_params: Vec::new(),
            tokens: tokens("uses getname"),
        });
        assert!(worklist.kept_result(unit, "own").is_none(), "a first pass walks everything");
        worklist.moved = Some(HashSet::from_iter([token_hash("other")]));
        assert!(worklist.kept_result(unit, "own").is_some());
        assert!(worklist.kept_any());
        worklist.moved = Some(HashSet::from_iter([token_hash("getname")]));
        assert!(worklist.kept_result(unit, "own").is_none());
        worklist.moved = Some(HashSet::from_iter([token_hash("own")]));
        assert!(worklist.kept_result(unit, "own").is_none());
    }

    /// Verifies a later run keeps every body but those naming a moved function.
    #[test]
    fn a_later_run_follows_the_functions_that_moved() {
        let mut worklist = MethodWorklist::default();
        let unit = (0, 0);
        worklist.functions_moved(std::iter::empty());
        assert!(worklist.moved.is_none(), "nothing has been walked yet");
        worklist.results.insert(unit, MethodResult {
            errors: Vec::new(),
            seed_params: Vec::new(),
            tokens: tokens("FunctionCall helper"),
        });
        worklist.functions_moved(std::iter::empty());
        assert!(worklist.kept_result(unit, "own").is_some());
        worklist.functions_moved(["App\\Other"].into_iter());
        assert!(worklist.kept_result(unit, "own").is_some());
        worklist.functions_moved(["App\\Helper"].into_iter());
        assert!(worklist.kept_result(unit, "own").is_none());
    }

    /// Verifies a member php reaches implicitly cannot be traced to a name.
    #[test]
    fn an_implicit_member_is_recognized() {
        assert!(is_implicit("offsetget"));
        assert!(is_implicit("__tostring"));
        assert!(!is_implicit("getname"));
    }

    /// Verifies a dynamic access in a body keeps it from being replayed once anything moved.
    #[test]
    fn a_dynamic_access_is_never_kept() {
        let mut worklist = MethodWorklist::default();
        let unit = (0, 0);
        worklist.results.insert(unit, MethodResult {
            errors: Vec::new(),
            seed_params: Vec::new(),
            tokens: tokens("ExprCall"),
        });
        let mut moved = HashSet::from_iter([token_hash("other")]);
        moved.extend(DYNAMIC_NODES.iter().map(|node| token_hash(node)));
        worklist.moved = Some(moved);
        assert!(worklist.kept_result(unit, "own").is_none());
    }
}
