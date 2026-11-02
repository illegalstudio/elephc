---
id: decision-a-method-exists-test-in-the-same-body-admits-the-461773c5
type: decision
title: "A method_exists test in the same body admits the call it guards"
description: "Twig calls a method no bundled class declares behind method_exists; the checker refused the file instead of compiling the dead branch"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/objects/methods.rs
    blob: 41165febac884fb04decd20c4bc0e74b01afe31e
  - path: src/types/checker/stmt_check/narrowing.rs
    blob: 340774e5f89d92493e806844a87aea0557bd577a
  - path: src/types/checker/mod.rs
    blob: eb476f7c71df0cd0c202ac9bd09bdbfdd04ed330
---

# A method_exists test in the same body admits the call it guards

## Fact

A `method_exists()` test in the same body admits the call it guards, instead of refusing it.

`infer_lenient_subtype_method_call` already accepts a method the nominal receiver does not declare
when SOME compatible concrete class does — PHP dispatches on the runtime class and the closed-world
backend mirrors that with a class-id switch. It refused when NOTHING in the program declares the
method, which is right for a typo and wrong for the one shape that says so out loud:

```php
public static function getOperatorTokensFor(ExpressionParserInterface $parser): array
{
    if (method_exists($parser, 'getOperatorTokens')) {
        return $parser->getOperatorTokens();
    }
    trigger_deprecation('twig/twig', '3.24', ...);
    return [$parser->getName(), ...$parser->getAliases()];
}
```

Twig ships this so a 4.0 interface method can be adopted early; no bundled parser implements it yet,
so `php -n` never enters the branch. It was the LAST compile error left in Twig, and a refused file
falls back to the interpreter — the opposite of maximizing compiled surface.

The admission is narrow by construction. It needs a `method_exists()` call written in the SAME
function body (keyed by `current_loop_storage_scope`, the scope key `flow_typed_returns` uses),
naming the SAME receiver place (`guard_env_key`, so `$parser`, `$this->parser` and `self::$parser`
are separate facts) and the SAME method as a literal string. Two negatives pin it:
`test_undefined_method_without_a_guard_is_still_refused` and
`test_method_exists_guard_does_not_admit_a_different_receiver`.

Facts are NOT flow-scoped to the `if` branch. A body that tests for a method has already declared it
does not know, and the checker's branch narrowing carries types, not capabilities — adding a
capability dimension to it would be a much larger change for no extra soundness here.

If the guarded call is somehow reached with no candidate class,
`lower_narrowed_interface_method_call` emits `Call to undefined method X::y()` and fatals — which is
what the interpreter does on the same source.
