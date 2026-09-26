//! Purpose:
//! Instantiates generic classes and interfaces into ordinary declarations.
//!
//! Called from:
//! - `crate::generics::monomorphize` (before the first type check, and after every function
//!   instantiation round)
//!
//! Key details:
//! - A generic CLASS needs no inference: its type arguments are written at every mention
//!   (`Box<int> $b`, `implements Repository<User>`, `new Box<int>(5)`). That is the whole
//!   reason this pass is pure syntax and runs before the checker, while a generic FUNCTION's
//!   arguments are inferred from call-site types and can only be resolved by the checker.
//! - Every mention is rewritten to `Named("Box<int>")` and one ordinary `ClassDecl` is emitted
//!   under that name, so no pass after this one has a notion of a generic class.
//! - Bounds are NOT checked here. Satisfying `T : Entity` is a subtyping question and only the
//!   checker holds the class table; this pass records one obligation per binding and the
//!   checker answers them with `type_accepts`, exactly as it does for generic functions.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::errors::CompileError;
use crate::generics::splice::substitute_in_body;
use crate::generics::{instantiated_name, Bindings, MAX_INSTANTIATION_ROUNDS};
use crate::magic_constants::walker::{walk_program, Pass};
use crate::names::Name;
use crate::parser::ast::{
    ExprKind, GenericDecl, MagicConstant, Program, Stmt, StmtKind, TypeExpr, TypeParam,
};
use crate::span::Span;

/// One type argument that has to satisfy its parameter's declared bound.
///
/// Produced here and answered by the checker: the pass that knows WHICH argument was bound to
/// which parameter is not the pass that can decide whether `User` is an `Entity`.
#[derive(Debug, Clone)]
pub struct BoundObligation {
    /// The template the argument was passed to, spelled as the mention wrote it (`Box`).
    /// For the diagnostic only — the instantiation is keyed by the normalized name.
    pub template: String,
    /// The type parameter's name (`T`).
    pub parameter: String,
    /// The declared upper bound (`Entity`).
    pub bound: TypeExpr,
    /// The type argument written at the mention (`User`).
    pub argument: TypeExpr,
    /// Where the mention was written, so the error lands on it.
    pub span: Span,
}

/// A generic class or interface declaration, indexed by the name it is declared under.
pub(super) struct Template {
    pub(super) declaration: Stmt,
    pub(super) type_params: Vec<TypeParam>,
    /// The name as the template DECLARED it, case intact.
    ///
    /// The map is keyed by a lowercased name because PHP class names are case-insensitive, but
    /// the instantiation is named from this: `get_class()` and every diagnostic report the name
    /// a programmer wrote, so `Box<int>` must not come back as `box<int>` because a mention
    /// somewhere spelled it `BOX`.
    pub(super) declared_name: String,
}

/// Every generic class and interface a program declares, lifted out of it.
///
/// Held SEPARATELY from the program, and collected once, because the templates are stripped
/// before the first type check — the checker has no representation for `T` — while a later
/// round still needs them: a generic FUNCTION instantiated after that check can have
/// `new Box<T>` in its body, which becomes `new Box<int>` the moment `T` is bound and asks for
/// an instantiation of a template the program no longer contains.
pub struct Templates {
    by_name: HashMap<String, Template>,
    /// The generic FUNCTIONS the program declares, by name.
    ///
    /// Not instantiated here — a function's type arguments are inferred by the checker — but a
    /// generic class mention inside one is not concrete either: `function unwrap<T>(Box<T> $b)`
    /// names `Box<T>`, and instantiating that would emit a class whose property is typed `T`.
    /// Recorded so the walk can leave those mentions alone until the checker binds `T` and the
    /// instantiated body comes back through here.
    generic_functions: std::collections::HashSet<String>,
    /// Template names found inside a CONDITIONAL, collected for the diagnostic alone.
    ///
    /// A declaration inside `if (…) { … }` is not part of the compiled program — an ordinary
    /// `class Foo` there is just as invisible, measured: `if (!class_exists('Foo')) { class Foo
    /// {} } new Foo();` reports `Undefined class: Foo`. So a template there is not collected
    /// either, and instantiating it would be the odd one out: its instantiations would be
    /// spliced at the TOP LEVEL, existing unconditionally next to the `Foo` that does not.
    ///
    /// What was worth fixing is the answer. `'Box' is written with type arguments but declares
    /// no type parameters` was FALSE — the declaration is right there and declares one — so the
    /// name is remembered here and the mention is told where its template actually is.
    conditional: std::collections::HashSet<String>,
}

/// One generic class's constructor signature, for inferring type arguments at `new`.
///
/// `new Box(5)` writes no type arguments, so the only thing that can determine `T` is what the
/// constructor declares its parameters as, matched against the argument types — which only the
/// CHECKER knows. This is the slice of a template the checker needs; the template's body stays
/// here, because instantiating it is still this module's job.
#[derive(Debug, Clone)]
pub struct TemplateSignature {
    /// The normalized lookup key, as [`template_key`] produces it.
    pub key: String,
    /// The name as the template declared it, for the instantiated name and for diagnostics.
    pub declared_name: String,
    pub type_params: Vec<TypeParam>,
    /// The constructor's declared parameter NAMES, in order. A named argument binds by name, so
    /// ordering a construction against the declaration needs them.
    pub constructor_param_names: Vec<String>,
    /// The constructor's declared parameter types, in order. Empty when the template declares
    /// no constructor — in which case nothing constrains its parameters and the mention has to
    /// write them.
    pub constructor_params: Vec<Option<TypeExpr>>,
    /// The constructor's declared variadic element type, if any — a binding position like the
    /// fixed ones, living in its own field.
    pub constructor_variadic: Option<TypeExpr>,
    /// The template's STATIC methods, by name, with their declared parameter and return types.
    ///
    /// `Box::of(5)` has to determine `T` the same way `new Box(5)` does, from the arguments
    /// against the declared parameters. PHP allows exactly one `__construct`, so a named static
    /// factory is how a real codebase offers more than one way to build something — inference
    /// that fires only at `new` would miss where most construction happens.
    pub static_methods: Vec<StaticMethodSignature>,
}

/// One static method of a generic class, as the checker needs it to infer at a call.
#[derive(Debug, Clone)]
pub struct StaticMethodSignature {
    pub name: String,
    /// Declared parameter NAMES in order, for the same reason the constructor records them.
    pub param_names: Vec<String>,
    pub params: Vec<Option<TypeExpr>>,
    /// Declared variadic element type, if any.
    pub variadic_type: Option<TypeExpr>,
    pub return_type: Option<TypeExpr>,
}

impl Templates {
    /// Returns the constructor signature of every template, for call-site inference.
    pub fn signatures(&self) -> Vec<TemplateSignature> {
        self.by_name
            .iter()
            .map(|(key, template)| TemplateSignature {
                key: key.clone(),
                declared_name: template.declared_name.clone(),
                type_params: template.type_params.clone(),
                constructor_param_names: constructor_param_names(&template.declaration),
                constructor_params: constructor_param_types(&template.declaration),
                constructor_variadic: constructor_variadic_type(&template.declaration),
                static_methods: static_method_signatures(&template.declaration),
            })
            .collect()
    }

    /// Returns whether the program declared no generic class or interface at all.
    ///
    /// Reported, not acted on: the walk runs either way. A program with no template can still
    /// MENTION one — `new Plain<int>()` on an ordinary class — and that mention has to be
    /// rejected rather than left for a later pass to meet a type it has no representation for.
    /// Skipping the walk on an empty template set is what let that reach the checker and panic.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Every collected template, for a pass that reads declarations rather than instantiating.
    pub(super) fn declarations(&self) -> impl Iterator<Item = &Template> {
        self.by_name.values()
    }

    /// One template by its normalized name, for a pass that follows an inheritance clause.
    pub(super) fn by_key(&self, key: &str) -> Option<&Template> {
        self.by_name.get(key)
    }
}

/// Lifts every generic class and interface declaration out of `program`.
pub fn collect(program: &Program) -> Templates {
    let mut by_name = HashMap::new();
    let mut generic_functions = std::collections::HashSet::new();
    collect_templates_into(program, &mut by_name, &mut generic_functions);
    let mut conditional = std::collections::HashSet::new();
    collect_conditional_template_names(program, &mut conditional);
    conditional.retain(|name| !by_name.contains_key(name));
    Templates {
        by_name,
        generic_functions,
        conditional,
    }
}

/// Records the names of templates declared inside a nested body, for the diagnostic.
///
/// Deliberately shallow about WHICH construct: every nested body is equally invisible, so a
/// template in an `if`, a `while`, a `try` or a `switch` case all get the same answer. Only the
/// name is kept — nothing here is instantiated, so nothing else about the declaration matters.
fn collect_conditional_template_names(
    stmts: &[Stmt],
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, generics, .. }
            | StmtKind::InterfaceDecl { name, generics, .. } => {
                if generics
                    .as_ref()
                    .is_some_and(|generics| !generics.type_params.is_empty())
                {
                    out.insert(template_key(name));
                }
            }
            _ => {}
        }
        for body in nested_bodies(stmt) {
            collect_conditional_template_names(body, out);
        }
    }
}

/// Returns every statement list nested inside one statement.
///
/// A `NamespaceBlock` is NOT here: its body is ordinary top-level code, which
/// `collect_templates_into` already descends into, and listing it would report every namespaced
/// template as conditional.
fn nested_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    let mut bodies: Vec<&[Stmt]> = Vec::new();
    match &stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            for (_, body) in elseif_clauses {
                bodies.push(body);
            }
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => bodies.push(body),
        StmtKind::Synthetic(body) => bodies.push(body),
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                bodies.push(body);
            }
            if let Some(body) = default {
                bodies.push(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            bodies.push(try_body);
            for catch in catches {
                bodies.push(&catch.body);
            }
            if let Some(body) = finally_body {
                bodies.push(body);
            }
        }
        _ => {}
    }
    bodies
}

/// Returns the declared parameter types of a class declaration's constructor, in order.
///
/// An interface cannot be constructed and a template without a `__construct` constrains
/// nothing, so both answer with an empty list and the mention is left to write its arguments.
fn constructor_param_names(declaration: &Stmt) -> Vec<String> {
    constructor_method(declaration)
        .map(|method| {
            method
                .params
                .iter()
                .map(|(name, _, _, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the constructor's declared variadic element type, if it declares one.
fn constructor_variadic_type(declaration: &Stmt) -> Option<TypeExpr> {
    constructor_method(declaration).and_then(|method| method.variadic_type.clone())
}

/// Returns the template's `__construct`, if it declares one.
fn constructor_method(declaration: &Stmt) -> Option<&crate::parser::ast::ClassMethod> {
    let StmtKind::ClassDecl { methods, .. } = &declaration.kind else {
        return None;
    };
    methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case("__construct"))
}

/// Returns the constructor's declared parameter types, in order.
fn constructor_param_types(declaration: &Stmt) -> Vec<Option<TypeExpr>> {
    let StmtKind::ClassDecl { methods, .. } = &declaration.kind else {
        return Vec::new();
    };
    methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case("__construct"))
        .map(|method| {
            method
                .params
                .iter()
                .map(|(_, declared, _, _)| declared.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the declared signature of every static method on a class declaration.
fn static_method_signatures(declaration: &Stmt) -> Vec<StaticMethodSignature> {
    let StmtKind::ClassDecl { methods, .. } = &declaration.kind else {
        return Vec::new();
    };
    methods
        .iter()
        .filter(|method| method.is_static)
        .map(|method| StaticMethodSignature {
            name: method.name.clone(),
            param_names: method
                .params
                .iter()
                .map(|(name, _, _, _)| name.clone())
                .collect(),
            params: method
                .params
                .iter()
                .map(|(_, declared, _, _)| declared.clone())
                .collect(),
            variadic_type: method.variadic_type.clone(),
            return_type: method.return_type.clone(),
        })
        .collect()
}

/// Rewrites every generic class mention to its instantiated name, recording what it asked for.
///
/// Implements the AST walker's `Pass` so that EVERY type position is covered — property and
/// constant annotations, method signatures, typed locals, closure types, inheritance clauses —
/// without a second walker that could disagree with the first about where a type can appear.
struct Instantiate<'a> {
    templates: &'a HashMap<String, Template>,
    generic_functions: &'a std::collections::HashSet<String>,
    /// Template names declared inside a conditional, for the diagnostic that names them.
    conditional: &'a std::collections::HashSet<String>,
    /// (enclosing function, `new` site) -> the class the checker inferred for it.
    new_names: &'a HashMap<(String, Span), String>,
    /// `RefCell` because `Pass::transform_type` takes `&self`: the walk rebuilds the tree and
    /// hands each type to a shared reference, so recording what a type asked for cannot go
    /// through `&mut self`.
    requests: RefCell<Vec<(String, Bindings)>>,
    obligations: RefCell<Vec<BoundObligation>>,
    errors: RefCell<Vec<CompileError>>,
    /// What a BARE template name was read as, so the reader learns it was not ignored.
    warnings: RefCell<Vec<crate::errors::CompileWarning>>,
    /// How many enclosing declarations still declare type parameters.
    ///
    /// A mention inside a template is not instantiated: `class Wrapper<T> { private Box<T> $b; }`
    /// names `Box<T>`, and `T` has no meaning until `Wrapper` itself is instantiated. The copy
    /// made for `Wrapper<int>` carries `Box<int>`, which the next round picks up.
    ///
    /// A generic FUNCTION counts the same way and for the same reason: `function unwrap<T>(Box<T>
    /// $b)` names a `Box<T>` that is concrete only once the checker binds `T`, and the body it
    /// splices then comes back through this pass naming `Box<int>`.
    ///
    /// Kept by the walker's own `enter_class`/`enter_function` hooks rather than by a pre-pass
    /// over top-level statements, because a template can be declared inside a namespace block or
    /// a conditional and both nest.
    in_template: usize,
    /// Whether each entered METHOD scope incremented `in_template`, innermost last.
    method_templates: Vec<bool>,
    /// The functions the walk is lexically inside, innermost last.
    ///
    /// Half the key for the construction rewrite, and it has to be built exactly as the checker
    /// builds `current_function` — the declared name, unnormalized — or the lookup misses and
    /// a `new Box(5)` keeps naming the stripped template. A miss is loud (`Undefined class`),
    /// not silent, which is what makes mirroring the key safe to do here at all.
    enclosing_functions: Vec<String>,
    /// The classes the walk is lexically inside, innermost last.
    ///
    /// `self` in a type argument means this, and it has to be substituted BEFORE the
    /// instantiated name is built — `Box<self>` names no class, `Box<A>` does. The checker's
    /// own `substitute_relative_class_types` runs far too late: this pass has already emitted
    /// a declaration by then.
    enclosing_classes: Vec<String>,
}

impl Instantiate<'_> {
    /// Rewrites one type, recursing so a generic class nested anywhere inside is reached.
    ///
    /// The walker hands over a whole annotation (`?array<Box<int>>`), not one node at a time,
    /// so the recursion belongs here rather than in the walker.
    fn rewrite(&self, ty: TypeExpr, span: Span) -> TypeExpr {
        match ty {
            TypeExpr::GenericClass { name, args } => {
                let args: Vec<TypeExpr> = args
                    .into_iter()
                    .map(|arg| self.resolve_relative(self.rewrite(arg, span), span))
                    .collect();
                match self.request(&name, &args, span) {
                    Some(instantiated) => TypeExpr::Named(Name::unqualified(&instantiated)),
                    None => TypeExpr::GenericClass { name, args },
                }
            }
            // A BARE template name — `function idOf(Repo $r)` where `Repo<T : Entity>` — used to
            // be `Unknown type: Repo`, because the templates are stripped before the checker runs
            // and nothing under that name survives. It is read as the instantiation its own
            // declaration describes: each parameter at its BOUND, or its DEFAULT, or `mixed` when
            // it has neither.
            //
            // And it warns, every time, because the reading is weaker than it looks.
            // Monomorphization makes each instantiation a distinct class, so `Repo<Entity>`
            // accepts a `Repo<User>` only where the parameter is declared covariant (`out T`);
            // on an invariant template this parameter takes exactly one type, and saying which
            // is the better program.
            TypeExpr::Named(name) if self.templates.contains_key(&template_key(name.as_str())) => {
                let template = &self.templates[&template_key(name.as_str())];
                let arguments: Vec<TypeExpr> = template
                    .type_params
                    .iter()
                    .map(|param| {
                        param
                            .bound
                            .clone()
                            .or_else(|| param.default.clone())
                            .unwrap_or_else(|| TypeExpr::Named(Name::unqualified("mixed")))
                    })
                    .collect();
                let rendered = arguments
                    .iter()
                    .map(crate::generics::describe_type)
                    .collect::<Vec<_>>()
                    .join(", ");
                self.warnings
                    .borrow_mut()
                    .push(crate::errors::CompileWarning::new(
                        span,
                        &format!(
                            "'{}' names a template and is read as '{}<{}>'; each instantiation is \
                             its own class, so this accepts another instantiation only where the \
                             parameter is declared covariant. Write the type arguments to say \
                             which one you mean",
                            name.as_str(),
                            template.declared_name,
                            rendered
                        ),
                    ));
                match self.request(&name, &arguments, span) {
                    Some(instantiated) => TypeExpr::Named(Name::unqualified(&instantiated)),
                    None => TypeExpr::Named(name),
                }
            }
            TypeExpr::Array(inner) => TypeExpr::Array(Box::new(self.rewrite(*inner, span))),
            TypeExpr::Buffer(inner) => TypeExpr::Buffer(Box::new(self.rewrite(*inner, span))),
            TypeExpr::Nullable(inner) => TypeExpr::Nullable(Box::new(self.rewrite(*inner, span))),
            TypeExpr::AssocArray { key, value } => TypeExpr::AssocArray {
                key: Box::new(self.rewrite(*key, span)),
                value: Box::new(self.rewrite(*value, span)),
            },
            TypeExpr::Union(members) => TypeExpr::Union(
                members
                    .into_iter()
                    .map(|member| self.rewrite(member, span))
                    .collect(),
            ),
            TypeExpr::Intersection(members) => TypeExpr::Intersection(
                members
                    .into_iter()
                    .map(|member| self.rewrite(member, span))
                    .collect(),
            ),
            other => other,
        }
    }

    /// Substitutes `self` in a type argument, and rejects the two relative types it cannot.
    ///
    /// `self` is lexical: it is whichever class the walk is inside, which `enter_class` has
    /// already recorded. `static` is LATE-bound — `Box<static>` in a parent means a different
    /// class per subclass, which monomorphization would have to answer with one instantiation
    /// per subclass — and `parent` names the inheritance clause, which this pass does not
    /// carry. Guessing either would emit a class under a name that means something else, so
    /// both are named and refused.
    fn resolve_relative(&self, ty: TypeExpr, span: Span) -> TypeExpr {
        let TypeExpr::Named(name) = &ty else {
            return ty;
        };
        let lowered = name.as_str().to_ascii_lowercase();
        match lowered.as_str() {
            "self" => match self.enclosing_classes.last() {
                Some(class) => TypeExpr::Named(Name::unqualified(class)),
                None => {
                    self.errors.borrow_mut().push(CompileError::new(
                        span,
                        "'self' is not a type argument outside a class",
                    ));
                    ty
                }
            },
            "static" | "parent" => {
                self.errors.borrow_mut().push(CompileError::new(
                    span,
                    &format!(
                        "'{}' cannot be a type argument: it is not known where the class is \
                         declared. Name the class instead",
                        name.as_str()
                    ),
                ));
                ty
            }
            _ => ty,
        }
    }

    /// Resolves one `Name<args>` mention to the instantiated class name, or records why not.
    ///
    /// Returns `None` when the mention cannot be honored, leaving the `GenericClass` in place;
    /// the recorded error is reported before the program reaches the checker, so an unresolved
    /// mention is never compiled.
    fn request(&self, name: &Name, args: &[TypeExpr], span: Span) -> Option<String> {
        let key = template_key(name.as_str());
        let Some(template) = self.templates.get(&key) else {
            self.errors.borrow_mut().push(CompileError::new(
                span,
                &if self.conditional.contains(&key) {
                    format!(
                        "'{}' declares type parameters, but inside a conditional, and a \
                         declaration there is not part of the compiled program — an ordinary \
                         class is invisible the same way. Move it to the top level",
                        name.as_str()
                    )
                } else {
                    format!(
                        "'{}' is written with type arguments but declares no type parameters",
                        name.as_str()
                    )
                },
            ));
            return None;
        };
        if args.len() > template.type_params.len() {
            self.errors.borrow_mut().push(CompileError::new(
                span,
                &format!(
                    "'{}' takes {} type argument(s) but {} were given",
                    name.as_str(),
                    template.type_params.len(),
                    args.len()
                ),
            ));
            return None;
        }
        let mut bindings: Bindings = Vec::with_capacity(template.type_params.len());
        for (index, param) in template.type_params.iter().enumerate() {
            // A parameter past the written arguments falls back to its default. Without one
            // there is nothing to bind: guessing `mixed` would give up exactly the storage the
            // annotation exists to pin.
            let argument = match args.get(index) {
                Some(argument) => argument.clone(),
                None => match &param.default {
                    // The default is written in the TEMPLATE's vocabulary, so an earlier
                    // parameter mentioned in it (`<K, V = K>`) resolves against what this
                    // mention already bound.
                    Some(default) => default.substitute_type_params(&bindings),
                    None => {
                        self.errors.borrow_mut().push(CompileError::new(
                            span,
                            &format!(
                                "'{}' needs a type argument for <{}>, which has no default",
                                name.as_str(),
                                param.name
                            ),
                        ));
                        return None;
                    }
                },
            };
            if let Some(bound) = &param.bound {
                self.obligations.borrow_mut().push(BoundObligation {
                    template: name.as_str().to_string(),
                    parameter: param.name.clone(),
                    bound: bound.substitute_type_params(&bindings),
                    argument: argument.clone(),
                    span,
                });
            }
            bindings.push((param.name.clone(), argument));
        }
        let instantiated = instantiated_name(&template.declared_name, &bindings);
        self.requests.borrow_mut().push((key, bindings));
        Some(instantiated)
    }
}

impl Pass for Instantiate<'_> {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    fn transform_type(&self, ty: TypeExpr, span: Span) -> TypeExpr {
        if self.in_template > 0 {
            return ty;
        }
        self.rewrite(ty, span)
    }

    fn transform_class_reference(&self, class_name: Name, span: Span) -> Name {
        let scope = self
            .enclosing_functions
            .last()
            .cloned()
            .unwrap_or_else(|| "main".to_string());
        match self.new_names.get(&(scope, span)) {
            Some(instantiated) => Name::unqualified(instantiated),
            None => class_name,
        }
    }

    fn enter_class(&mut self, name: &str) {
        if self.templates.contains_key(&template_key(name)) {
            self.in_template += 1;
        }
        self.enclosing_classes.push(name.to_string());
    }

    fn enter_function(&mut self, name: &str) {
        if self.generic_functions.contains(&template_key(name)) {
            self.in_template += 1;
        }
        self.enclosing_functions.push(name.to_string());
    }

    fn leave_function(&mut self) {
        self.in_template = self.in_template.saturating_sub(1);
        self.enclosing_functions.pop();
    }

    fn enter_method(&mut self, name: &str, type_params: &[TypeParam]) {
        // A generic METHOD is a template for the same reason a generic function is: `Box<U>` in
        // its signature names no class until a call binds `U`, and instantiating it here would
        // emit a class literally called `Box<U>`.
        //
        // Whether this scope incremented is REMEMBERED rather than re-derived on the way out.
        // Every method of a generic class passes through here, so a `leave` that always
        // decremented would cancel the enclosing CLASS's increment on the first ordinary method
        // and start instantiating `Box<T>` inside a template.
        let is_template = !type_params.is_empty();
        if is_template {
            self.in_template += 1;
        }
        self.method_templates.push(is_template);
        self.enclosing_functions.push(name.to_string());
    }

    fn leave_method(&mut self) {
        if self.method_templates.pop().unwrap_or(false) {
            self.in_template = self.in_template.saturating_sub(1);
        }
        self.enclosing_functions.pop();
    }

    fn leave_class(&mut self) {
        // Only a template incremented it, and the walker pairs every enter with a leave, so a
        // saturating decrement cannot drift: an ordinary class leaves the counter at zero.
        self.in_template = self.in_template.saturating_sub(1);
        self.enclosing_classes.pop();
    }
}

/// Instantiates every generic class and interface the program mentions.
///
/// Returns the program with no generic declaration left in it — every template stripped, every
/// mention rewritten — plus the bound obligations the checker still has to answer.
///
/// Idempotent, and called once per monomorphization round for that reason: a program with no
/// generic mention left comes back unchanged with no obligations.
pub fn instantiate(
    program: Program,
    templates: &Templates,
    inferred: &InferredConstructions,
) -> Result<Instantiated, CompileError> {
    let templates_store = templates;
    let templates = &templates_store.by_name;
    // What the CHECKER worked out about `new Box(5)`, which this pass could not: the classes it
    // asked for, spliced here, and the constructions themselves, renamed by the walk below.
    let (program, new_names) = adopt_inferred_constructions(program, templates, inferred)?;
    let mut program = program;
    let mut obligations = Vec::new();
    let mut warnings: Vec<crate::errors::CompileWarning> = Vec::new();
    let mut rounds = 0usize;
    loop {
        let mut pass = Instantiate {
            templates,
            generic_functions: &templates_store.generic_functions,
            conditional: &templates_store.conditional,
            new_names: &new_names,
            requests: RefCell::new(Vec::new()),
            obligations: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
            warnings: RefCell::new(Vec::new()),
            in_template: 0,
            method_templates: Vec::new(),
            enclosing_functions: Vec::new(),
            enclosing_classes: Vec::new(),
        };
        let walked = walk_program(program, &mut pass);
        let mut errors = pass.errors.into_inner();
        if !errors.is_empty() {
            return Err(errors.remove(0));
        }
        obligations.extend(pass.obligations.into_inner());
        // Deduplicated on the way out: the walk runs once per round and a bare mention reads the
        // same way every time, so the same warning is produced again on each of them.
        for warning in pass.warnings.into_inner() {
            if !warnings
                .iter()
                .any(|seen| seen.span == warning.span && seen.message == warning.message)
            {
                warnings.push(warning);
            }
        }
        let requests = pass.requests.into_inner();
        let (spliced, added) = splice(walked, templates, &requests);
        program = spliced;
        if added == 0 {
            break;
        }
        rounds += 1;
        if rounds >= MAX_INSTANTIATION_ROUNDS {
            return Err(CompileError::new(
                runaway_span(&program),
                &format!(
                    "Generic class instantiation did not settle after {} rounds. Every round \
                     added a new instantiation, so some generic class holds itself at an \
                     ever-changing type",
                    MAX_INSTANTIATION_ROUNDS
                ),
            ));
        }
    }
    Ok(Instantiated {
        program: strip_templates(program),
        obligations,
        warnings,
    })
}

/// What one run of the instantiating pass produced.
///
/// A struct rather than a tuple because the third member arrived after the first two, and
/// `(Program, Vec<BoundObligation>, Vec<CompileWarning>)` says nothing at the call site about
/// which is which.
#[derive(Debug)]
pub struct Instantiated {
    pub program: Program,
    pub obligations: Vec<BoundObligation>,
    /// Warnings the pass itself produced — today, every BARE template name it read as an
    /// instantiation.
    pub warnings: Vec<crate::errors::CompileWarning>,
}

/// What a checker walk worked out about constructions whose type arguments were not written.
///
/// Produced by `Checker::infer_generic_construction` and consumed here on the next round. It is
/// the one part of generic class instantiation this module cannot do alone: `new Box(5)` names
/// no arguments, so the constructor's parameters have to be matched against the ARGUMENT TYPES,
/// and only the checker knows those.
#[derive(Debug, Default)]
pub struct InferredConstructions {
    /// The instantiations to splice, as (template key, type arguments).
    pub requested: Vec<(String, Bindings)>,
    /// (enclosing function, construction site) -> every class that site resolved to.
    ///
    /// Still a SET after the enclosing function is in the key, because a `Span` carries no file
    /// identity: two same-named functions in two included files collide. That residue is
    /// rejected rather than resolved — see [`adopt_inferred_constructions`].
    pub sites: HashMap<(String, Span), std::collections::BTreeSet<String>>,
}

/// Splices the checker's inferred instantiations and resolves each construction site to one name.
///
/// A site that resolved to two different classes is a compile ERROR, not a choice. Rewriting it
/// to either one would compile a program that constructs the wrong class at one of the two
/// places it appears, with nothing to notice it afterwards — the AST would name a class that
/// exists and lowering would build it. Writing the type arguments is the way out, and the
/// message says so.
fn adopt_inferred_constructions(
    program: Program,
    templates: &HashMap<String, Template>,
    inferred: &InferredConstructions,
) -> Result<(Program, HashMap<(String, Span), String>), CompileError> {
    if inferred.requested.is_empty() && inferred.sites.is_empty() {
        return Ok((program, HashMap::new()));
    }
    let mut names = HashMap::with_capacity(inferred.sites.len());
    for (site, resolved) in &inferred.sites {
        let (_, span) = site;
        if resolved.len() > 1 {
            return Err(CompileError::new(
                *span,
                &format!(
                    "This construction resolves to more than one generic class ({}); \
                     write the type arguments to say which one is meant",
                    resolved.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
        if let Some(single) = resolved.iter().next() {
            names.insert(site.clone(), single.clone());
        }
    }
    let (program, _) = splice(program, templates, &inferred.requested);
    Ok((program, names))
}

/// Walks statements, including namespace blocks, collecting templates.
///
/// Deliberately NOT a full statement walk. A class declared inside a conditional
/// (`if (...) { class Box<T> {...} }`) is not collected and not stripped, so a mention of it
/// is rejected as naming no template and the declaration itself reaches the checker as
/// `Unknown type: T`. Both are hard errors — no program compiles to something it did not say —
/// and covering the case properly means an exhaustive statement walk, which is the one thing
/// this module borrows `magic_constants::walker` to avoid writing twice.
fn collect_templates_into(
    stmts: &[Stmt],
    templates: &mut HashMap<String, Template>,
    generic_functions: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name, type_params, ..
            } if !type_params.is_empty() => {
                generic_functions.insert(template_key(name));
            }
            StmtKind::ClassDecl { name, generics, .. }
            | StmtKind::InterfaceDecl { name, generics, .. } => {
                let Some(generics) = generics else { continue };
                if generics.type_params.is_empty() {
                    continue;
                }
                templates.insert(
                    template_key(name),
                    Template {
                        declaration: stmt.clone(),
                        type_params: generics.type_params.clone(),
                        declared_name: name.clone(),
                    },
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_templates_into(body, templates, generic_functions)
            }
            _ => {}
        }
    }
}

/// Normalizes a declared or written class name to the key both sides agree on.
///
/// The name resolver qualifies a mention to `\App\Box` while the declaration is stored without
/// the leading separator, so it is dropped on both sides. Matching is case-insensitive because
/// PHP class names are. The instantiated name itself is built from the template's DECLARED
/// spelling, not from this key, so `Box<int>` and `BOX<int>` name one class — the one the
/// declaration spelled.
pub fn template_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Appends one ordinary declaration per requested instantiation the program does not carry.
///
/// Returns the program and how many were added; a zero is the fixpoint signal.
fn splice(
    program: Program,
    templates: &HashMap<String, Template>,
    requests: &[(String, Bindings)],
) -> (Program, usize) {
    if requests.is_empty() {
        return (program, 0);
    }
    let mut declared: Vec<String> = Vec::new();
    collect_declared_names(&program, &mut declared);

    let mut program = program;
    let mut added = 0usize;
    for (base, bindings) in requests {
        let Some(template) = templates.get(base) else {
            continue;
        };
        let target = instantiated_name(&template.declared_name, bindings);
        if declared.iter().any(|name| name == &target) {
            continue;
        }
        declared.push(target.clone());
        program.push(instantiate_declaration(
            &template.declaration,
            &target,
            bindings,
        ));
        added += 1;
    }
    (program, added)
}

/// Collects every class and interface name the program declares, namespace blocks included.
fn collect_declared_names(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. } => {
                names.push(name.clone())
            }
            StmtKind::NamespaceBlock { body, .. } => collect_declared_names(body, names),
            _ => {}
        }
    }
}

/// Clones a template under `target`, substituting its type parameters everywhere.
///
/// The clone keeps its INHERITED type arguments (`class Repo<T> implements Finder<T>` becomes
/// `implements Finder<int>`, still a generic mention) so the next round instantiates the parent
/// too. What it loses is its own `type_params`: the copy is an ordinary class, and leaving them
/// behind would make the next round treat it as a template and strip it.
fn instantiate_declaration(template: &Stmt, target: &str, bindings: &Bindings) -> Stmt {
    let mut declaration = template.clone();
    match &mut declaration.kind {
        StmtKind::ClassDecl { name, generics, .. }
        | StmtKind::InterfaceDecl { name, generics, .. } => {
            *name = target.to_string();
            if let Some(declared) = generics.take() {
                let GenericDecl {
                    type_params: _,
                    extends_args,
                    interface_args,
                } = *declared;
                *generics = GenericDecl::new(Vec::new(), extends_args, interface_args);
            }
        }
        _ => {}
    }
    // One substitution helper for classes and functions alike: the walker reaches every type
    // position in the declaration, members included, so a parameter cannot survive in a corner
    // the instantiation forgot.
    substitute_in_body(vec![declaration], bindings)
        .pop()
        .expect("substituting one declaration yields one declaration")
}

/// Removes every generic class and interface template from the program.
///
/// A template is not a class: its annotations name types that have no representation, so the
/// checker must never see it. Called once the fixpoint has produced every instantiation the
/// program reaches.
fn strip_templates(program: Program) -> Program {
    program
        .into_iter()
        .filter_map(|stmt| {
            let is_template = match &stmt.kind {
                StmtKind::ClassDecl { generics, .. } | StmtKind::InterfaceDecl { generics, .. } => {
                    generics
                        .as_ref()
                        .is_some_and(|generics| !generics.type_params.is_empty())
                }
                _ => false,
            };
            if is_template {
                return None;
            }
            Some(strip_in_namespace(stmt))
        })
        .collect()
}

/// Applies [`strip_templates`] inside a namespace block, and returns anything else unchanged.
fn strip_in_namespace(stmt: Stmt) -> Stmt {
    let mut stmt = stmt;
    if let StmtKind::NamespaceBlock { body, .. } = &mut stmt.kind {
        *body = strip_templates(std::mem::take(body));
    }
    stmt
}

/// Returns a span from the deepest instantiation the runaway produced, for the round-cap error.
///
/// `Span::dummy()` would lose the file, and a program that never settles is exactly the one a
/// programmer needs pointed at: the last declaration appended is the deepest instantiation, and
/// its span is the mention that asked for it.
fn runaway_span(program: &Program) -> Span {
    program
        .last()
        .map(|stmt| stmt.span)
        .unwrap_or_else(Span::dummy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses PHP source into a resolved program, the shape this pass is handed.
    fn resolved_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        let program = crate::parser::parse(&tokens).expect("parse failed");
        crate::name_resolver::resolve(program).expect("name resolution failed")
    }

    /// Returns the names of every class and interface the program declares.
    fn declared(program: &Program) -> Vec<String> {
        let mut names = Vec::new();
        collect_declared_names(program, &mut names);
        names
    }

    /// Two type arguments produce two classes, and the template is gone.
    #[test]
    fn test_two_type_arguments_produce_two_classes() {
        let program = resolved_program(
            "<?php class Box<T> { private T $v; } Box<int> $a = 1; Box<string> $b = 2;",
        );
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        let mut names = declared(&program);
        names.sort();
        assert_eq!(names, vec!["Box<int>".to_string(), "Box<string>".to_string()]);
    }

    /// The instantiation is named from the template's DECLARED spelling, not the mention's.
    ///
    /// PHP class names are case-insensitive, so two mentions spelled differently must name one
    /// class — and `get_class()` must report what the programmer declared.
    #[test]
    fn test_instantiation_uses_the_declared_spelling() {
        let program = resolved_program("<?php class Box<T> { private T $v; } BOX<int> $a = 1;");
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        assert_eq!(declared(&program), vec!["Box<int>".to_string()]);
    }

    /// A template mentioning another template instantiates both, in successive rounds.
    #[test]
    fn test_a_template_mentioning_another_instantiates_both() {
        let program = resolved_program(
            "<?php class Box<T> { private T $v; } \
             class Wrapper<T> { private Box<T> $inner; } \
             Wrapper<int> $w = 1;",
        );
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        let mut names = declared(&program);
        names.sort();
        assert_eq!(
            names,
            vec!["Box<int>".to_string(), "Wrapper<int>".to_string()]
        );
    }

    /// A mention inside a template is NOT instantiated at the literal parameter: `Box<T>` has
    /// no meaning until `T` is bound, and emitting `Box<T>` would give it a property typed `T`.
    #[test]
    fn test_a_mention_inside_a_template_is_not_instantiated_at_the_parameter() {
        let program = resolved_program(
            "<?php class Box<T> { private T $v; } class Wrapper<T> { private Box<T> $inner; }",
        );
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        assert!(
            declared(&program).is_empty(),
            "nothing mentions either template, so neither is instantiated"
        );
    }

    /// A trailing parameter with no argument falls back to its default.
    #[test]
    fn test_type_parameter_default_fills_a_missing_argument() {
        let program = resolved_program(
            "<?php class Pair<K, V = string> { private K $k; private V $v; } Pair<int> $p = 1;",
        );
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        assert_eq!(declared(&program), vec!["Pair<int, string>".to_string()]);
    }

    /// A bound produces an obligation for the checker rather than a decision here.
    #[test]
    fn test_a_bound_records_an_obligation() {
        let program = resolved_program(
            "<?php class Vault<T : Entity> { private T $v; } Vault<User> $x = 1;",
        );
        let templates = collect(&program);
        let obligations = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").obligations;
        assert_eq!(obligations.len(), 1);
        assert_eq!(obligations[0].parameter, "T");
        // Compared by spelling: the name resolver has already canonicalized both, so the
        // `Name` carries a fully-qualified kind that the written form does not.
        assert_eq!(crate::generics::describe_type(&obligations[0].bound), "Entity");
        assert_eq!(crate::generics::describe_type(&obligations[0].argument), "User");
    }

    /// An unbounded parameter records nothing: there is nothing for the checker to answer.
    #[test]
    fn test_an_unbounded_parameter_records_no_obligation() {
        let program = resolved_program("<?php class Box<T> { private T $v; } Box<int> $a = 1;");
        let templates = collect(&program);
        let obligations = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").obligations;
        assert!(obligations.is_empty());
    }

    /// Type arguments on a class that declares none are rejected, not silently dropped.
    #[test]
    fn test_type_arguments_on_a_non_generic_class_are_rejected() {
        let program = resolved_program("<?php class Plain { public int $x = 1; } Plain<int> $p = 1;");
        let templates = collect(&program);
        let error = instantiate(program, &templates, &InferredConstructions::default()).expect_err("expected a rejection");
        assert!(
            error.message.contains("declares no type parameters"),
            "unexpected error: {}",
            error.message
        );
    }

    /// More arguments than the template declares is an error, not a truncation.
    #[test]
    fn test_too_many_type_arguments_are_rejected() {
        let program = resolved_program(
            "<?php class Pair<A, B> { private A $a; private B $b; } Pair<int, string, bool> $p = 1;",
        );
        let templates = collect(&program);
        let error = instantiate(program, &templates, &InferredConstructions::default()).expect_err("expected a rejection");
        assert!(
            error.message.contains("takes 2 type argument"),
            "unexpected error: {}",
            error.message
        );
    }

    /// A parameter with neither an argument nor a default has nothing to bind.
    #[test]
    fn test_a_missing_argument_without_a_default_is_rejected() {
        let program = resolved_program(
            "<?php class Pair<A, B> { private A $a; private B $b; } Pair<int> $p = 1;",
        );
        let templates = collect(&program);
        let error = instantiate(program, &templates, &InferredConstructions::default()).expect_err("expected a rejection");
        assert!(
            error.message.contains("has no default"),
            "unexpected error: {}",
            error.message
        );
    }

    /// A program with no generic declaration comes back untouched and with no obligations.
    #[test]
    fn test_an_ordinary_program_is_unchanged() {
        let program = resolved_program("<?php class Plain { public int $x = 1; } echo 1;");
        let before = declared(&program);
        let templates = collect(&program);
        let run = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed");
        let (program, obligations) = (run.program, run.obligations);
        assert_eq!(declared(&program), before);
        assert!(obligations.is_empty());
    }

    /// A generic class declared inside a namespace block is found and instantiated under its
    /// canonical name.
    #[test]
    fn test_a_namespaced_template_is_collected() {
        let program = resolved_program(
            "<?php namespace App; class Box<T> { private T $v; } Box<int> $a = 1;",
        );
        let templates = collect(&program);
        let program = instantiate(program, &templates, &InferredConstructions::default()).expect("instantiation failed").program;
        assert_eq!(declared(&program), vec!["App\\Box<int>".to_string()]);
    }
}
