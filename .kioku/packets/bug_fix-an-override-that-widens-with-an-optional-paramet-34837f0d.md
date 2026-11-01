---
id: bug_fix-an-override-that-widens-with-an-optional-paramet-34837f0d
type: bug_fix
title: "An override that WIDENS with an optional parameter never applies its default - and that is what blocks twig's IncludeNode"
description: "Found while trying to accept extra arguments at a call site; the override's extra slot exists but is left empty instead of taking the declared default"
created: 2026-09-18
verified_by: "A 25-line reduction: php -n prints embed:c2/dflt, elephc prints embed:c2/ . The checker relaxation that exposed the second half was reverted; error_tests stayed at 1431/99 with identical names"
sources:
  - path: src/codegen/lower_inst/method_resolution.rs
    blob: 27dc8b6cc99c8097c60b24cba07b413ebac3ce54
  - path: src/types/checker/functions/call_validation.rs
    blob: 4b8990da20d59f4d2fe4c98f7679ee2173df29e4
---

# An override that WIDENS with an optional parameter never applies its default - and that is what blocks twig's IncludeNode

## Fact

NOT FIXED. Recorded with its reduction so the next attempt starts from the right place.

REDUCTION, 25 lines, no framework:

    class Base  { protected function m(string $c): string { return 'include:'.$c; }
                  public function go(string $c): string { return $this->m($c); } }
    class Child extends Base {
        protected function m(string $c, string $t = 'dflt'): string { return 'embed:'.$c.'/'.$t; } }

    (new Child())->go('c2');     // php: embed:c2/dflt     elephc: embed:c2/

Widening an override with an OPTIONAL parameter is legal php and elephc accepts the declaration.
The call goes through the vtable with the BASE's one-argument ABI, the override's second slot
exists -- there is no crash, the value is an empty string, not garbage -- and nothing ever writes
its declared default into it.

THE SECOND HALF, and why the two must be fixed together. `twig/twig`'s `IncludeNode` declares
`addGetTemplate(Compiler $compiler/* , string $template = '' */)` with the second parameter
COMMENTED OUT for backwards compatibility, `EmbedNode` overrides it with two, and `IncludeNode`
itself calls it with TWO arguments. php allows passing more arguments than a userland function
declares; elephc refuses at the checker
(`Method Twig\Node\IncludeNode::addGetTemplate expects 1 arguments, got 2`).

Relaxing that check was tried: `check_known_callable_call_with_options` already carries an unused
`callee_arity_is_advisory` flag, and turning it on when a subclass declares more parameters makes
the checker accept the call. The BACKEND then refuses cleanly --
`method call to IncludeNodeLike::addGetTemplate with 3 operands for 2 ABI params`
(`resolve_method_call_target` sizes the ABI from `callee_sig.params.len() + 1`) -- so nothing
miscompiles, but nothing is gained either. REVERTED: it only moves the diagnostic and weakens a
real one.

WHAT A REAL FIX LOOKS LIKE: size a method's ABI by the WIDEST signature in its dispatch family, and
initialise every slot past the caller's argument count from the RUNTIME implementation's defaults.
Both halves live around `resolve_method_call_target` and the method prologue.

## Why

It is the floor under the last twig blocker: accepting the extra argument is pointless until the callee's own default works
