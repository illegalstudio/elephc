---
title: "random_bytes()"
description: "Generates cryptographically secure pseudo-random bytes."
sidebar:
  order: 596
---

## random_bytes()

```php
function random_bytes(int $length): string
```

Generates cryptographically secure pseudo-random bytes.

**Parameters**:
- `$length` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/random_bytes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/random_bytes.rs)).

**Examples**:

```php
// Full example: examples/versions-and-tokens/main.php
$token = bin2hex(random_bytes(16));  // 32 hex characters
```

## Internals

For how `random_bytes` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_bytes.md).
