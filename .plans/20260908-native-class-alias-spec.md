# Native class alias semantics — draft, not locked

## Evidence and scope

The foreign-frame reflection regression reports `ForeignAlias` instead of the
declared `ForeignClass`. This is not a spelling-only defect:

- `src/autoload/alias.rs` synthesizes subclasses for static alias calls. The
  first pass removes some calls; the post-resolution pass retains other calls
  while appending declarations outside their original control flow.
- `src/builtins/callables/class_alias.rs` rejects runtime-dependent class names.
- `lower_class_alias` in `src/codegen/lower_inst/builtins/types.rs` returns false
  unconditionally for retained calls.
- Native reflection rows faithfully describe those synthetic subclasses, so
  rebinding or changing the displayed name cannot recover true alias identity.
- Eval has alias mappings, but its current helper ignores the autoload argument.
  It is therefore not an independently established complete PHP reference.

Do not infer aliases from empty subclasses: genuine subclasses are valid PHP.
Do not hide the defect with a Reflection-only name substitution.

## Behavioral requirements

1. Preserve the original call, argument evaluation order, boolean result,
   execution point, conditional execution and request lifetime.
2. An alias refers to the same class entry: object identity/type tests, late
   static class names, inheritance, static storage and Reflection must agree.
3. Support class-like kinds according to the frozen PHP baseline, including
   final classes, interfaces, traits and enums. An alias is not inheritance.
4. Preserve canonical declaration spelling while applying PHP's name matching
   rules to lookups and leading separators.
5. Honor autoload control, alias collisions and source lookup failures. Preserve
   diagnostics and side effects; do not substitute unconditional false.
6. Preserve native behavior for known targets and use genuine runtime alias
   state for dynamic names. No framework/tool-specific names or predicates.
7. Keep every supported target coherent. Compiler inference must not turn a
   conditional alias into an unconditional runtime declaration or incorrectly
   fold `instanceof`, class-existence queries or reflection results.
8. Reset request-local activation state correctly in repeated web requests.

## Design boundary to investigate before choosing an implementation

Separate compiler knowledge of a possible alias from runtime activation of that
alias. Alias provenance must be explicit, not encoded as `ClassDecl extends`.
Map the consumers of the current synthesized declarations: autoload discovery,
name resolution, checker lookup, nominal guards, instantiation, class constants,
static members, dispatch, reflection, optimizer and runtime request reset.

Compare the existing eval alias resolver and native class-name lookup paths.
Reuse shared name/argument rules, but do not assume the eval implementation is
fully compliant. Do not introduce new PHP interpretation for statically known
operations simply to avoid implementing the EIR/runtime boundary.

## TDD and audit gates

- Preserve the existing failing foreign-frame alias regression.
- Add first-call success/duplicate behavior, before/after and conditional
  activation, canonical name, real parent, symmetric type tests and shared
  static storage cases.
- Include final class, interface, trait, enum, alias chains, case variants and
  autoload true/false cases. Test runtime names as well as literal names.
- Replay inherited methods/properties/constants and optimizer on/off behavior.
- Validate supported target emitters plus appropriate native executable tests.
- Exercise repeated HTTP requests without changing application/vendor sources.
- Freeze and inspect actual php-src, then obtain the requested reviewers'
  agreement on the exact spec/revision. No such consensus is claimed here.
- Changes to the builtin binding require the update-builtin-docs workflow before
  declaring the change ready; this document does not complete that gate.

## PHP smoke evidence

PHP CLI 8.5.6 accepts an alias of a final class. A newly constructed alias object
reports the original class, its parent is the original class's parent, and an
original object satisfies the alias type. Static writes are shared. Interface,
trait and enum aliases are recognized by their corresponding existence queries;
enum cases preserve strict identity. These probes establish expected cases, not
an exhaustive php-src parity verdict. The source revision audit remains open.
