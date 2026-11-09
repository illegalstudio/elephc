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
        if std::env::var_os("ELEPHC_TYPECHECK_TIMES").is_some() {
            trace_full_walk_reason(before, after);
        }
        self.moved = moved_names(before, after).filter(|names| !names.iter().any(|name| is_implicit(name) && !is_offset_member(name))).map(|names| {
            let mut hashes: HashSet<u64> = names.iter().map(|name| token_hash(name)).collect();
            // `ArrayAccess` members are reached through the nodes that index a value, so a body
            // depending on one names such a node: those nodes move with them.
            if names.iter().any(|name| is_offset_member(name)) {
                hashes.extend(OFFSET_ACCESS_NODES.iter().map(|node| token_hash(node)));
            }
            if !hashes.is_empty() {
                hashes.extend(DYNAMIC_NODES.iter().map(|node| token_hash(node)));
            }
            hashes
        });
    }

    /// Adds names to what the next pass treats as moved. Nothing to do when it walks everything.
    pub(super) fn also_moved<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        let Some(moved) = self.moved.as_mut() else {
            return;
        };
        let before = moved.len();
        moved.extend(names.map(|name| token_hash(&name.to_ascii_lowercase())));
        if before == 0 && !moved.is_empty() {
            moved.extend(DYNAMIC_NODES.iter().map(|node| token_hash(node)));
        }
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

/// The `ArrayAccess` members: php reaches them only through a node that indexes a value.
fn is_offset_member(name: &str) -> bool {
    matches!(name, "offsetget" | "offsetset" | "offsetexists" | "offsetunset")
}

/// AST nodes (as their `Debug` names read, lowercased) through which an `ArrayAccess` member is
/// reached: reads and probes (`ArrayAccess`, inside `isset`/`unset`/`??` too), writes and appends
/// on a local, a property or a static property, `list()` and `foreach` destructuring, and
/// references to an element. On the Symfony demo `offsetGet` moving made the third method pass
/// walk every body.
const OFFSET_ACCESS_NODES: &[&str] = &[
    "arrayaccess", "arrayassign", "nestedarrayassign", "arraypush", "listunpack", "foreach",
    "arrayreference", "refassign", "propertyrefassign", "propertyarraypush", "propertyarrayassign",
    "staticpropertyarraypush", "staticpropertyarrayassign", "staticpropertyelementrefassign",
];

/// `ELEPHC_TYPECHECK_TIMES`: says why the next pass walks every body, or which names moved.
fn trace_full_walk_reason(before: &ClassTable, after: &ClassTable) {
    if before.len() != after.len() {
        eprintln!("[elephc-worklist] full walk: class count {} -> {}", before.len(), after.len());
        return;
    }
    match moved_names(before, after) {
        Some(names) => {
            let mut implicit: Vec<&String> = names.iter().filter(|name| is_implicit(name)).collect();
            implicit.sort();
            eprintln!(
                "[elephc-worklist] moved {} names; implicit {:?}",
                names.len(),
                implicit
            );
        }
        None => {
            for (class_name, after_class) in after {
                let Some(before_class) = before.get(class_name) else {
                    eprintln!("[elephc-worklist] full walk: new class {class_name}");
                    return;
                };
                let mut names = HashSet::default();
                let Some(rest) = name_keyed_rest(before_class, after_class, &mut names) else {
                    eprintln!("[elephc-worklist] full walk: {class_name} slot count changed");
                    return;
                };
                if rest != *after_class {
                    let (old, new) = (format!("{rest:#?}"), format!("{after_class:#?}"));
                    let line = old
                        .lines()
                        .zip(new.lines())
                        .find(|(old, new)| old != new)
                        .map(|(old, new)| format!("{} => {}", old.trim(), new.trim()))
                        .unwrap_or_default();
                    eprintln!("[elephc-worklist] full walk: {class_name} field changed: {line}");
                    return;
                }
            }
        }
    }
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
        if name_keyed_rest(before_class, after_class, &mut names)? != *after_class {
            return None;
        }
    }
    Some(names)
}

/// Adds to `names` every member of `after_class` whose name-keyed entry differs from
/// `before_class`, and returns `before_class` with those fields taken from `after_class`: what is
/// left to compare. `None` when a slot list changed length, which no name describes.
///
/// A body reaches each of these fields only through the member's name, which its tokens hold:
/// methods and static methods, properties and static properties (`self::$x`), and the callable
/// signatures a method returns (`$this->m()(...)`). On the Symfony demo a static property of the
/// mbstring polyfill moving from `string` to `mixed`, and one callable return signature, each made
/// the second method pass walk all 23 000 bodies.
fn name_keyed_rest(
    before_class: &ClassInfo,
    after_class: &ClassInfo,
    names: &mut HashSet<String>,
) -> Option<ClassInfo> {
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
    for (before_sigs, after_sigs) in [
        (&before_class.callable_method_return_sigs, &after_class.callable_method_return_sigs),
        (&before_class.callable_array_method_return_sigs, &after_class.callable_array_method_return_sigs),
    ] {
        for (method, signature) in after_sigs {
            if before_sigs.get(method) != Some(signature) {
                names.insert(method.to_ascii_lowercase());
            }
        }
        for method in before_sigs.keys() {
            if !after_sigs.contains_key(method) {
                names.insert(method.to_ascii_lowercase());
            }
        }
    }
    for (before_slots, after_slots) in [
        (&before_class.properties, &after_class.properties),
        (&before_class.static_properties, &after_class.static_properties),
    ] {
        if before_slots.len() != after_slots.len() {
            return None;
        }
        for (old, new) in before_slots.iter().zip(after_slots) {
            if old != new {
                names.insert(old.0.to_ascii_lowercase());
                names.insert(new.0.to_ascii_lowercase());
            }
        }
    }
    // Everything else a pass could have written, compared by taking the members above out.
    let mut rest = before_class.clone();
    rest.methods = after_class.methods.clone();
    rest.static_methods = after_class.static_methods.clone();
    rest.properties = after_class.properties.clone();
    rest.static_properties = after_class.static_properties.clone();
    rest.callable_method_return_sigs = after_class.callable_method_return_sigs.clone();
    rest.callable_array_method_return_sigs = after_class.callable_array_method_return_sigs.clone();
    Some(rest)
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

    /// The checked class table of a small program, for the member-change tests.
    fn checked_classes() -> ClassTable {
        let source = "<?php\nclass Poly { public static $list = 'a'; public function cb() { return fn ($x) => $x; } }\n\
                      echo Poly::$list;\n";
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        crate::types::check(&program).expect("fixture must type check").classes
    }

    /// A static property whose type moved, or a method's callable return signature that moved,
    /// is a NAMED change: only bodies naming it walk again, where both used to walk every body.
    #[test]
    fn static_properties_and_callable_returns_move_by_name() {
        let before = checked_classes();
        let mut after = before.clone();
        let poly = after.get_mut("Poly").expect("Poly");
        let slot = poly.static_properties.iter_mut().find(|(name, _)| name == "list").expect("$list");
        slot.1 = crate::types::PhpType::Mixed;
        let names = moved_names(&before, &after).expect("a static property type is a named change");
        assert!(names.contains("list"), "{names:?}");

        let mut after = before.clone();
        let poly = after.get_mut("Poly").expect("Poly");
        let signature = poly.methods.get("cb").cloned().expect("cb");
        poly.callable_method_return_sigs.insert("cb".to_string(), signature);
        let names = moved_names(&before, &after).expect("a callable return signature is a named change");
        assert!(names.contains("cb"), "{names:?}");

        let mut after = before.clone();
        after.get_mut("Poly").expect("Poly").static_properties.clear();
        assert!(moved_names(&before, &after).is_none(), "a slot list that changed length is not");
    }

    /// An `ArrayAccess` member that moved re-walks the bodies that index a value, not every body;
    /// any other implicit member still walks everything.
    #[test]
    fn an_offset_member_moves_the_indexing_nodes() {
        let before = checked_classes();
        let mut after = before.clone();
        let poly = after.get_mut("Poly").expect("Poly");
        let signature = poly.methods.get("cb").cloned().expect("cb");
        poly.methods.insert("offsetGet".to_string(), signature.clone());
        let mut worklist = MethodWorklist::default();
        worklist.prepare_next_pass(&before, &after);
        let moved = worklist.moved.as_ref().expect("an offset member is traced to its nodes");
        assert!(moved.contains(&token_hash("arrayaccess")));
        assert!(moved.contains(&token_hash("offsetget")));

        let mut after = before.clone();
        after.get_mut("Poly").expect("Poly").methods.insert("__toString".to_string(), signature);
        worklist.prepare_next_pass(&before, &after);
        assert!(worklist.moved.is_none(), "__toString cannot be traced to a node");
    }

    /// Names added after a pass are moved too, with the dynamic nodes, unless everything walks.
    #[test]
    fn names_added_after_a_pass_are_moved() {
        let mut worklist = MethodWorklist::default();
        worklist.also_moved(["walk"].into_iter());
        assert!(worklist.moved.is_none(), "a pass that walks everything stays that way");
        worklist.moved = Some(HashSet::default());
        worklist.also_moved(["Walk"].into_iter());
        let moved = worklist.moved.as_ref().expect("moved");
        assert!(moved.contains(&token_hash("walk")));
        assert!(moved.contains(&token_hash("exprcall")));
    }
}
