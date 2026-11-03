---
title: "password_needs_rehash()"
description: "Reports whether a hash does not match the given algorithm and options."
sidebar:
  order: 1024
---

## password_needs_rehash()

```php
function password_needs_rehash(string $hash, mixed $algo, array $options = []): bool
```

Reports whether a hash does not match the given algorithm and options.

**Parameters**:
- `$hash` (`string`)
- `$algo` (`mixed`)
- `$options` (`array`), default `[]`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected password prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/password_needs_rehash.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_needs_rehash.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_needs_rehash` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/password_needs_rehash.md).
