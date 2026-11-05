---
title: "Password builtins"
description: "Builtins in the Password category."
sidebar:
  order: 124
---

## Password builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`password_algos()`](./password/password_algos.md) | `(): array` | `array` | ✓ | - |
| [`password_get_info()`](./password/password_get_info.md) | `(string $hash): array` | `array` | ✓ | - |
| [`password_hash()`](./password/password_hash.md) | `(string $password, string $algo = PASSWORD_DEFAULT, array $options = []): string|false` | `string|false` | ✓ | - |
| [`password_needs_rehash()`](./password/password_needs_rehash.md) | `(string $hash, string $algo, array $options = []): bool` | `bool` | ✓ | - |
| [`password_verify()`](./password/password_verify.md) | `(string $password, string $hash): bool` | `bool` | ✓ | - |
