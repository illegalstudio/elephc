---
title: "password_hash()"
description: "Creates a bcrypt password hash with a random salt."
sidebar:
  order: 1023
---

## password_hash()

```php
function password_hash(string $password, mixed $algo, array $options = []): string
```

Creates a bcrypt password hash with a random salt.

**Parameters**:
- `$password` (`string`)
- `$algo` (`mixed`)
- `$options` (`array`), default `[]`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected password prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/password_hash.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_hash.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/password_hash.md).
