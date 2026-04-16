---
title: "Your First Program"
description: "Write, compile, and run your first PHP program with elephc."
sidebar:
  order: 2
---

## Hello, World

Create a file called `hello.php`:

```php
<?php
echo "Hello, World!\n";
```

Compile it:

```bash
elephc hello.php
```

This produces a native binary called `hello` in the same directory. Run it:

```bash
./hello
```

```
Hello, World!
```

That's it — a standalone native binary, no PHP interpreter needed.

## A slightly bigger example

Create `greet.php`:

```php
<?php
if ($argc < 2) {
    echo "Usage: ./greet <name>\n";
    exit(1);
}

$name = $argv[1];
echo "Hello, " . strtoupper($name) . "!\n";
```

Compile and run:

```bash
elephc greet.php
./greet elephc
```

```
Hello, ELEPHC!
```

The program reads command-line arguments through `$argc` and `$argv`, just like PHP.

## FizzBuzz

A classic example to see variables, loops, and conditionals in action:

```php
<?php
for ($i = 1; $i <= 100; $i++) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i . "\n";
    }
}
```

```bash
elephc fizzbuzz.php
./fizzbuzz
```

## What happens under the hood

When you run `elephc hello.php`, the compiler:

1. **Lexes** the source into tokens
2. **Parses** tokens into an AST (Abstract Syntax Tree)
3. **Resolves** includes and namespaces
4. **Type-checks** the program
5. **Generates** assembly for the selected target
6. **Assembles** the `.s` file with `as`
7. **Links** the `.o` file with `ld` into a native executable

The intermediate `.s` and `.o` files are cleaned up automatically. You're left with a single executable.

## Next steps

- Browse the [PHP syntax reference](../php/types.md) to see what's supported
- Check out the [examples](https://github.com/illegalstudio/elephc/tree/main/examples) for more programs
- If you need FFI, game loops, or raw memory access, see [Beyond PHP](../beyond-php/pointers.md)
