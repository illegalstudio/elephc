---
title: "password_algos()"
description: "Returns the identifiers of the available password hashing algorithms (bcrypt)."
sidebar:
  order: 1017
---

## password_algos()

```php
function password_algos(): array
```

Returns the identifiers of the available password hashing algorithms (bcrypt).

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected password prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/password_algos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_algos.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_algos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/password_algos.md).
