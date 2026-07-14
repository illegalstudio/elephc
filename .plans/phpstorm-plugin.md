# JetBrains Plugin for elephc

## Context

elephc compiles PHP to native ARM64 binaries and extends PHP with two families
of non-standard syntax:

1. **`extern`**: FFI declarations for functions, classes, globals, and named
   library blocks.
2. **`ptr` / `ptr<T>`**: opaque pointer types and builtins such as `ptr()`,
   `ptr_cast<T>()`, `ptr_null()`, and related helpers.

PhpStorm does not recognize these extensions and reports errors throughout
elephc code. The plugin should integrate the extensions without losing native
PHP support.

## Architectural Approach: PHP Plugin Extension

Do **not** implement a custom language or language injection. The plugin should
extend the existing PHP plugin with:

- stub files for builtins, removing "undefined function" errors;
- `HighlightInfoFilter`, suppressing parse errors for `extern` and
  `ptr_cast<T>()`;
- completion contributor for custom keywords and types;
- file-based index for project `extern` declarations;
- type provider for `ptr_cast<T>()` return types.

### Why Not Other Paths

| Approach | Problem |
|---|---|
| Custom language | Loses all PHP intelligence; elephc is 95% standard PHP. |
| Language injection | Intended for embedded languages such as SQL in strings, not syntax extensions. |
| `.elephc` file type | Users write `.php`; changing extensions breaks the ecosystem. |

## Plugin Structure

```text
elephc-intellij-plugin/
├── build.gradle.kts
├── src/main/resources/
│   ├── META-INF/plugin.xml
│   └── stubs/_elephc_stubs.php
└── src/main/kotlin/com/illegalstudio/elephc/
    ├── stubs/ElephcStubLibraryProvider.kt
    ├── suppress/ElephcHighlightFilter.kt
    ├── completion/ElephcCompletionContributor.kt
    ├── index/ElephcExternIndex.kt
    ├── types/ElephcTypeProvider.kt
    ├── run/ElephcRunConfiguration.kt
    └── settings/ElephcSettingsConfigurable.kt
```

### plugin.xml

```xml
<idea-plugin>
    <id>com.illegalstudio.elephc</id>
    <name>elephc - PHP Native Compiler</name>
    <depends>com.intellij.modules.platform</depends>
    <depends>com.jetbrains.php</depends>

    <extensions defaultExtensionNs="com.intellij">
        <daemon.highlightInfoFilter
            implementation="com.illegalstudio.elephc.suppress.ElephcHighlightFilter"/>
        <completion.contributor language="PHP"
            implementation="com.illegalstudio.elephc.completion.ElephcCompletionContributor"/>
        <fileBasedIndex
            implementation="com.illegalstudio.elephc.index.ElephcExternIndex"/>
    </extensions>

    <extensions defaultExtensionNs="com.jetbrains.php">
        <libraryRoot
            implementation="com.illegalstudio.elephc.stubs.ElephcStubLibraryProvider"/>
        <typeProvider4
            implementation="com.illegalstudio.elephc.types.ElephcTypeProvider"/>
    </extensions>
</idea-plugin>
```

## Components

### 1. Stub Library

Bundle `_elephc_stubs.php` with declarations for all `ptr_*` functions:

```php
<?php
// elephc compiler intrinsics - IDE support only

/** @return int Raw pointer address of $var */
function ptr(mixed &$var): int { return 0; }
function ptr_null(): int { return 0; }
function ptr_is_null(int $ptr): bool { return true; }
function ptr_get(int $ptr): int { return 0; }
function ptr_set(int $ptr, int|bool|null $value): void {}
function ptr_read8(int $ptr): int { return 0; }
function ptr_read32(int $ptr): int { return 0; }
function ptr_write8(int $ptr, int $value): void {}
function ptr_write32(int $ptr, int $value): void {}
function ptr_offset(int $ptr, int $bytes): int { return 0; }
function ptr_sizeof(string $type): int { return 0; }

/** Opaque pointer type for type hints */
class ptr {}
```

Register it through `PhpAdditionalLibraryRootsProvider` so PhpStorm indexes it
automatically.

Signature references:

- `src/types/checker/builtins/pointers.rs`;
- `src/types/checker/builtins/catalog.rs`.

These files contain types, catalog entries, and validation for each `ptr_*`
builtin.

### 2. HighlightInfoFilter

Implement `com.intellij.daemon.impl.HighlightInfoFilter`. Suppress error
highlights when:

1. A line starts with `extern` for functions, classes, globals, or named
   library blocks.
2. `ptr_cast<T>(...)` is parsed by PHP as comparisons. Recognize the pattern
   with `ptr_cast\s*<\s*\w+\s*>\s*\(`.
3. `ptr<T>` appears in type hints and angle brackets cause syntax errors.

This is the most critical component. Without it, the IDE is unusable on elephc
code.

### 3. Extern Declaration Index

Implement a `FileBasedIndexExtension` that scans `.php` files for `extern`
declarations. PHP PSI cannot be used because it does not parse `extern`, so use
a small regex parser:

- `extern\s+function\s+(\w+)\s*\(([^)]*)\)\s*:\s*(\w+)` for one function;
- `extern\s+"([^"]+)"\s*\{` for a library block start;
- `extern\s+class\s+(\w+)\s*\{` for an extern class;
- `extern\s+global\s+(\w+)\s+\$(\w+)` for an extern global.

The index feeds completion and navigation.

Exact syntax reference: `src/parser/stmt/ffi.rs`.

### 4. Completion Contributor

- After `ext`, suggest `extern`.
- After `extern`, suggest `function`, `class`, `global`, and `"` for a library
  name.
- In type-hint position, suggest `ptr` and `ptr<ClassName>` using classes from
  the extern index.
- After `ptr_cast<`, suggest extern class names.

### 5. Type Provider

Implement `PhpTypeProvider4` to resolve the return type of `ptr_cast<T>()`.
Because angle brackets break PHP PSI, inspect raw text around the expression and
return `ptr` as the type.

## Implementation Phases

### MVP: Remove Noise

1. Set up the Gradle project with a `com.jetbrains.php` dependency.
2. Create and bundle `_elephc_stubs.php`.
3. Implement `ElephcStubLibraryProvider`.
4. Implement `ElephcHighlightFilter`.
5. Test against `examples/ffi/main.php` and other project examples.

### Phase 2: Intelligence

6. Implement `ElephcExternIndex` for extern declaration indexing.
7. Implement `ElephcCompletionContributor` for keywords and types.
8. Add reference resolution: Ctrl-click on extern calls should navigate to the
   declaration.
9. Implement `ElephcTypeProvider` for `ptr_cast<T>()`.

### Phase 3: Developer Experience

10. Add a run configuration that compiles and runs the binary.
11. Parse elephc errors into clickable `line:col` links.
12. Add gutter icons on extern declarations.
13. Add a settings page for the elephc binary path.

## Main Challenges

| Challenge | Impact | Mitigation |
|---|---|---|
| PHP PSI does not parse `extern`. | No structured PSI for extern declarations. | Custom regex parser in the file-based index. |
| `ptr_cast<T>()` breaks the PHP parser. | PHP interprets `<` and `>` as comparisons. | Highlight filter plus text-level regex recognition. |
| `ptr<T>` in type hints. | Angle brackets are invalid in type position. | Error suppression plus stub class `ptr` for the simple case. |
| PHP plugin APIs can be unstable. | PhpStorm versions may break the plugin. | Declare a narrow `since-build`/`until-build` range and use stable APIs only. |

## elephc Codebase References

- `src/parser/ast/stmt.rs`, `src/parser/ast/expr.rs`,
  `src/parser/ast/ffi.rs`: custom AST nodes such as `ExternFunctionDecl`,
  `PtrCast`, and `CType`.
- `src/parser/stmt/ffi.rs`: extern declaration parsing.
- `src/parser/expr/prefix_complex.rs`: `ptr_cast<T>()` parsing.
- `src/types/checker/builtins/pointers.rs` and
  `src/types/checker/builtins/catalog.rs`: signatures, catalog entries, and
  validation for `ptr_*` builtins.
- `examples/ffi/main.php`: canonical example that exercises extern syntax.

## Verification

1. Open the elephc project in PhpStorm with the plugin installed.
2. Verify that `examples/ffi/main.php` has no red errors on `extern` and
   `ptr_cast`.
3. Verify that `ptr_null()`, `ptr_get()`, and related helpers get completion and
   documentation.
4. Ctrl-click an extern call and verify navigation to its declaration.
5. Compile and run an example from the run configuration.
