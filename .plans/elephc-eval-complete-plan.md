# Piano dettagliato: supporto completo a eval tramite libelephc-magician

Nota percorso: la richiesta indicava `~/Downlaods`, che sembra un refuso. Questo file e' stato scritto in `~/Downloads`.

## Obiettivo

Implementare `eval($code)` completo per il sottoinsieme PHP supportato da elephc, senza imporre parser runtime, interprete o scope dinamico ai programmi che non usano `eval`.

La soluzione proposta e':

```text
programma senza eval
  -> runtime elephc normale
  -> nessuna lib eval
  -> nessun interprete
  -> stesse performance di oggi

programma con eval
  -> runtime elephc normale
  -> + libelephc-magician linkata condizionalmente
  -> solo gli scope che arrivano a eval pagano il costo dinamico
```

La strategia runtime per le funzioni che contengono `eval` e':

```text
codice nativo prima di eval
  -> resta nativo e ottimizzabile finche' non attraversa la barriera eval

al punto eval
  -> valuta l'argomento in source order
  -> materializza lo scope se non esiste
  -> flush dei locals vivi nello scope dinamico
  -> chiama libelephc-magician

libelephc-magician
  -> parse della stringa PHP
  -> lowering a EvalIR/EIR dinamico
  -> interpretazione usando lo scope ricevuto

dopo eval
  -> invalida i locals che eval puo' aver letto/scritto/unset
  -> ricarica lazy dai dynamic cells al prossimo uso
```

## Decisioni architetturali

1. `libelephc-magician` e' una bridge staticlib, come `elephc_tls`, `elephc_pdo`, `elephc_crypto` e `elephc_phar`.
2. Il linker la include solo quando il programma contiene una chiamata PHP a `eval`.
3. Il backend attivo resta AST -> EIR -> `src/codegen_ir/`; non si estende il backend AST legacy.
4. Il codice statico resta nativo. Non si interpreta tutta la funzione salvo dove necessario.
5. `eval` e' una barriera di effetti totale per optimizer, type checker e lowering.
6. L'interprete eval non introduce un value system separato: lavora sugli stessi valori/celle runtime di elephc.
7. L'EIR statico non deve diventare completamente dinamico. Lato eval conviene introdurre un `EvalIR` o un sottoinsieme EIR dinamico con istruzioni esplicite di scope.
8. Nessun JIT nella prima versione: niente assembler/linker a runtime, niente `dlopen`, niente compilazione ARM64/x86 al volo.

## Semantica PHP da preservare

Da verificare con `php -r` e codificare in test:

1. La stringa passata a `eval` non deve contenere tag `<?php`.
2. Le variabili dello scope chiamante sono visibili dentro eval.
3. Le modifiche fatte dentro eval sono visibili dopo eval.
4. Le variabili create da eval sono visibili dopo eval nello scope chiamante.
5. `unset($x)` dentro eval rimuove `$x` dallo scope chiamante.
6. `return expr;` dentro eval ritorna da `eval`, non dalla funzione chiamante.
7. Senza `return`, `eval` ritorna `null`.
8. Errori di parse devono produrre comportamento compatibile con PHP moderno, cioe' `ParseError` quando il modello eccezioni lo supporta.
9. `eval` puo' produrre output e chiamare funzioni.
10. Funzioni e classi dichiarate dentro eval devono finire nelle tabelle globali dinamiche, non nello scope locale.
11. Le dichiarazioni duplicate devono fallire come in PHP.
12. Namespace, magic constants, `__FILE__`, `__LINE__`, `__DIR__`, `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__` vanno verificati caso per caso contro PHP.

Esempi gia' verificati localmente con PHP:

```php
$x = 1;
$r = eval('$x = 3; return $x + 4;');
var_dump($x, $r);
// int(3), int(7)
```

```php
namespace A;
eval('function f_eval_test(){return 1;}');
var_dump(function_exists('A\\f_eval_test'), function_exists('f_eval_test'));
// bool(false), bool(true)
```

## Nuovi componenti

### Workspace crate

Aggiungere una nuova crate:

```text
crates/elephc-magician/
  Cargo.toml
  src/lib.rs
  src/abi.rs
  src/context.rs
  src/scope.rs
  src/value.rs
  src/parser.rs
  src/lower.rs
  src/eval_ir.rs
  src/interpreter.rs
  src/errors.rs
```

`Cargo.toml` root:

```toml
[workspace]
members = [
  ".",
  "crates/elephc-tls",
  "crates/elephc-pdo",
  "crates/elephc-crypto",
  "crates/elephc-phar",
  "crates/elephc-magician",
]
default-members = [
  ".",
  "crates/elephc-tls",
  "crates/elephc-pdo",
  "crates/elephc-crypto",
  "crates/elephc-phar",
  "crates/elephc-magician",
]
```

Come per le altre bridge staticlibs, aggiungere anche una dev-dependency per farla costruire in `cargo test`.

### Linker

In `src/linker.rs`, aggiungere una entry a `BRIDGES`:

```rust
BridgeStaticlib {
    lib_name: "elephc_magician",
    env_var: "ELEPHC_MAGICIAN_LIB_DIR",
    crate_name: "elephc-magician",
    whole_archive: false,
    macos_frameworks: &[],
    needs_libdl: true,
}
```

`whole_archive` puo' restare `false` se la libreria esporta solo simboli chiamati direttamente dal codice generato. Se in futuro usa registrazioni statiche o side effects di link, rivalutare.

### RuntimeFeatures

In `src/codegen/runtime_features.rs`:

```rust
pub struct RuntimeFeatures {
    pub regex: bool,
    pub phar_archive: bool,
    pub descriptor_invoker: bool,
    pub eval: bool,
}
```

Detection:

```text
program_requires_eval(program)
  -> true se c'e' una call PHP a eval dopo name resolution/case-insensitive builtin lookup
```

Required libraries:

```rust
if features.eval {
    libs.push("elephc_magician".to_string());
}
```

Importante: `eval` deve essere rilevato anche se scritto con casing diverso, se PHP lo consente, e anche in presenza di namespace fallback.

## ABI nativa verso libelephc-magician

L'ABI deve essere piccola, stabile e target-aware. Non passare enum Rust non `repr(C)` attraverso il confine staticlib.

Simboli minimi:

```c
uint32_t __elephc_eval_abi_version(void);

int32_t __elephc_eval_execute(
    ElephcEvalContext *ctx,
    ElephcEvalScope *scope,
    const uint8_t *code_ptr,
    uint64_t code_len,
    ElephcEvalResult *out
);
```

Possibili codici ritorno:

```text
0 = ok, out contiene il valore di ritorno di eval o null
1 = parse error
2 = runtime fatal
3 = uncaught throwable
4 = unsupported construct in eval subset
5 = ABI/runtime version mismatch
```

`ElephcEvalResult`:

```c
typedef struct {
    uint32_t kind;      // normal, return, fatal, throw
    void *value_cell;   // Mixed/runtime cell, owned secondo contratto runtime
    void *error;        // optional runtime error/throwable object
} ElephcEvalResult;
```

Regole:

1. Nessun panic Rust deve attraversare l'ABI.
2. Nessuna eccezione C++/unwind attraverso l'ABI.
3. Tutti i valori passati sono opaque handle o puntatori a celle runtime elephc.
4. Il codice generato deve controllare il codice ritorno e abbassarlo al comportamento PHP atteso.
5. L'ABI deve essere identica su `macos-aarch64`, `linux-aarch64`, `linux-x86_64`.

## EvalContext

`EvalContext` rappresenta lo stato globale richiesto da eval:

```text
EvalContext
  - ABI version / runtime version
  - allocator / GC hooks
  - global function table dinamica
  - global class table dinamica
  - global constants table dinamica
  - builtin registry
  - current file path
  - current line
  - current namespace metadata
  - current class/function/method metadata
  - diagnostics sink
```

Il codice nativo deve creare o ottenere un `EvalContext` per processo/program activation. Non va ricreato a ogni chiamata se contiene tabelle globali dinamiche.

## Dynamic Scope

Lo scope deve rappresentare celle PHP per nome:

```text
ElephcEvalScope
  - parent/global link opzionale
  - map: string name -> RuntimeCell*
  - flags per entry:
      present
      unset
      dirty
      by_ref
      persistent/owned/borrowed
  - generation counter
```

Ogni variabile locale materializzata diventa una cella:

```text
$x static local
  -> slot nativo corrente
  -> cella scope "x"
```

Regola chiave:

```text
il codice nativo puo' continuare a usare slot/registri statici,
ma al punto eval deve sincronizzare i locals osservabili nello scope dinamico.
```

## Flush / invalidation / reload

### Prima di eval

Il lowering della chiamata `eval($code)` deve:

1. Valutare `$code` in ordine sorgente.
2. Convertire/coercire `$code` a stringa secondo PHP.
3. Creare lo scope materializzato della activation se non esiste.
4. Calcolare il set di locals vivi e osservabili.
5. Fare flush dei locals nello scope:

```text
local slot x -> RuntimeCell scope["x"]
local slot y -> RuntimeCell scope["y"]
```

6. Passare `EvalContext`, `Scope`, `code_ptr`, `code_len` a `__elephc_eval_execute`.

### Durante eval

L'interprete legge e scrive solo lo scope:

```text
LoadVar("x")     -> scope_get("x")
StoreVar("x", v) -> scope_set("x", v)
UnsetVar("x")    -> scope_unset("x")
```

### Dopo eval

Il codice nativo non deve fidarsi dei valori statici precedenti.

Strategia consigliata:

1. Marcare invalidi tutti i locals flushati.
2. Se libelephc-magician restituisce una dirty set affidabile, invalidare almeno quella.
3. Se non c'e' dirty set o se eval usa variabili variabili, references, `unset`, `global`, invalidare tutto lo scope materializzato.
4. Al prossimo uso statico di `$x`, fare reload lazy:

```text
if local x invalid:
    if scope has "x":
        slot x = scope_get("x")
    else:
        slot x = null/unset semantics secondo contesto
```

Questo mantiene il codice prima e dopo eval nativo, ma tratta eval come barriera forte.

### Variabili create da eval

Esempio:

```php
eval('$newVar = 42;');
echo $newVar;
```

Il compiler statico vede `$newVar` dopo eval, ma non ha una definizione precedente.

Regola proposta:

1. Dentro una funzione che contiene eval, ogni read di variabile dopo una barriera eval deve poter fare fallback allo scope dinamico.
2. Se la variabile e' nota staticamente e non invalidata, usare lo slot nativo.
3. Se e' unknown/created-by-eval, leggere da `scope_get_or_null`.
4. Il tipo statico dopo eval degrada a `Mixed` per le variabili che possono essere toccate da eval.

### unset

Esempio:

```php
$x = 10;
eval('unset($x);');
echo $x;
```

Lo scope deve poter distinguere:

```text
present null
missing/unset
```

Non basta salvare `Null`, perche' PHP distingue variabile definita a `null` da variabile non definita per `isset`, warning e certi accessi.

## Cambi type checker

### Builtin/catalog

Agganciare `eval` nei punti canonici:

1. `src/types/checker/builtins/catalog.rs`
2. `src/types/signatures.rs`
3. categoria builtin appropriata sotto `src/types/checker/builtins/`
4. `first_class_callable_builtin_sig()` solo se si decide che `eval` e' callable; PHP lo tratta come language construct, quindi probabilmente non deve diventare first-class callable.
5. `function_exists("eval")` va verificato contro PHP e allineato alla policy scelta. Se PHP restituisce false per language constructs, non inserirlo nel catalogo dei callable normali senza un'eccezione.

Firma semantica:

```text
eval(string $code): mixed
```

Ma internamente va modellata come language construct:

```text
reads/writes locals
may define functions/classes/constants
may output
may throw/fatal
may return a value
is never pure
```

### Dynamic barrier

Aggiungere una nozione nel checker:

```text
FunctionDynamicState
  contains_eval: bool
  eval_barrier_seen: bool per blocco/control-flow
```

Dopo una barriera eval:

1. I tipi dei locals osservabili diventano `Mixed` o `MaybeUnset<Mixed>`.
2. Le chiamate a funzioni/classi non note potrebbero essere permesse come lookup dinamico se il control-flow ha attraversato un eval che puo' averle dichiarate.
3. Warning/diagnostiche devono evitare falsi positivi su variabili create da eval.

## Cambi optimizer/effects

In `src/optimize/effects/`:

```text
eval effects:
  - reads all visible locals
  - writes all visible locals
  - may unset locals
  - may define global functions/classes/constants
  - may read/write globals
  - may allocate heap
  - may output
  - may throw/fatal
  - may call arbitrary functions
```

Regole:

1. Non eliminare mai `eval`, anche se il risultato non e' usato.
2. Non propagare costanti attraverso `eval` per variabili visibili.
3. Non riordinare side effects attraverso `eval`.
4. Non foldare `eval` come stringa statica nella prima implementazione completa, per evitare due semantiche divergenti. Eventuale AOT static-eval puo' essere una ottimizzazione successiva.
5. Il DCE deve trattare eval come una call con effetti massimi.

## Cambi IR / lowering

### Nel programma statico

Il codice statico non deve usare istruzioni dinamiche ovunque. Aggiungere solo cio' che serve per la barriera:

```text
EnsureMaterializedScope
FlushLocalToScope(name, local)
CallEvalRuntime(scope, code)
InvalidateScopeLocals(scope)
ReloadLocalFromScope(name, local)
```

Queste possono essere:

1. istruzioni EIR nuove, se servono al backend;
2. oppure lowering diretto in `src/ir_lower/expr/` verso call runtime + metadata;
3. oppure metadata sulla call builtin `eval` che `src/codegen_ir/` abbassa target-aware.

Preferenza:

```text
EIR statico esplicito abbastanza da validare ownership e frame layout,
ma senza trasformare tutte le variabili normali in lookup dinamici.
```

### Nel runtime eval

Usare `EvalIR`, non necessariamente l'EIR completo:

```text
EvalIR
  ConstNull
  ConstBool
  ConstInt
  ConstFloat
  ConstString
  LoadVar(name)
  StoreVar(name, value)
  UnsetVar(name)
  LoadGlobal(name)
  BinaryOp(op, left, right)
  UnaryOp(op, value)
  Echo(value)
  Return(value)
  CallDynamic(name, args)
  If(...)
  While(...)
  Foreach(...)
  ArrayGet/ArraySet
  PropertyGet/PropertySet
  NewObject
  DefineFunction
  DefineClass
```

Motivo: l'EIR statico e' ottimizzato per frame/locals noti; eval ha bisogno di operazioni by-name e lookup dinamico.

## Runtime value bridge

Non creare:

```rust
enum Value {
    Null,
    Bool(bool),
    Int(i64),
    ...
}
```

come rappresentazione autonoma se poi il runtime nativo usa un'altra ABI.

Fare invece:

```text
EvalValue = handle/cell runtime elephc
```

Il bridge deve offrire operazioni:

```text
value_null()
value_bool(bool)
value_int(i64)
value_float(f64)
value_string(ptr,len)
value_add(a,b)
value_concat(a,b)
value_truthy(v)
value_to_string(v)
value_release(v)
value_retain(v)
array_get/array_set with COW
object_get/object_set
```

Queste operazioni devono rispettare:

1. boxed `Mixed` contract;
2. refcount;
3. ownership temporanei;
4. borrowed vs owned;
5. copy-on-write array/string;
6. cleanup su normal return, fatal, throw.

## Dynamic function/class tables

Eval completo richiede tabelle dinamiche globali.

Esempio:

```php
eval('function dyn() { return 42; }');
echo dyn();
```

Il codice statico dopo eval potrebbe chiamare una funzione non nota al compile-time.

Serve:

```text
DynamicFunctionTable
  name -> EvalFunction

DynamicClassTable
  name -> EvalClass
```

Per chiamate statiche a simboli non risolti dopo eval:

1. Il checker deve permettere lookup dinamico se il path di controllo puo' aver attraversato eval.
2. Il lowering deve generare `__rt_dynamic_call("dyn", args)` invece di direct symbol call.
3. `__rt_dynamic_call` cerca prima funzioni native note, poi funzioni eval-defined.
4. Le funzioni definite da eval possono essere interpretate da libelephc-magician.

Per chiamate note staticamente:

```text
foo(); // foo definita nel programma AOT
```

continuare a generare direct call quando possibile.

## Name resolution e namespace

Eval deve ricevere metadata del punto chiamante:

```text
current namespace
current file
current line
current class
current function
current method
```

Pero' funzioni e classi dichiarate dentro eval vanno verificate contro PHP. Il test locale mostra che una funzione dichiarata da eval dentro `namespace A` risulta globale:

```text
function_exists("A\\f_eval_test") -> false
function_exists("f_eval_test")    -> true
```

Da trasformare in test prima di fissare l'implementazione.

## Codegen target-aware

Tutto il lowering verso la call ABI deve stare nel percorso EIR attivo:

```text
src/ir_lower/expr/
src/codegen_ir/lower_inst/
src/codegen/abi/
```

Regole:

1. Non hardcodare registri ARM64 o x86_64 fuori dagli helper ABI.
2. La call a `__elephc_eval_execute` deve passare per gli helper target-aware.
3. Frame slots per scope handle, code string e result devono essere allocati prima del frame sizing.
4. Ogni `emitter.instruction(...)` aggiunta deve avere commento allineato secondo policy.
5. Coprire `macos-aarch64`, `linux-aarch64`, `linux-x86_64`.

## Pipeline di implementazione

### Fase 0: spike semantico

Obiettivo: fissare comportamento PHP prima del codice.

Deliverable:

1. File di note o test fixture con output PHP per:
   - variable read/write;
   - variable creation;
   - unset;
   - return from eval;
   - parse error;
   - output;
   - function declaration;
   - class declaration;
   - namespace interaction;
   - `$this` in method scope;
   - by-ref variables;
   - global/static variables.
2. Decidere se il supporto iniziale e' "eval completo per tutto il sottoinsieme elephc" o "eval completo PHP" piu' ampio del compilatore statico.

### Fase 1: link condizionale e stub ABI

Obiettivo: linkare `libelephc-magician` solo quando serve.

Modifiche:

1. Aggiungere crate `crates/elephc-magician`.
2. Aggiungere `elephc_magician` a `BRIDGES` in `src/linker.rs`.
3. Aggiungere `RuntimeFeatures.eval`.
4. Aggiungere detection `program_requires_eval`.
5. Aggiungere builtin/language construct `eval` al checker.
6. Generare una call a `__elephc_eval_execute` che per ora ritorna un errore controllato.

Test:

1. Programma senza eval non linka `elephc_magician`.
2. Programma con eval linka `elephc_magician`.
3. Test linker per `ELEPHC_MAGICIAN_LIB_DIR`.
4. Errore chiaro se la libreria non e' trovata.

### Fase 2: MaterializedScope minimo

Obiettivo: avere uno scope dinamico passabile alla lib, ancora senza parser completo.

Modifiche:

1. Rappresentare `ElephcEvalScope`.
2. Aggiungere helper runtime per scope:
   - create/destroy activation scope;
   - set cell by name;
   - get cell by name;
   - unset by name;
   - mark dirty;
   - generation counter.
3. Nel lowering, creare scope handle per funzioni con eval.
4. Al punto eval, flush dei locals vivi.
5. Dopo eval, invalidazione totale dei locals flushati.
6. Reload lazy al prossimo uso.

Test:

1. Stub eval modifica `$x` via scope e il codice nativo dopo eval vede il nuovo valore.
2. Stub eval crea `$y` e `echo $y` dopo eval funziona.
3. Stub eval unsetta `$x` e il codice nativo vede lo stato unset.

Nota: gli stub devono essere test-only o nascosti, non comportamento PHP pubblico.

### Fase 3: parser runtime e fragment parsing

Obiettivo: parse della stringa eval.

Opzioni:

1. Estrarre lexer/parser in crate riusabile, ad esempio `crates/elephc-frontend`.
2. Oppure duplicare temporaneamente solo il parser necessario in `crates/elephc-magician`, ma e' sconsigliato.

Regole:

1. `eval` parse-a statement fragment senza tag `<?php`.
2. Errori di parse diventano `ParseError` o fatal coerente con supporto eccezioni.
3. Span/line mapping deve partire dalla line del call-site.
4. Magic constants devono usare metadata del call-site.

Test:

1. `eval('$x = 1;');`
2. `eval('echo "ok";');`
3. stringa con `<?php` produce errore compatibile;
4. syntax error produce `ParseError`.

### Fase 4: EvalIR e interprete base

Obiettivo: eseguire assegnamenti, scalar ops, echo e return.

Implementare:

1. `Const*`
2. `LoadVar`
3. `StoreVar`
4. `UnsetVar`
5. `BinaryOp` per operatori base
6. `Echo`
7. `Return`

Test:

```php
$x = 10;
eval('$x = $x + 5;');
echo $x;
```

```php
$x = 1;
$r = eval('$x = 3; return $x + 4;');
echo $x, ":", $r;
```

```php
eval('$created = "yes";');
echo $created;
```

### Fase 5: control flow e arrays

Obiettivo: supportare costrutti comuni dentro eval.

Implementare:

1. `if/elseif/else`
2. `while`
3. `for`
4. `foreach`
5. `break/continue`
6. array literal
7. array read/write
8. string concat
9. comparisons e truthiness PHP

Test:

1. loop dentro eval modifica local esterno;
2. array COW dentro eval non rompe alias esterni;
3. foreach su array dello scope chiamante;
4. nested eval.

### Fase 6: dynamic calls e declarations

Obiettivo: funzioni chiamate o dichiarate da eval.

Implementare:

1. `CallDynamic`
2. dynamic builtin lookup
3. dynamic user function table
4. function declaration inside eval
5. duplicate declaration diagnostics/fatal
6. callable dispatch verso funzioni eval-defined

Test:

```php
eval('function dyn() { return 42; }');
echo dyn();
```

```php
function stat() { return 10; }
eval('echo stat();');
```

```php
eval('function dyn2($x) { return $x + 1; }');
echo call_user_func('dyn2', 4);
```

### Fase 7: objects/classes

Obiettivo: classi, metodi, properties e `$this`.

Implementare:

1. class declaration inside eval;
2. `new` dynamic;
3. method call dynamic;
4. property get/set;
5. `$this` nello scope metodo che chiama eval;
6. visibility coerente con runtime esistente.

Test:

```php
class A {
    public int $x = 1;
    public function bump() {
        eval('$this->x = $this->x + 1;');
    }
}
$a = new A();
$a->bump();
echo $a->x;
```

### Fase 8: references, globals, static locals

Obiettivo: chiudere le parti piu' sensibili dello scope PHP.

Implementare:

1. references/by-ref nello scope;
2. `global $x` dentro eval;
3. `static $x` se supportato dal compilatore;
4. variabili variabili `${$name}` se supportate;
5. superglobals;
6. closure capture interaction se supportata.

Test:

1. modifica by-ref dentro eval;
2. `global` dentro eval modifica global esterno;
3. `unset` su reference;
4. nested eval con scope condiviso.

### Fase 9: errori, throwable, cleanup

Obiettivo: ownership e control flow robusti.

Implementare:

1. parse error;
2. fatal runtime;
3. exception/throw dentro eval se supportato;
4. cleanup temporanei su return/fatal/throw;
5. GC/refcount stress tests.

Test:

1. parse error catchable come PHP supportato;
2. throw dentro eval attraversa chiamante;
3. temporanei refcounted non leakano;
4. array COW dopo eccezione.

### Fase 10: ottimizzazione performance

Solo dopo correttezza.

Possibili ottimizzazioni:

1. Scope materializzato lazy: creare solo alla prima eval call eseguita.
2. Flush selettivo con liveness precisa.
3. Dirty set dalla lib per reload selettivo.
4. Versioned cells: reload solo se generation cambia.
5. Cache parse/lower per stringhe eval ripetute identiche.
6. Interning nomi variabili nello scope.
7. Fast path per eval senza writes esterni, se l'analisi runtime lo prova.

Non fare queste ottimizzazioni prima dei test semantici.

## Strategia test

### Test unitari compiler

1. `RuntimeFeatures.eval` detection.
2. Link required libs.
3. Builtin/catalog/signature.
4. Effects: eval non pure, non DCE.
5. Type invalidation after eval.

### Test codegen end-to-end

In `tests/codegen/eval.rs` o modulo dedicato:

1. read local;
2. write local;
3. create local;
4. unset local;
5. return value;
6. output;
7. parse error;
8. dynamic function declaration;
9. dynamic function call after eval;
10. class/method `$this`;
11. arrays and COW;
12. nested eval;
13. eval inside loop;
14. eval in branch;
15. eval in function and top-level.

### Test error

In `tests/error_tests/`:

1. wrong argument count;
2. non-string coercion behavior;
3. unsupported construct inside eval, se il sottoinsieme non lo supporta ancora;
4. duplicate function/class declaration;
5. parse error formatting.

### Test cross-target

Per ogni fase che tocca ABI/codegen/runtime:

```bash
cargo test --test codegen_tests eval
./scripts/test-linux-x86_64.sh eval
./scripts/test-linux-arm64.sh eval
```

Prima del merge completo:

```bash
cargo build
cargo test eval
cargo test
cargo test -- --include-ignored
git diff --check
```

## Rischi principali

1. Scope sync incompleta: bug sottili in variabili create/unset dopo eval.
2. Value model duplicato: se EvalIR usa valori propri, COW/refcount diventano fragili.
3. Dynamic declarations: `eval('function f(){}'); f();` richiede fallback dinamico nel codice statico.
4. Type checker troppo statico: potrebbe rifiutare codice PHP valido dopo eval.
5. Optimizer troppo aggressivo: const propagation/DCE possono miscompilare attraversando eval.
6. ABI instabile tra compiler/runtime eval.
7. Parser runtime troppo accoppiato al compiler CLI.
8. File size e responsabilita': evitare un unico file gigante `interpreter.rs` che contiene parser, scope, lowering e runtime calls.

## Criteri di completamento

La feature e' considerata completa solo quando:

1. Programmi senza eval non linkano `elephc_magician`.
2. Programmi senza eval non cambiano assembly/performance in modo osservabile.
3. Programmi con eval linkano `elephc_magician` automaticamente.
4. Le funzioni con eval usano flush/invalidate/reload corretto.
5. Eval modifica, crea e unsetta variabili dello scope chiamante.
6. `return` dentro eval ritorna il valore di `eval`.
7. Funzioni/classi dichiarate dentro eval sono richiamabili secondo semantica PHP.
8. Ownership/GC/COW sono coperti da test.
9. Tutti i target supportati passano i test eval.
10. Docs PHP e internals descrivono chiaramente che eval abilita un runtime dinamico opzionale.

## Prima implementazione consigliata

Sequenza pragmatica:

1. Link condizionale `elephc_magician` + stub ABI.
2. `contains_eval` + lowering call runtime nel backend EIR.
3. `MaterializedScope` + flush/invalidate/reload con stub test-only.
4. Parser runtime per fragment eval.
5. EvalIR base: scalari, variabili, assign, add/concat, echo, return.
6. Arrays/control flow.
7. Dynamic calls.
8. Declarations.
9. Objects/classes.
10. References/global/static.
11. Performance pass.

Questa sequenza tiene separati i rischi: prima si dimostra che lo scope condiviso funziona, poi si rende reale l'interprete eval.
