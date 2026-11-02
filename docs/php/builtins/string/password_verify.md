---
title: "password_verify()"
description: "Verifies that a password matches a bcrypt hash."
sidebar:
  order: 1021
---

## password_verify()

```php
function password_verify(string $password, string $hash): bool
```

Verifies that a password matches a bcrypt hash.

**Parameters**:
- `$password` (`string`)
- `$hash` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected password prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/password_verify.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_verify.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_verify` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/password_verify.md).
