# elephc Wiki

A guide to understanding how elephc works — from PHP source code to native ARM64 binary. Written for anyone who wants to learn how compilers work, using a real, working compiler as the reference.

## Start here

- **[What is a Compiler?](what-is-a-compiler.md)** — What happens when source code becomes a program. The big picture of compilation, interpretation, and why elephc exists.

## ARM64 Assembly

- **[Introduction to ARM64 Assembly](arm64-assembly.md)** — What assembly language is, how the CPU executes it, registers, memory, and the mental model you need before reading compiler output.
- **[ARM64 Instruction Reference](arm64-instructions.md)** — The specific ARM64 instructions elephc uses, organized by category. Each one explained with examples.

## How elephc works

- **[The Pipeline](how-elephc-works.md)** — The full journey from `<?php echo "hello";` to a running binary. Overview of each compilation phase and how they connect.
- **[The Lexer](the-lexer.md)** — How raw text becomes tokens. Cursor, scanning, keywords, string interpolation.
- **[The Parser](the-parser.md)** — How tokens become an AST. Pratt parsing, binding power, operator precedence.
- **[The Type Checker](the-type-checker.md)** — How elephc infers and validates types at compile time, without PHP's runtime type system.
- **[The Code Generator](the-codegen.md)** — How AST nodes become ARM64 assembly. Expressions, statements, function calls, and the push/pop pattern.
- **[The Runtime](the-runtime.md)** — The hand-written assembly routines that handle string conversion, concatenation, arrays, exception unwinding, and I/O at runtime.
- **[Memory Model](memory-model.md)** — Stack frames, heap allocation, the string buffer, array layout, and how elephc manages memory with reference counting plus targeted cycle collection.

## Reference

- **[Language Reference](language-reference.md)** — What PHP features elephc supports: types, operators, control flow, functions, built-ins.
- **[Architecture](architecture.md)** — Module map, calling conventions, pipeline diagram (technical reference).
