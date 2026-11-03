---
title: "hash_pbkdf2()"
description: "Generates a PBKDF2 key derivation of a supplied password."
sidebar:
  order: 918
---

## hash_pbkdf2()

```php
function hash_pbkdf2(string $algo, string $password, string $salt, int $iterations, int $length = 0, bool $binary = false, array $options = []): string
```

Generates a PBKDF2 key derivation of a supplied password.

**Parameters**:
- `$algo` (`string`)
- `$password` (`string`)
- `$salt` (`string`)
- `$iterations` (`int`)
- `$length` (`int`), default `0`, optional
- `$binary` (`bool`), default `false`, optional
- `$options` (`array`), default `[]`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected hash_pbkdf2 prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_pbkdf2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_pbkdf2.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_pbkdf2` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_pbkdf2.md).
