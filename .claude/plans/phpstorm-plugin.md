# JetBrains Plugin per elephc

## Context

elephc compila PHP a binari nativi ARM64 ed estende PHP con due famiglie di sintassi non-standard:
1. **`extern`** — dichiarazioni FFI (funzioni, classi, globali, blocchi raggruppati con nome libreria)
2. **`ptr` / `ptr<T>`** — tipo puntatore opaco, funzioni built-in (`ptr()`, `ptr_cast<T>()`, `ptr_null()`, ecc.)

PhpStorm non riconosce queste estensioni e segna errori ovunque. Serve un plugin che le integri senza perdere il supporto PHP nativo.

## Approccio architetturale: PHP Plugin Extension

**Non** un linguaggio custom, **non** language injection. Il plugin **estende** il plugin PHP esistente con:
- Stub file per le funzioni built-in → elimina errori "undefined function"
- `HighlightInfoFilter` → sopprime errori di parsing su `extern` e `ptr_cast<T>()`
- Completion contributor → autocompletamento per keyword e tipi custom
- File-based index → indicizza le dichiarazioni `extern` nel progetto
- Type provider → risolve tipi di ritorno per `ptr_cast<T>()`

### Perché non altre strade

| Approccio | Problema |
|---|---|
| Linguaggio custom | Perde tutta l'intelligenza PHP — elephc è 95% PHP standard |
| Language injection | È per linguaggi embedded (SQL in stringhe), non estensioni sintattiche |
| File type `.elephc` | Gli utenti scrivono `.php`, cambiare estensione rompe l'ecosistema |

## Struttura del plugin

```
elephc-intellij-plugin/
├── build.gradle.kts
├── src/main/resources/
│   ├── META-INF/plugin.xml
│   └── stubs/_elephc_stubs.php        # stub functions + ptr class
└── src/main/kotlin/com/illegalstudio/elephc/
    ├── stubs/ElephcStubLibraryProvider.kt    # registra gli stub
    ├── suppress/ElephcHighlightFilter.kt     # sopprime errori extern/ptr_cast
    ├── completion/ElephcCompletionContributor.kt
    ├── index/ElephcExternIndex.kt            # indicizza extern declarations
    ├── types/ElephcTypeProvider.kt           # tipi per ptr_cast<T>()
    ├── run/ElephcRunConfiguration.kt         # compile & run
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

## Componenti in dettaglio

### 1. Stub Library (risolve ~80% dei problemi)

File `_elephc_stubs.php` bundled nel plugin con dichiarazioni per tutte le funzioni `ptr_*`:

```php
<?php
// elephc compiler intrinsics — stub for IDE support only

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

Registrato via `PhpAdditionalLibraryRootsProvider` — PhpStorm lo indicizza automaticamente.

Riferimento per le firme: `src/types/checker/builtins.rs` (contiene tipi e validazione per ogni `ptr_*` built-in).

### 2. HighlightInfoFilter (soppressione errori)

Implementa `com.intellij.daemon.impl.HighlightInfoFilter`. Sopprime highlight di errore quando:

1. **Righe `extern`** — qualsiasi riga che inizia con `extern` (function, class, global, blocco con stringa)
2. **`ptr_cast<T>(...)`** — PHP parsa `ptr_cast < T > (...)` come due confronti; il filtro riconosce il pattern via regex `ptr_cast\s*<\s*\w+\s*>\s*\(` e sopprime
3. **`ptr<T>` in type hints** — le angle brackets nei type hint causano errori; sopprimere quando il contesto è `ptr<\w+>`

Questo è il componente più critico — senza, l'IDE è inutilizzabile su codice elephc.

### 3. Extern Declaration Index

`FileBasedIndexExtension` che scansiona `.php` per dichiarazioni extern. Non può usare il PSI di PHP (che non parsa `extern`), quindi usa un mini-parser a regex:

- `extern\s+function\s+(\w+)\s*\(([^)]*)\)\s*:\s*(\w+)` → funzione singola
- `extern\s+"([^"]+)"\s*\{` → inizio blocco con nome libreria
- `extern\s+class\s+(\w+)\s*\{` → classe extern
- `extern\s+global\s+(\w+)\s+\$(\w+)` → globale extern

L'indice alimenta completion e navigation.

Riferimento per la sintassi esatta: `src/parser/stmt.rs` funzione `parse_extern_stmts`.

### 4. Completion Contributor

- Dopo `ext` → suggerisce `extern`
- Dopo `extern` → suggerisce `function`, `class`, `global`, `"` (per nome libreria)
- In posizione type hint → suggerisce `ptr`, `ptr<NomeClasse>` (classi dall'indice extern)
- Dopo `ptr_cast<` → suggerisce nomi di classi extern

### 5. Type Provider (fase 2)

`PhpTypeProvider4` che risolve il tipo di ritorno di `ptr_cast<T>()`. Esamina il testo raw intorno all'espressione (il PSI è rotto dalle angle brackets) e restituisce `ptr` come tipo.

## Piano di implementazione a fasi

### MVP — elimina il rumore

1. Setup progetto Gradle con dipendenza `com.jetbrains.php`
2. Creare e bundlare `_elephc_stubs.php`
3. Implementare `ElephcStubLibraryProvider`
4. Implementare `ElephcHighlightFilter`
5. Testare con `examples/ffi/main.php` e altri esempi del progetto

### Fase 2 — intelligenza

6. `ElephcExternIndex` per indicizzare dichiarazioni extern
7. `ElephcCompletionContributor` per keyword e tipi
8. Reference resolution: Ctrl+click su chiamata extern → dichiarazione
9. `ElephcTypeProvider` per `ptr_cast<T>()`

### Fase 3 — developer experience

10. Run configuration (compile + esegui binario)
11. Parsing errori elephc con link cliccabili (line:col)
12. Gutter icons su dichiarazioni extern
13. Settings page per path del binario elephc

## Sfide principali

| Sfida | Impatto | Mitigazione |
|---|---|---|
| PHP PSI non parsa `extern` | Nessun PSI strutturato per dichiarazioni extern | Mini-parser custom a regex nel FileBasedIndex |
| `ptr_cast<T>()` rompe il parser PHP | PHP interpreta `<`/`>` come confronti | HighlightFilter + text-level regex per riconoscere il pattern |
| `ptr<T>` nei type hint | Angle brackets non valide in type position | Soppressione errori + stub class `ptr` per il caso semplice |
| API PHP plugin instabili | Possibili breaking changes tra versioni PhpStorm | Dichiarare range `since-build`/`until-build` stretto, usare solo API stabili |

## File di riferimento nel codebase elephc

- `src/parser/ast.rs` — definisce tutti i nodi AST custom (ExternFunctionDecl, PtrCast, CType, ecc.)
- `src/parser/stmt.rs` — parsing delle dichiarazioni extern (`parse_extern_stmts`)
- `src/parser/expr.rs` — parsing di `ptr_cast<T>()`
- `src/types/checker/builtins.rs` — firme e validazione dei built-in `ptr_*`
- `examples/ffi/main.php` — esempio canonico con tutta la sintassi extern

## Verifica

1. Aprire il progetto elephc in PhpStorm con il plugin installato
2. Verificare che `examples/ffi/main.php` non mostri errori rossi su `extern` e `ptr_cast`
3. Verificare che `ptr_null()`, `ptr_get()`, ecc. abbiano autocompletamento e documentazione
4. Ctrl+click su una chiamata extern → dovrebbe navigare alla dichiarazione
5. Compilare ed eseguire un esempio dalla run configuration
