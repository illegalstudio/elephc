---
id: gotcha-a-class-without-its-dependencies-gets-a-fabricat-29546389
type: gotcha
title: "A class without its dependencies gets a fabricated type, not Mixed - three places, three refused programs"
description: "Twig Compiler write typed null|bool, a twig-bridge array literal typed array<int> over mixed elements, and interface_exists(Foo::class) answering false; all three refused code PHP runs"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/objects/methods.rs
    blob: 41165febac884fb04decd20c4bc0e74b01afe31e
  - path: src/ir_lower/expr/indexed_array_literals.rs
    blob: 5d691fed7c6d804db40c5776c7c0970434af7c43
  - path: src/autoload/walk.rs
    blob: 4c2970949a00632386f187b9b8030b1b927bd444
---

# A class without its dependencies gets a fabricated type, not Mixed - three places, three refused programs

## Fact

Three places fabricated a type instead of answering "unknown", and each one refused a program PHP runs.

They are the same defect seen from three angles: a class enters the compiled world without its
dependencies — signature type names deliberately do not autoload (`collect_callable_signature`), and
an app may simply not install a package the framework references — and the checker then invents an
answer rather than degrading to `Mixed`.

**1. A Mixed receiver's method return.** `mixed_receiver_method_return_type` unions the return type
of every class declaring a method of that name. It already collapsed to `Mixed` when an OBJECT was
involved; a union of NON-object members was still handed back. Traced on Symfony's `--web` build:

    [cls] Twig\Compiler::addDebugInfo known=false -> Mixed
    [recv] ->string() on Union([Void, Bool])
    error: Nullsafe method call requires an object or null, got null|bool

`Twig\Compiler::write()` returns `$this` and was never a candidate, because its class is absent.
Candidates that DISAGREE are methods that share a name, not a fact about this call. Codegen boxes a
union exactly as it boxes a Mixed, so only the static reading changes.

**2. An array literal's element storage.** `array_literal_element_type_for_ir` fell back to
`infer_expr_type_syntactic`, which answers `Int` for everything it does not recognise. As a STORAGE
stamp that makes the backend read a boxed pointer as an integer. Symfony's twig-bridge built
`[$view->vars['id'], $view->vars['name']]` over a `FormView` absent from the world (symfony/form is
not installed in the fixture app) and got `array<int>` over elements the same IR typed `mixed`;
`str_replace` refused it. The `ArrayAccess` and `PropertyAccess` arms now answer `Mixed` when their
resolver cannot answer.

**3. `interface_exists(Foo::class)`.** The autoload walk treats these four functions as compile-time
demands — PHP really does run the loader for them — but only recognised a STRING LITERAL name.
`symfony/string`'s `AsciiSlugger.php` guards its entire file with
`interface_exists(LocaleAwareInterface::class)` and throws "the symfony/translation-contracts
package is not installed" otherwise. The package IS installed; the interface just never entered the
world, and every `--web` worker exited 255 at boot. A bare `Foo::class` still does not autoload —
the general walker keeps ignoring it — this is the same exception attribute metadata already gets.

Fixing 1 removed the need to preload `Twig\Compiler` by hand; fixing 1+2+3 took the Symfony
`--web` build from a checker refusal all the way to a linked 212 MB binary in 2m16s.

Recipe when a refusal names a type that looks wrong: check whether the class involved is in the
closed world at all (`self.classes.contains_key`), before looking for a bug in the rule that
refused it.
