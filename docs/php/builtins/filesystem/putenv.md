---
title: "putenv()"
description: "Sets an environment variable, or removes it when the argument has no equals sign."
sidebar:
  order: 291
---

## putenv()

```php
function putenv(string $assignment): bool
```

Sets an environment variable, or removes it when the argument has no equals sign.

**Parameters**:
- `$assignment` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/putenv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/putenv.rs)).

**Examples**:

```php
putenv("APP_ENV=production");
echo getenv("APP_ENV") . "\n";
putenv("APP_ENV");
echo getenv("APP_ENV") === false ? "unset\n" : "still set\n";
```

## Internals

For how `putenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/putenv.md).
