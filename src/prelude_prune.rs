//! Purpose:
//! Drops the prelude declarations a program cannot reach, before name resolution.
//!
//! Called from:
//! - `crate::image_prelude::inject_if_used`, which injects a 44-class surface whenever a program
//!   references any image builtin.
//! - `crate::web_prelude::inject_if_web`, which had its own copy of this reachability walk.
//!
//! Key details:
//! - WHY THIS EXISTS, measured rather than assumed. Injecting the image prelude into a program
//!   that calls `imagecreatetruecolor()` and `imagedestroy()` and nothing else turns a 9,501-line
//!   assembly file into a 162,630-line one and a 297 ms compile into a 1.8 s one. Building the
//!   declarations costs 1.9 ms of that; the rest is the assembler and linker chewing through
//!   Imagick, Gmagick and Cairo. The lever is not building faster, it is EMITTING LESS.
//! - IT PRUNES THE PRELUDE, NEVER THE PROGRAM. `prune` is called on the prelude's own
//!   declarations before they are combined with user code. A user declaration is never a
//!   candidate — frameworks register callbacks by name in ways no static walk can follow, and
//!   the program's own symbols are not this pass's to remove.
//! - CLASS GRANULARITY, DELIBERATELY, AND THE NUMBERS THAT SETTLED IT. Rooting a class keeps ALL
//!   of its methods. This walk runs before name resolution, so `$x->clear()` has no receiver
//!   type; the pruner answers it by rooting every class that declares `clear` (seven, in the
//!   image surface). That is an over-approximation by construction, and going finer was
//!   investigated and REJECTED — not on principle, on measurement.
//!
//!   The case for going finer looked overwhelming. For this program, in full —
//!   `<?php $db = new PDO("sqlite::memory:");` —
//!
//!   the emitted assembly is 57,302 lines against an 8,254-line floor, and 29,876 of those lines
//!   are method bodies. Closed over the prelude's own internal calls, `__construct` reaches 2,825
//!   of them. The other 25,335 — 89% — are dead, `PDOStatement::assignColumns` alone being 9,858.
//!   PDOStatement survives at all only because keeping `PDO` keeps `PDO::prepare`, whose body
//!   says `new PDOStatement`.
//!
//!   But that program is not one anybody writes. Opening a connection and never querying it is
//!   the flattering case, and measuring the honest one — connect, exec, prepare, execute,
//!   fetchAll, lastInsertId — collapses the headroom to 3,571 lines, 12% of the method output and
//!   about 6% of the binary. The realistic program's whole assembly is 60,745 lines against the
//!   trivial one's 57,302: it was already paying 94% of the bill. The class-level pass had
//!   already taken the gain; what is left is code a real program reaches.
//!
//!   And the price of taking that 6% is a WORSE FAILURE MODE, which is the part that actually
//!   decides it. At class granularity every channel this walk misses ends in a compile error:
//!   the class is gone, and naming it says so. At method granularity that stops being true.
//!   `__call` fields any method name, so on a class declaring one, every call becomes an
//!   existence test that quietly succeeds — the same silent-answer property that puts probes in
//!   the keep-everything tier. Worse, the set of methods the engine calls without the source
//!   naming them is not PHP's seventeen magic names: `src/intrinsics.rs` dispatches `current`,
//!   `key`, `next`, `rewind`, `valid` and `count` by string, so the real set is "whatever this
//!   compiler names", and it grows every time an intrinsic is added. A safety property that
//!   drifts with unrelated work is not one to build a silent pass on.
//!
//!   What makes class granularity affordable is precisely that it cannot be quietly wrong.
//! - THE FALLBACK IS NOT SURRENDER. The web prelude's original pruner kept everything the moment
//!   it saw a dynamic call. That is safe and useless: one `$f()` anywhere reimposes the whole
//!   surface. Instead, an unanalysable dynamic CALL switches to LITERAL HARVESTING — every
//!   prelude symbol whose name appears as a string literal becomes a root.
//! - A DYNAMIC CALL AND A DYNAMIC PROBE ARE NOT THE SAME RISK, and that is the whole reason the
//!   two are separate flags. `$f()` on a name this walk cannot read fails LOUDLY, at the call
//!   site, so widening the roots is a proportionate answer. `function_exists($f)` on a name it
//!   cannot read fails SILENTLY — the guard takes its else branch and the program does something
//!   different forever — so it keeps the surface, exactly as symbol-table enumeration does.
//! - BEFORE ADDING A PRELUDE TO THIS PASS, CHECK FOR SYNTHESISED ENTRY POINTS. Pruning runs
//!   before name resolution, and a later pass may REWRITE a construct into a call to a prelude
//!   function that the source never names: `name_resolver` desugars
//!   `DateTimeZone::listIdentifiers()` into `__elephc_list_identifiers()`, and `var_export($v)`
//!   into `__elephc_var_export_echo($v)`. At pruning time neither reference exists, so the
//!   target looks dead. Both were caught by `ir_lower::tests::corpus::lowers_examples_corpus`
//!   over 209 example programs — a grep for the name does NOT find them, because the compiler
//!   reaches one through a `const`. That is why `list_id_prelude` and `var_export_prelude` are
//!   NOT wired into this pass: measured on a sample program, pruning them saves nothing (their
//!   surfaces are wholly reachable from their entry points), so the risk buys nothing either.
//! - THE SILENT FAILURE MODE GOVERNS THE DESIGN. A pruned function that is CALLED fails loudly.
//!   A pruned function that is merely PROBED — `if (function_exists('imagecreatefromwebp'))` —
//!   silently takes the guard's else branch. So the subject of every probe is a reference, and
//!   the probe family is larger than it first looks: `method_exists`, `property_exists`,
//!   `is_subclass_of`, `get_class_methods` and their kin all answer quietly. `usage` enumerates
//!   the full set, and that enumeration — not this module — is where the pass's safety lives.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};

pub(crate) mod usage;

/// What a program can reach, and what it does that this walk cannot see through.
pub(crate) struct Roots {
    usage: usage::Usage,
}

/// Summarises what a program references, for [`prune`].
pub(crate) fn collect_roots(program: &Program) -> Roots {
    Roots {
        usage: usage::collect(program),
    }
}

impl Roots {
    /// Merges in the references of statements that are not part of the program but will run —
    /// the `--web` catch-all wrapper is one, since it calls `session_write_close()` from a
    /// `finally` the user never wrote.
    pub(crate) fn add(&mut self, extra: usage::Usage) {
        self.usage.merge(extra);
    }
}

/// One prelude declaration's name and the kind of symbol it declares.
enum Symbol<'a> {
    Function(&'a str),
    Class(&'a str),
}

/// Returns the symbol a top-level declaration defines, if it is one this pass can drop.
///
/// Constants and `extern` blocks are deliberately absent: a `ConstDecl` folds to a literal and an
/// `extern` lowers to nothing, so neither reaches the assembler, and dropping them would trade a
/// measured zero for a real risk.
fn declared_symbol(stmt: &Stmt) -> Option<Symbol<'_>> {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => Some(Symbol::Function(name)),
        StmtKind::ClassDecl { name, .. } => Some(Symbol::Class(name)),
        _ => None,
    }
}

/// Removes the declarations of `prelude` that `roots` cannot reach.
///
/// Returns the prelude unchanged when anything reachable enumerates the symbol table, which is
/// the one case no root set approximates.
pub(crate) fn prune(prelude: Program, roots: &Roots) -> Program {
    let index = Index::of(&prelude);
    let mut frontier = Frontier::new(&index);

    // What the PROGRAM names.
    frontier.absorb(&roots.usage);

    // What the pass is KEEPING ANYWAY. A prelude's top level is not necessarily all
    // declarations: the `--web` surface interleaves executable bootstrap statements that populate
    // the superglobals and may start the session. Those always run, so what they reference is
    // reachable by definition. Constants and externs count for the same reason — if it survives,
    // its references must survive with it.
    for stmt in prelude.iter() {
        if declared_symbol(stmt).is_none() {
            frontier.absorb(&usage::collect_stmt(stmt));
        }
    }

    // The transitive closure. A kept declaration's own references are roots in turn, INCLUDING
    // its dispatch hazards: `__ElephcCallableSessionHandler::read` does
    // `call_user_func($this->readCb, …)`, so keeping it means the surface now contains a call
    // this walk cannot name, exactly as if the program had made one.
    while let Some(position) = frontier.next() {
        frontier.absorb(&usage::collect_stmt(&prelude[position]));
    }

    if frontier.introspects {
        return prelude;
    }
    let kept = frontier.finish();
    prelude
        .into_iter()
        .enumerate()
        .filter(|(position, stmt)| declared_symbol(stmt).is_none() || kept.contains(position))
        .map(|(_, stmt)| stmt)
        .collect()
}

/// Where each prunable declaration sits, and which classes declare a given method.
///
/// The method index is what makes a receiver-less `->m()` answerable: this walk runs before name
/// resolution, so the only handle on `$x->clear()` is the NAME, and every class carrying it is
/// rooted.
struct Index {
    functions: HashMap<String, usize>,
    classes: HashMap<String, usize>,
    classes_by_method: HashMap<String, Vec<usize>>,
}

impl Index {
    fn of(prelude: &Program) -> Self {
        let mut index = Index {
            functions: HashMap::new(),
            classes: HashMap::new(),
            classes_by_method: HashMap::new(),
        };
        for (position, stmt) in prelude.iter().enumerate() {
            match declared_symbol(stmt) {
                Some(Symbol::Function(name)) => {
                    let previous = index.functions.insert(php_symbol_key(name), position);
                    debug_assert!(
                        previous.is_none(),
                        "a prelude declares {name} twice; the later one would shadow the earlier \
                         and this index would only ever reach one of them"
                    );
                }
                Some(Symbol::Class(name)) => {
                    let previous = index.classes.insert(php_symbol_key(name), position);
                    debug_assert!(previous.is_none(), "a prelude declares class {name} twice");
                    if let StmtKind::ClassDecl {
                        methods,
                        trait_uses,
                        ..
                    } = &stmt.kind
                    {
                        // The method index is built from DECLARED methods. A method arriving
                        // through a trait would be invisible to it, so `$x->m()` would root
                        // nothing and the class would be dropped — silently. No pruned surface
                        // uses traits today; this is the tripwire for the day one does.
                        debug_assert!(
                            trait_uses.is_empty(),
                            "class {name} uses traits, whose methods this index cannot see"
                        );
                        // `__call` fields ANY method name, so a class declaring one cannot be
                        // indexed by the methods it spells out. Today that costs nothing: a
                        // method root only ever has to reach a class the program already holds an
                        // instance of, and obtaining one roots the class by name. The tripwire is
                        // for the day a pruned surface declares `__call` and that reasoning needs
                        // rechecking rather than assuming.
                        debug_assert!(
                            !methods.iter().any(|method| {
                                method.name.eq_ignore_ascii_case("__call")
                                    || method.name.eq_ignore_ascii_case("__callStatic")
                            }),
                            "class {name} declares __call; the method index cannot see what it \
                             will accept"
                        );
                        for method in methods {
                            index
                                .classes_by_method
                                .entry(php_symbol_key(&method.name))
                                .or_default()
                                .push(position);
                        }
                    }
                }
                None => {}
            }
        }
        index
    }
}

/// The worklist, and the hazards seen so far.
///
/// Absorbing a `Usage` is the ONE place a reference becomes a root, so the tier policy is stated
/// once instead of at every call site.
struct Frontier<'a> {
    index: &'a Index,
    pending: VecDeque<usize>,
    kept: HashSet<usize>,
    /// Every string literal seen so far, held because a dynamic call discovered LATER makes the
    /// ones seen EARLIER into roots.
    literals: HashSet<String>,
    /// A call this walk cannot name has been seen; literals are roots from here on.
    harvests_literals: bool,
    /// Something enumerates the symbol table; no root set approximates that.
    introspects: bool,
}

impl<'a> Frontier<'a> {
    fn new(index: &'a Index) -> Self {
        Frontier {
            index,
            pending: VecDeque::new(),
            kept: HashSet::new(),
            literals: HashSet::new(),
            harvests_literals: false,
            introspects: false,
        }
    }

    /// Turns one subtree's references into roots, applying the tier policy.
    fn absorb(&mut self, usage: &usage::Usage) {
        self.introspects |= usage.introspects;
        for name in &usage.functions {
            self.root_function(name);
        }
        for name in &usage.classes {
            self.root_class(name);
        }
        for method in &usage.methods {
            self.root_method(method);
        }

        // T1 — an unanalysable dynamic call widens the roots to every symbol the surface NAMES,
        // rather than to every symbol there is. A program that dispatches on `$handler` but only
        // ever mentions `'imagecreate'` keeps `imagecreate`, not Imagick.
        //
        // INVARIANT: while `harvests_literals` holds, every name in `self.literals` has been
        // rooted. The two branches maintain it incrementally — the first is the transition, which
        // must reach back over literals banked before the hazard was discovered, because whether
        // it was discovered first or last is an accident of declaration order.
        let transition = usage.dynamic_function_call && !self.harvests_literals;
        self.harvests_literals |= usage.dynamic_function_call;
        if transition {
            self.literals.extend(usage.literals.iter().cloned());
            for name in std::mem::take(&mut self.literals) {
                self.root_literal(&name);
            }
        } else if self.harvests_literals {
            for name in &usage.literals {
                self.root_literal(name);
            }
        } else {
            self.literals.extend(usage.literals.iter().cloned());
        }
    }

    /// Roots a string that might name anything.
    ///
    /// A harvested literal carries no syntax to say what it is, so it is tried as a function, a
    /// class and a method. Over-approximating here is free — a name that matches nothing roots
    /// nothing — and under-approximating is how `$obj->$m()` with `$m = 'read'` loses every class
    /// that declares `read`.
    fn root_literal(&mut self, name: &str) {
        self.root_function(name);
        self.root_class(name);
        self.root_method(name);
    }

    fn root_function(&mut self, name: &str) {
        if let Some(position) = self.index.functions.get(name) {
            self.pending.push_back(*position);
        }
    }

    fn root_class(&mut self, name: &str) {
        if let Some(position) = self.index.classes.get(name) {
            self.pending.push_back(*position);
        }
    }

    fn root_method(&mut self, method: &str) {
        for position in self.index.classes_by_method.get(method).into_iter().flatten() {
            self.pending.push_back(*position);
        }
    }

    /// The next declaration to walk, skipping ones already kept.
    fn next(&mut self) -> Option<usize> {
        while let Some(position) = self.pending.pop_front() {
            if self.kept.insert(position) {
                return Some(position);
            }
        }
        None
    }

    fn finish(self) -> HashSet<usize> {
        self.kept
    }
}

#[cfg(test)]
mod tests;
