---
name: full namespace support
overview: Aggiungere namespace completi in stile PHP a elephc, con nomi qualificati, `namespace`, `use`, alias, risoluzione coerente di classi/funzioni/costanti e integrazione con codegen, callback string-based e include/require.
todos:
  - id: fqn-model
    content: Definire il modello interno dei nomi qualificati e la funzione unica di mangling per i simboli assembly
    status: pending
  - id: parser-namespaces
    content: Estendere lexer/parser/AST per namespace, nomi qualificati, use imports, alias e group use
    status: pending
  - id: name-resolution-pass
    content: Introdurre una fase dedicata di risoluzione nomi tra resolver e type checker
    status: pending
  - id: checker-codegen
    content: Migrare type checker, codegen e callback string-based all'uso sistematico di FQN canonici
    status: pending
  - id: tests-docs
    content: Aggiungere test completi e aggiornare documentazione e roadmap
    status: pending
isProject: true
---

# Piano per namespace completi

## Obiettivo

Introdurre supporto completo ai namespace PHP nel compilatore, non come semplice prefisso cosmetico ma come vero sottosistema di name resolution coerente lungo tutta la pipeline: lexer, parser, AST, resolver, type checker, codegen e builtins che lavorano con nomi string-based.

## Definizione di "completo"

Per questo progetto, completo dovrebbe significare:

- supporto a `namespace Foo\\Bar;` e `namespace Foo\\Bar { ... }`
- supporto a nomi qualificati e fully-qualified con `\\`
- supporto a `use` per classi, funzioni e costanti, con alias e group use
- risoluzione coerente di classi, interfacce, trait, funzioni e costanti secondo le regole PHP compatibili con un compilatore AOT
- supporto coerente per `function_exists`, `call_user_func`, callback string literal e lookup compile-time basati sullo stesso schema di nomi
- simboli assembly mangled in modo deterministico e privo di collisioni

## Perche' e' cross-cutting

Oggi il compilatore assume quasi ovunque un unico spazio globale di nomi:

- parser e AST usano `String` flat per nomi di funzioni/classi/costanti: [src/parser/ast.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/parser/ast.rs)
- il type checker usa mappe globali `HashMap<String, ...>` per funzioni, classi, interfacce e costanti: [src/types/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/types/mod.rs)
- il codegen interpola direttamente i nomi nei label assembly come `_fn_{name}`, `_method_{class}_{method}`, `_static_{class}_{method}`: [src/codegen/functions.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/functions.rs), [src/codegen/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/mod.rs)
- i builtins che lavorano con stringhe, come `function_exists()` e `call_user_func()`, cercano nomi flat e generano label flat: [src/codegen/builtins/arrays/function_exists.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/builtins/arrays/function_exists.rs), [src/codegen/builtins/arrays/call_user_func.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/builtins/arrays/call_user_func.rs)

## Decisione architetturale consigliata

La strada piu' robusta e' introdurre una **rappresentazione canonica dei nomi pienamente qualificati** e un passaggio esplicito di **name resolution**.

Scelte consigliate:

- usare internamente FQN canonici senza `\\` iniziale, ad esempio `Game\\AI\\Enemy`
- separare il concetto di nome sorgente dal nome risolto
- aggiungere una funzione centrale di mangling per trasformare FQN in label assembly validi
- far passare tutte le lookup del checker e del codegen da questa rappresentazione canonica

## Architettura proposta

```mermaid
flowchart LR
    SourcePhp[SourcePhp] --> Lexer
    Lexer --> Parser
    Parser --> RawAst
    RawAst --> Resolver
    Resolver --> IncludedAst
    IncludedAst --> NameResolver
    NameResolver --> ResolvedAst
    ResolvedAst --> TypeChecker
    TypeChecker --> Codegen
    Codegen --> AsmLabels
    AsmLabels --> NativeBinary
```

## Moduli da toccare

### Lexer

Aggiungere la superficie sintattica che oggi manca del tutto:

- `namespace`
- separatore `\\` nei nomi qualificati
- supporto lessicale sufficiente per `use`, alias e group use in contesto file-level

File principali:

- [src/lexer/token.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/lexer/token.rs)
- [src/lexer/literals.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/lexer/literals.rs)
- [src/lexer/scan.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/lexer/scan.rs)

### Parser e AST

Oggi i nomi sono quasi tutti `String` flat. Serve introdurre nodi espliciti per:

- nome qualificato / fully qualified / relativo
- dichiarazioni `namespace`
- `use` import per classi, funzioni, costanti
- alias e group imports

File principali:

- [src/parser/ast.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/parser/ast.rs)
- [src/parser/stmt.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/parser/stmt.rs)
- [src/parser/expr.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/parser/expr.rs)

### Resolver / include pipeline

Il resolver oggi inlining i file senza alcun contesto namespace. Va deciso e implementato come preservare il contesto del file incluso:

- associare un namespace attivo alle dichiarazioni provenienti da ciascun file
- supportare file con `namespace` diversi una volta inclusi nel programma finale
- evitare collisioni tra dichiarazioni che oggi sono flat ma domani diventano FQN distinti

File principale:

- [src/resolver.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/resolver.rs)

### Name resolution dedicata

Aggiungere una fase nuova dopo `resolve()` e prima del type checker:

- costruzione ambiente namespace corrente
- costruzione tabelle `use class`, `use function`, `use const`
- risoluzione dei nomi sorgente in FQN canonici
- errori per nomi ambigui o non risolti

Questa fase dovrebbe produrre AST risolto oppure metadati equivalenti, per non spargere la logica di risoluzione in tutto il checker.

### Type checker

Aggiornare tutte le mappe globali keyed-by-name verso FQN canonici:

- `functions`
- `classes`
- `interfaces`
- `constants`
- riferimenti in `PhpType::Object(String)`
- `extends`, `implements`, trait use, static receiver named

File principali:

- [src/types/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/types/mod.rs)
- [src/types/checker/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/types/checker/mod.rs)
- [src/types/traits.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/types/traits.rs)

### Codegen e label mangling

Centralizzare una policy di mangling unica per tutti i simboli user-defined:

- `_fn_*`
- `_method_*`
- `_static_*`
- eventuali global/static symbols e tabelle runtime collegate alle classi

Questo evita collisioni e semplifica il debug.

File principali:

- [src/codegen/functions.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/functions.rs)
- [src/codegen/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/mod.rs)
- [src/codegen/expr/objects/dispatch.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/expr/objects/dispatch.rs)
- [src/codegen/runtime/mod.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/runtime/mod.rs)
- [src/codegen/ffi.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/ffi.rs)

### Builtins e callback string-based

Serve una policy coerente per tutte le stringhe che oggi rappresentano nomi funzione:

- `function_exists("...")`
- `call_user_func("...")`
- `call_user_func_array("...")`
- array callbacks string-based
- callback FFI passate come string literal

Questi punti devono usare la stessa risoluzione canonica o, se limitati ai literal, essere risolti compile-time con le stesse regole namespace-aware.

File principali:

- [src/codegen/builtins/arrays/function_exists.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/builtins/arrays/function_exists.rs)
- [src/codegen/builtins/arrays/call_user_func.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/builtins/arrays/call_user_func.rs)
- [src/codegen/builtins/arrays/call_user_func_array.rs](/Volumes/Crucio/Developer/illegal.studio/elephc/src/codegen/builtins/arrays/call_user_func_array.rs)
- altri builtins con callback string-based in `src/codegen/builtins/arrays/`

## Strategia di rollout consigliata

### Fase 1: infrastruttura nomi

- definire tipo/name model per nomi qualificati
- introdurre FQN canonico interno
- introdurre funzione di mangling assembly riusabile ovunque
- aggiungere test unitari per canonicalizzazione e collisioni

### Fase 2: sintassi namespaces

- parser per `namespace Foo\\Bar;`
- parser per `namespace Foo\\Bar { ... }`
- parser per nomi qualificati e fully qualified
- aggiornamento AST per i nuovi nodi

### Fase 3: import system completo

- `use Foo\\Bar\\Baz;`
- `use Foo\\Bar\\Baz as Qux;`
- `use function Foo\\bar;`
- `use const Foo\\BAR;`
- group use

### Fase 4: name resolution

- introdurre pass esplicito di risoluzione nomi
- riscrivere dichiarazioni e riferimenti in FQN
- supportare fallback compatibile per funzioni e costanti dove applicabile

### Fase 5: integrazione checker/codegen

- sostituire chiavi flat con FQN
- aggiornare dispatch di classi/metodi
- aggiornare label assembly e tabelle runtime
- aggiornare callback string-based e lookup compile-time

### Fase 6: include/require + namespace context

- propagare il namespace dei file inclusi
- testare collisioni, alias e riferimenti incrociati tra file

### Fase 7: documentazione e roadmap

- aggiornare [docs/language-reference.md](/Volumes/Crucio/Developer/illegal.studio/elephc/docs/language-reference.md)
- aggiornare [docs/architecture.md](/Volumes/Crucio/Developer/illegal.studio/elephc/docs/architecture.md)
- aggiornare [ROADMAP.md](/Volumes/Crucio/Developer/illegal.studio/elephc/ROADMAP.md)

## Test da prevedere

Seguire il test policy del progetto con copertura in lexer, parser, error e codegen:

- tokenizzazione e parsing di nomi qualificati
- namespace semicolon e braced
- import con alias e group use
- class/function/const resolution in namespace corrente
- fully qualified names con `\\`
- inheritance/interfaces/traits cross-namespace
- `function_exists` e `call_user_func` con nomi namespaced
- include/require tra file in namespace differenti
- collisioni tra simboli con stesso short name ma namespace diversi
- compatibilita' con builtins globali

## Rischi principali

- Il keyword `use` e' gia' usato per closure captures e trait use, quindi il parser deve restare rigorosamente context-sensitive.
- E' facile aggiornare classi/funzioni dirette ma dimenticare i path string-based, producendo mismatch tra checker e codegen.
- Il resolver a include flattening puo' introdurre bug sottili se il namespace del file non viene conservato correttamente.
- La fedelta' PHP completa sulla name resolution e' la parte piu' costosa; qui conviene definire subito test di compatibilita' molto mirati.

## Raccomandazione finale

Implementare i namespace come **vero sistema di nomi qualificati con pass dedicato di risoluzione**, non come semplice prefisso applicato qua e la'. E' un cambiamento ampio, ma e' l'unico modo per ottenere supporto completo e non fragile.
