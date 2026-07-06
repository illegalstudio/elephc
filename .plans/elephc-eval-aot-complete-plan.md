# Piano: completare AOT per literal `eval`

## Obiettivo

Completare il percorso AOT per `eval('...')` quando il codice da eseguire e'
una stringa nota a compile time, evitando il passaggio attraverso
`__elephc_eval_execute` per tutti i frammenti che possono essere compilati in
modo staticamente sicuro.

Il binario finale non deve incorporare il compilatore elephc, il parser, il
type checker o il codegen. Tutto il lavoro di parsing/lowering/codegen del
frammento literal deve avvenire a compile time. A runtime il programma deve
contenere solo:

- codice nativo generato per il frammento AOT;
- ABI glue per leggere/scrivere lo scope eval quando necessario;
- runtime helpers gia' esistenti;
- `libelephc-magician` solo quando rimangono eval dinamici o fallback non-AOT.

## Motivazione

Il piano performance di magician ha introdotto un primo AOT conservativo per
literal eval, ma oggi il subset e' limitato a scalar return/output/store e a
scope read/write semplici. Frammenti reali come:

```php
eval('$sum = 0; $n = 2; while ($n <= 100000) { ... } echo $sum;');
```

sono ancora marcati come fallback e chiamano:

```asm
bl ___elephc_eval_execute
```

Sul benchmark "somma dei numeri primi fino a 100000", questo mantiene il path
eval intorno a 800 ms e circa 642 MB RSS, contro circa 14 ms per Elephc
standard e circa 92 ms per PHP CLI. Il completamento dell'AOT deve trasformare
questo tipo di literal eval in codice nativo.

## Principi

1. Il backend attivo resta AST -> EIR -> `src/codegen_ir/`. Non estendere il
   backend AST legacy.
2. Il fallback a `__elephc_eval_execute` resta obbligatorio per codice dinamico
   o per costrutti non ancora supportati.
3. `eval` non ritorna implicitamente l'ultima espressione: senza `return`,
   ritorna `null`.
4. `return` dentro eval esce dal frammento eval, non dalla funzione chiamante.
5. Il frammento AOT deve preservare la semantica di scope PHP: variabili del
   caller visibili, scritture visibili dopo eval, variabili nuove create da
   eval visibili dopo eval.
6. Nessun costrutto deve passare ad AOT se non e' semanticamente coperto da
   test e fallback sicuro.
7. Ogni fase deve supportare `macos-aarch64`, `linux-aarch64` e
   `linux-x86_64`, oppure essere esplicitamente target-gated con diagnostica,
   test e documentazione.

## Stato attuale

Gia' implementato:

- `EvalLiteralCall` conserva il frammento literal nel payload EIR.
- `src/codegen_ir/lower_inst/builtins/eval.rs` prova un parser AOT prima del
  bridge.
- Il subset AOT attuale supporta scalar constants, scalar arithmetic, concat,
  `echo` / `print`, `return`, scalar stores e read-modify-write scope access
  tramite boxed Mixed helpers.
- I fallback literal emettono marker assembly `eval literal AOT fallback`.
- I path AOT emettono marker assembly `eval literal AOT compiled`.

Mancante per "AOT completo" in senso utile:

- `while`, `if`, `break`, `continue`;
- locals interni del frammento senza roundtrip continuo nello scope dinamico;
- operatori di confronto e truthiness di controllo;
- compound assignment nel subset AOT;
- chiamate statiche a builtins/funzioni gia' note;
- integrazione piu' diretta con la pipeline frontend/EIR per evitare un secondo
  mini-codegen parallelo troppo grande;
- registrazione e test target-aware per frammenti AOT non banali;
- benchmark di accettazione sul caso "primi fino a 100000".

## Stato implementazione - 2026-07-05

Implementato in questo ramo:

- nuovo percorso AOT `local scalar` in
  `src/codegen_ir/lower_inst/builtins/eval.rs`, separato dal percorso boxed
  Mixed esistente;
- locals interni del frammento su stack temporaneo, con flag `defined` per
  flushare nello scope solo le variabili effettivamente assegnate a runtime;
- supporto AOT per:
  - assignment semplice e compound normalizzato dal parser;
  - `echo`, `print`, `return`;
  - `while`, `if`/`elseif`/`else`, `break`, `continue`;
  - valori `int`/`bool` e stringhe solo in output;
  - operatori `+`, `-`, `*`, `%`, `<`, `<=`, `>`, `>=`, `==`, `!=`, `&&`,
    `||`, concat in `echo`;
  - `strlen("literal")` foldato a compile time come primo builtin statico;
  - chiamate a funzioni utente statiche gia' note con argomenti `int`/`bool`
    in registri e ritorno `int`;
- fallback conservativo ancora attivo per costrutti non coperti, verificato con
  `foreach`;
- test assembly/runtime per:
  - loop `while` self-contained senza bridge;
  - benchmark primi `100000` senza bridge, output `454396537`;
  - `strlen("literal")` senza bridge;
  - chiamata `inc(int $x): int` senza bridge;
  - regressioni boxed scope read/write esistenti.

Evidenza locale:

- `cargo test --test codegen_tests literal_eval_ -- --nocapture`
  - 5 test passati prima dell'aggiunta del test `strlen`;
- `cargo test --test codegen_tests test_literal_eval_static_strlen_uses_aot_without_execute_bridge -- --nocapture`
  - passato;
- `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`
  - passato;
- benchmark temporaneo primi `100000`, 5 iterazioni:
  - Elephc standard: mediana `13.451 ms`;
  - Elephc via eval AOT: mediana `10.561 ms`;
  - PHP standard: mediana `87.725 ms`;
  - PHP via eval: mediana `85.066 ms`;
  - assembly eval: `__elephc_eval_execute` assente, marker
    `eval literal AOT compiled local scalar` presente;
- benchmark `benchmarks/magician/cases/algebra_heavy`, 5 iterazioni:
  - Elephc standard: `3.52 ms`;
  - Elephc via eval AOT: `4.43 ms`;
  - PHP standard: `65.50 ms`;
  - PHP via eval: `66.66 ms`.

Ancora aperto:

- il percorso `local scalar` e' ancora un mini-codegen manuale, non una
  funzione EIR interna;
- il link a `elephc_magician` e' stato rimosso solo per literal eval
  native-only senza locals/scope (`return 7`, `strlen("...")`, chiamata utente
  statica scalar-register); resta richiesto per frammenti con locals creati
  dentro eval per preservare la visibilita' PHP nello scope chiamante;
- gli helper `eval_scope_get/set` e `eval_value_*` usati per preservare scope
  vivono ancora sotto la feature runtime eval;
- builtin statici foldabili supportati per `strlen("literal")`,
  `intval(<literal-scalar>)` e `abs(<int-literal>)`;
- chiamate a funzioni statiche utente supportate solo per il subset
  scalar-register (`int`/`bool` args, ritorno `int`, niente by-ref/variadic).

Aggiornamento Milestone 6 - 2026-07-05:

- `src/ir_lower/expr/mod.rs` evita `apply_eval_barrier()` per literal eval
  classificati come native-only, quindi non dichiara hidden eval
  context/scope quando non servono;
- `src/ir_lower/program.rs` distingue `EvalLiteralCall` native-only nello scan
  `RuntimeFeatures`, includendo il sottoinsieme di chiamate utente statiche
  note con argomenti letterali `int`/`bool` e ritorno `int`;
- `src/codegen_ir/lower_inst/builtins/eval.rs` usa il boxing core runtime per
  i ritorni native-only e forza invece lo scope-sync per frammenti con locals
  interni, evitando di perdere variabili create da eval;
- test aggiornati: `return 7`, `strlen("test")` e `inc(41)` ora verificano
  assenza di `__elephc_eval_*` in user/runtime asm e assenza di
  `elephc_magician` tra le librerie richieste;
- aggiunto test esplicito per `$a = 10; echo eval('return $a + 20;');`, che
  verifica read dello scope caller via AOT e output `30`;
- `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 8/8;
- `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato.

Aggiornamento Milestone 2 - 2026-07-05:

- il parser AOT boxed ora conserva `scope_reads` e `scope_writes` per ogni
  frammento literal;
- il path AOT boxed filtra la sync scope:
  - flush solo dei nomi letti o scritti dal frammento;
  - reload solo dei nomi scritti dal frammento;
  - separazione local/global mantenuta tramite le tabelle sync esistenti;
- il test `$a = 10; $unused = 99; echo eval('return $a + 20;')` verifica che
  venga emesso un solo `__elephc_eval_scope_set`, quindi `$unused` non viene
  flushato nello scope eval;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_scope -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 8/8;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato.

Aggiornamento Milestone 4 - 2026-07-05:

- il subset local-scalar AOT folda static builtins puri quando il risultato e'
  completamente noto a compile time:
  - `strlen("literal")`;
  - `intval()` su `int`, `bool` o stringa intera parsabile;
  - `abs()` su espressioni integer-literal rappresentabili come `int`;
- il classifier `RuntimeFeatures` riconosce gli stessi fold, quindi i casi
  native-only non linkano `elephc_magician`;
- aggiunti test:
  - builtin case-insensitive in caller namespaced:
    `STRLEN("ab") + InTvAl("40") + ABS(-2)` -> `44`, senza helper eval e
    senza `elephc_magician`;
  - body local-scalar con `$x = intval("42"); echo $x + 1; return abs(-10);`
    -> `4310`, senza bridge;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_builtins -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 10/10;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato.

Aggiornamento Milestone 7 - 2026-07-05:

- aggiunto `src/eval_aot.rs` come modulo target-independent per parsing dei
  literal eval, classificazione del subset native-only e folding dei builtin
  statici supportati;
- `src/ir_lower/program.rs` e `src/ir_lower/expr/mod.rs` ora usano lo stesso
  classifier condiviso per decidere rispettivamente feature runtime e barriera
  eval in lowering;
- `src/codegen_ir/lower_inst/builtins/eval.rs` riusa lo stesso parsing/folding
  condiviso prima di emettere i path AOT, riducendo la duplicazione tra scan
  feature e codegen;
- questo non e' ancora il target finale delle funzioni EIR interne, ma sposta
  le decisioni di eleggibilita' fuori dal lowerer assembly e rende piu'
  difficile divergere tra "non linkare magician" e "emettere davvero AOT";
- aggiunto un primo path reale a funzione EIR interna per literal eval
  self-contained senza variabili/scope:
  - nome interno deterministico non esprimibile come nome PHP sorgente;
  - generazione della `Function` EIR sintetica dopo il lowering dei body
    ordinari e prima dello scan `RuntimeFeatures`;
  - call-site `eval()` ancora rappresentato da `EvalLiteralCall`, ma il
    lowerer assembly chiama la funzione EIR sintetica quando presente;
  - il subset iniziale e' volutamente stretto (`echo`, `print`, `return`,
    `if`/`elseif`/`else` senza scope, literal scalar, operatori scalar
    no-scope, builtin statici foldabili e chiamate statiche a funzioni utente
    gia' ammesse dal classifier) mentre assignment/local/scope restano sul path
    AOT manuale esistente;
  - i builtin statici foldabili vengono riscritti a literal integer nell'AST
    del frammento prima di abbassare la funzione EIR, evitando dipendenze da
    name-resolution runtime o case-sensitivity del frammento eval;
- lo scan `RuntimeFeatures` considera no-bridge anche i frammenti eleggibili
  alla funzione EIR interna, non solo il vecchio subset native-only scalar;
- `src/ir_lower/expr/mod.rs` non applica piu' la barriera eval ai literal che
  verranno gestiti da funzione EIR interna: questo evita di dichiarare
  `EvalContext`/`EvalScope` nascosti e di trascinare helper user-side del bridge
  quando il bridge non puo' essere chiamato;
- scelta semantica ancora aperta: i frammenti con assegnazioni o locals creati
  dentro eval restano sul percorso `local scalar` manuale finche' non esistono
  primitive EIR per sincronizzare correttamente lo scope PHP. Spostarli subito
  in una funzione EIR no-param perderebbe la visibilita' delle variabili create
  o modificate da eval;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scalar_return_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_user_function_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_builtins -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests test_literal_eval_if_without_scope_uses_eir_aot_function -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_scope -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - riproduzione CLI manuale con `eval('if (true) { echo "yes"; } else { echo "no"; }')`: compilazione/link riusciti, output `yes`.

Aggiornamento Milestone 7.1 - 2026-07-05:

- `src/eval_aot.rs` ora materializza un `EvalAotPlan` condiviso invece di
  esporre solo classifier booleani separati;
- il piano contiene:
  - nome funzione EIR sintetica, quando il frammento e' eleggibile;
  - AST gia' parsato/foldato per il lowering EIR;
  - set conservativi di variabili lette e scritte;
  - flag per variabili create dinamicamente, necessita' di eval context,
    necessita' di global scope e motivo di fallback;
- `src/ir_lower/program.rs` usa `EvalAotPlan::requires_runtime_eval_bridge()`
  per decidere se il modulo deve ancora linkare il runtime eval/magician;
- la raccolta dei candidati AOT EIR usa il nome e il body calcolati dal piano,
  evitando di duplicare hashing/classificazione nel lowerer;
- `src/ir_lower/expr/mod.rs` continua a usare lo stesso piano per decidere se
  applicare la barriera eval;
- nessuna nuova semantica di assignment/local e' stata spostata nella funzione
  EIR no-param: i frammenti che leggono o scrivono scope restano sul percorso
  AOT scope/local esistente finche' non vengono introdotte primitive EIR
  esplicite per `eval_scope_get/set`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_if_without_scope_uses_eir_aot_function -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4.

Aggiornamento Milestone 7.2 - 2026-07-05:

- introdotte primitive EIR esplicite `EvalScopeGet` e `EvalScopeSet`, con
  validazione immediato `GlobalName`, effetti conservativi heap/fatal/refcount
  e dispatch nel backend EIR target-aware;
- `src/codegen_ir/lower_inst/builtins/eval.rs` abbassa le primitive verso gli
  helper runtime esistenti `__elephc_eval_scope_get/set`, usando ABI helpers
  invece di registri hardcoded;
- `src/eval_aot.rs` ora puo' produrre una seconda funzione EIR sintetica per
  frammenti literal che leggono variabili note dallo scope caller ma non
  scrivono scope e non creano nuove variabili;
- la funzione sintetica scope-read riceve un handle allo scope eval, e
  `LoweringContext` trasforma le letture selezionate di variabili PHP in
  `EvalScopeGet`;
- il call-site `eval()` sincronizza solo i nomi effettivamente letti dal
  frammento prima di chiamare la funzione EIR scope-read. La selezione
  local/global e' stata corretta per non trattare un nome internato dalla
  funzione sintetica come storage globale reale;
- caso supportato e verificato: `$a = 10; echo eval('return $a + 20;')`
  usa funzione EIR AOT con scope-read, emette un solo
  `__elephc_eval_scope_set`, legge con `__elephc_eval_scope_get`, non chiama
  `__elephc_eval_execute` e produce `30`;
- restano fuori da questa migrazione le scritture scope/local complete:
  `EvalScopeSet` esiste come primitiva e backend, ma assignment/read-write
  dentro eval resta sul percorso AOT manuale finche' non viene portato al
  lowering EIR con ownership/reload completi;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scope_read_return_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`;
  - `git diff --check` focalizzato sui file AOT eval toccati: passato.

Aggiornamento Milestone 7.3 - 2026-07-05:

- estesa la funzione EIR scope-AOT ai frammenti read/write semplici, per
  esempio `$x = $x + 1`, usando `EvalScopeGet` per leggere il valore caller e
  `EvalScopeSet` per scrivere il risultato nello scope eval;
- `EvalAotPlan` distingue ora le letture che devono arrivare dal caller dalle
  letture di variabili gia' inizializzate dentro il frammento. Questo evita di
  spostare benchmark/local temporaries come prime-loop e while local-scalar nel
  path scope EIR, che sarebbe corretto ma molto piu' costoso;
- `LoweringContext` supporta `enable_eval_scope_access(read_names,
  write_names)`: le letture selezionate abbassano a `EvalScopeGet`, le
  assegnazioni semplici selezionate abbassano a `EvalScopeSet` senza creare
  slot locali interni;
- il call-site EIR scope-AOT:
  - flusha nello scope solo i nomi letti dal caller;
  - chiama la funzione EIR sintetica con l'handle dello scope;
  - salva il valore di ritorno durante i reload;
  - ricarica nel caller solo i nomi scritti dal frammento;
  - per i global-backed writes legge dallo scope locale scritto dalla funzione
    EIR, non dallo scope globale separato del bridge;
- il test read/write ora verifica output runtime (`2`) oltre a marker EIR,
  `__elephc_eval_scope_get/set` e assenza di `__elephc_eval_execute`;
- restano aperti:
  - creazione completa di nuove variabili via funzione EIR, quando non c'e'
    una lettura caller iniziale;
  - assegnazioni non semplici (`array`, property, list/unpack, by-ref, dynamic
    variable);
  - ownership/reload piu' ampi per shape array/object oltre al subset gia'
    coperto dai sync helpers esistenti;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scope_read_write_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 7.4 - 2026-07-05:

- abilitata la funzione EIR scope-AOT anche per frammenti writes-only lineari
  senza letture interne di variabili, come `eval('$created = "yes";')`;
- il gate e' volutamente stretto: non prende body con letture interne dopo
  una scrittura (`$x = intval("42"); echo $x`) e non prende loop/local body
  come il prime benchmark, che restano sul percorso local-scalar ottimizzato;
- il classifier conserva due viste distinte:
  - letture raw del frammento, usate per capire se un writes-only e' davvero
    una creazione/store lineare;
  - letture che devono arrivare dal caller, usate per decidere il flush scope
    prima della funzione EIR;
- il test `test_literal_eval_scalar_store_uses_aot_scope_write` ora verifica
  esplicitamente il marker `eval literal AOT compiled EIR function`, la
  scrittura `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute` e
  output runtime `yes`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scalar_store_uses_aot_scope_write -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_builtins_in_local_body_use_aot -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 7.5 - 2026-07-05:

- aggiunto un finalizer EIR per funzioni eval scope-aware: prima di ogni
  `return` esplicito o implicito, `LoweringContext` puo' flushare un set
  selezionato di local slot nel runtime eval scope tramite `EvalScopeSet`;
- `EvalAotPlan` distingue ora:
  - `writes`: tutti i nomi scritti dal frammento, usati dal call-site per il
    reload nel caller;
  - `direct_writes`: assegnazioni che devono scrivere subito nello scope
    durante il corpo EIR, usate per read-modify-write come `$x = $x + 1`;
  - `flush_writes`: assegnazioni mantenute come local EIR interni e scritte
    nello scope solo a fine funzione;
- i frammenti con write locali ma senza letture iniziali dal caller, inclusi
  local-body con builtin foldabili, `while` self-contained e il prime-sum
  benchmark, ora vengono abbassati a funzione EIR scope-aware invece che al
  mini-codegen manuale `local scalar`;
- il vecchio percorso manuale `local scalar` resta come fallback di codegen per
  frammenti non ancora pianificati come funzione EIR, ma non e' piu' il path
  atteso per il benchmark dei primi coperto dai test;
- `requires_runtime_eval_bridge()` non tratta piu' read/write come motivo di
  bridge quando il piano ha gia' una funzione EIR scope-aware: serve il runtime
  eval-scope, ma non l'entrypoint interpretato `__elephc_eval_execute`;
- test aggiornati per aspettarsi il marker
  `eval literal AOT compiled EIR function` nei casi local-body, while e
  prime-loop;
- verifiche eseguite finora:
  - `cargo test --test codegen_tests test_literal_eval_local_while_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_builtins_in_local_body_use_aot -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_prime_loop_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 11/11;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`;
  - `git diff --check` focalizzato sui file AOT eval toccati: passato.

Aggiornamento Milestone 7.6 - 2026-07-05:

- ampliato il folding compile-time condiviso dei builtin statici puri nel
  planner AOT eval, in modo che scan runtime, lowering EIR sintetico e codegen
  vedano lo stesso sottoinsieme;
- oltre a `strlen`, `intval` e `abs`, il planner folda ora:
  - `boolval()` su literal `int`/`bool`/`string`/`null`;
  - `ord("...")`;
  - `chr(<ascii-int>)`;
  - `min()` / `max()` su argomenti integer literal;
  - `strtolower()`, `strtoupper()` e `strrev()` su stringhe ASCII literal;
- le trasformazioni stringa sono volutamente ASCII-only per non promettere
  semantica byte-string non ancora rappresentabile in modo sicuro dall'AST
  `StringLiteral`;
- aggiunto test namespaced/case-insensitive che verifica EIR AOT, assenza di
  `__elephc_eval_execute`, assenza di helper eval in user/runtime assembly,
  assenza di `elephc_magician` e output `AB:cd:cba77`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_more_static_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 12/12.

Aggiornamento Milestone 7.7 - 2026-07-05:

- il path EIR scope-read read-only puo' ora evitare completamente lo scope
  runtime eval: la funzione sintetica riceve un parametro `mixed` per ogni
  variabile letta dal caller;
- `src/ir_lower/function.rs` genera la signature diretta dei read params e
  abbassa il body senza `EvalScopeGet` quando il frammento non scrive scope;
- `src/codegen_ir/lower_inst/builtins/eval.rs` materializza i parametri
  leggendo i local slot del caller, boxandoli a `Mixed`, e chiama la funzione
  EIR sintetica senza `__elephc_eval_scope_get/set` e senza
  `__elephc_eval_execute`;
- `src/ir_lower/expr/mod.rs` evita la barriera eval per questo subset solo se
  le variabili lette sono local PHP ordinari, non global/superglobal/by-ref, e
  hanno tipi supportati;
- `src/ir_lower/program.rs` rileva comunque eventuale stato eval hidden nei
  locals prima di decidere le runtime features, evitando link mancanti quando
  altri path richiedono ancora magician;
- `src/ir_passes/dead_store.rs` esclude dal dead-store scalar slots letti
  implicitamente da `EvalLiteralCall`: senza questa esclusione il DSE non vedeva
  il load generato piu' tardi in codegen e trasformava `$a = 10` in `nop`;
- il test `$a = 10; $unused = 99; echo eval('return $a + 20;'); echo ':' .
  $unused;` ora verifica:
  - marker `eval literal AOT compiled EIR function with direct read params`;
  - assenza di helper `__elephc_eval_*` in user/runtime assembly;
  - assenza di `elephc_magician` tra le librerie richieste;
  - output runtime `30:99`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scope_read_return_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 12/12;
  - `cargo test --test codegen_tests eval_scope -- --nocapture`: 4/4;
  - riproduzione CLI manuale con `<?php $a = 10; echo eval('return $a;');`:
    IR main contiene `store_local` prima di `eval_literal_call`;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 7.8 - 2026-07-05:

- il classifier condiviso EIR AOT ora accetta anche `FloatLiteral` nei
  frammenti literal eval, allineandosi al fatto che il lowering EIR e il
  boxing del ritorno `Mixed` gia' supportano float;
- aggiunti test per:
  - `echo eval('return 1.5 + 2.25;')`, con funzione EIR AOT, assenza di
    helper `__elephc_eval_*`, assenza di `elephc_magician` e output `3.75`;
  - `$a = 1.5; echo eval('return $a + 2.25;')`, con direct read params,
    assenza di helper eval/runtime magician e output `3.75`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_float_ -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 14/14;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 5.1 - 2026-07-05:

- il subset di chiamate statiche a funzioni utente dentro literal eval non e'
  piu' limitato a firme `int`/`bool` con ritorno `int`;
- `static_function_signature_supported()` accetta ora parametri e ritorni
  scalar-register/string literal per `int`, `bool`, `float` e `string`, sempre
  escludendo by-ref, variadic e mismatch tipo/argomento;
- aggiunto test con:
  - `eval_label(string $s, bool $ok): string`;
  - `eval_scale(float $x): float`;
  - literal eval che chiama entrambe, non usa bridge/helper eval, non linka
    `elephc_magician` e produce `Hi:T3.75`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_user_functions_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 15/15;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 1/7.9 - 2026-07-05:

- il classifier condiviso EIR AOT ora considera `null` una literal expression
  sicura, quindi `eval('return null;')` puo' usare la funzione EIR sintetica
  senza cadere nel bridge interpretato;
- aggiunto test combinato per la semantica PHP di `eval`:
  - `return null;` restituisce un boxed `Mixed` nullo;
  - un frammento che termina senza `return` restituisce comunque `null`;
  - entrambi i frammenti usano funzioni EIR AOT, non referenziano helper
    `__elephc_eval_*`, non emettono runtime eval bridge e non linkano
    `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_null_return_and_fallthrough_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 16/16;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.10 - 2026-07-05:

- il gate condiviso EIR AOT accetta ora anche `for` quando init, condizione,
  update e body appartengono gia' al subset EIR sicuro;
- il supporto riusa il normale lowering EIR `lower_for`, incluso il loop-depth
  gia' usato per `break` e `continue`, senza aggiungere codegen assembly
  specifico per eval;
- aggiunto test con `for ($i = 0; $i < 5; $i = $i + 1)`, `continue`,
  `break`, output dal body e flush finale di `$i` nello scope caller;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_for_loop_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 17/17;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.11 - 2026-07-05:

- il gate condiviso EIR AOT accetta ora anche `do/while` quando body e
  condizione appartengono al subset EIR sicuro;
- il path riusa il lowering EIR normale `lower_do_while`, incluso il target di
  `continue` verso la condizione, senza codegen assembly speciale per eval;
- aggiunto test con assegnazione locale, `continue`, output dal body e flush
  finale nello scope caller;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_do_while_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 18/18;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.12 - 2026-07-05:

- il gate condiviso EIR AOT accetta ora espressioni ternarie complete e short
  ternary (`?:`) quando condizione e rami appartengono al subset EIR sicuro;
- il folding condiviso dei builtin statici ricorre anche dentro i rami del
  ternario, cosi' scan runtime, funzione EIR sintetica e codegen restano
  allineati;
- `??` resta volutamente fuori da questa tranche perche' la semantica di
  variabile mancante/null richiede una verifica separata sui read params;
- aggiunto test no-scope con ternario completo, short ternary e condizione con
  `strlen("abc")`, verificando assenza di helper `__elephc_eval_*` e di
  `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_ternary_expressions_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 19/19;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.13 - 2026-07-05:

- il gate condiviso EIR AOT accetta ora `??` quando value e default
  appartengono al subset EIR sicuro;
- il folding condiviso dei builtin statici ricorre anche dentro value/default
  del null-coalesce;
- aggiunto test no-scope/read-only con:
  - `null ?? "literal"`;
  - `"set" ?? "bad"`;
  - `$a ?? "fallback"` tramite direct read params;
  - `$missing ?? "missing"` materializzato come param `Mixed` nullo senza
    inizializzare lo scope eval;
- il test verifica funzioni EIR AOT, direct read params, assenza di helper
  `__elephc_eval_*`, assenza di runtime eval bridge e assenza di
  `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_null_coalesce_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_null_coalesce_executes_through_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 20/20;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`;
- nota: il filtro largo `cargo test --test codegen_tests null_coalesce -- --nocapture`
  non e' stato usato come segnale di questa modifica perche' include
  `test_constant_folding_null_coalesce_removes_runtime_concat_call`, che
  fallisce su un'asserzione optimizer non collegata al path eval AOT.

Aggiornamento Milestone 3/7.14 - 2026-07-06:

- il folding condiviso dei builtin statici ricorre ora anche dentro i nodi
  `for` e `do/while`, inclusi init/condition/update e body;
- questo evita divergenze tra classifier AOT e body EIR sintetico quando un
  loop e' ammesso perche' contiene un builtin foldabile nella condizione;
- aggiornati i test AOT di `for` e `do/while` per usare `strlen("...")` nelle
  condizioni di loop, mantenendo le verifiche su funzione EIR AOT, flush scope
  e assenza di `__elephc_eval_execute`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_do_while_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_for_loop_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 20/20;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.15 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora espressioni `match` quando subject,
  condizioni, risultati e default appartengono al subset EIR sicuro;
- il default e' richiesto dal gate per restare conservativi ed evitare di
  introdurre nuovi casi fatal non ancora coperti dal subset AOT;
- il folding condiviso dei builtin statici ricorre dentro subject, condizioni,
  risultati e default del `match`;
- aggiunto test con:
  - confronto stretto `"1"` vs `1`/`"1"` in frammento no-scope;
  - match su variabile caller `$x` tramite direct read params;
  - assenza di helper `__elephc_eval_*`, runtime bridge e `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_match_expression_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_match_expression_dispatches_strict_arms -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 21/21;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.16 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora `===` e `!==` quando entrambi gli
  operandi appartengono al subset EIR sicuro;
- il supporto resta volutamente limitato agli operatori di identita' stretta:
  divisione, potenza, bitwise, shift e spaceship restano fuori da questa
  tranche per evitare edge case numerici o coercioni non ancora coperti;
- aggiunto test con:
  - `"10" === 10` e `true !== false` in frammento no-scope;
  - `$a === 10` tramite direct read params;
  - assenza di helper `__elephc_eval_*`, runtime bridge e `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_strict_equality_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_scalar_strict_equality_executes_through_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 22/22;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.17 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora l'operatore logico `xor` quando gli
  operandi appartengono al subset EIR sicuro;
- il supporto riusa il normale lowering EIR `lower_logical_xor`, senza codegen
  assembly dedicato a eval;
- aggiunto test con:
  - `(true xor false)` in frammento no-scope;
  - `($flag xor true)` tramite direct read params;
  - assenza di helper `__elephc_eval_*`, runtime bridge e `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_logical_xor_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_logical_keyword_operators_execute_through_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 23/23;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.18 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora cast scalari `(int)`, `(float)`,
  `(string)` e `(bool)` quando l'espressione sorgente appartiene al subset EIR
  sicuro;
- `(array)` resta fuori dal subset AOT perche' introdurrebbe ownership/shape
  array non necessarie a questa tranche;
- il folding condiviso dei builtin statici ricorre anche dentro l'espressione
  sorgente del cast;
- aggiunto test con:
  - cast scalari no-scope su string/int/bool/float;
  - cast `(int)$a` tramite direct read params;
  - assenza di helper `__elephc_eval_*`, runtime bridge e `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scalar_casts_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 24/24;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`;
- nota: il filtro largo `cargo test --test codegen_tests cast -- --nocapture`
  include `test_constant_folding_string_cast_removes_runtime_itoa_call`, che
  fallisce su un'asserzione optimizer non collegata al path eval AOT. Nello
  stesso run sono passati sia `test_literal_eval_scalar_casts_use_aot_without_magician`
  sia `test_eval_dispatches_cast_builtin_calls`.

Aggiornamento Milestone 2/7.19 - 2026-07-06:

- i direct read params per eval AOT accettano ora local caller `null`
  (`PhpType::Void`) senza materializzare eval context/scope;
- il lowerer codegen tratta un local read-only `Void` come sorgente Mixed null
  dedicata, senza inserirlo nella sync scope generale, per evitare un finto
  round-trip write/reload di un tipo che rappresenta solo `null`;
- il test null-coalesce copre ora `$n = null; eval('return $n ?? ...')`,
  verificando output corretto e assenza di `__elephc_eval_*`, bridge runtime e
  `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_null_coalesce_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 24/24;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 4/7.20 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora:
  - `floatval()` su literal int/float/bool/null e stringhe float parsabili in
    modo completo;
  - `strval()` su literal int/bool/string/null con risultati PHP stabili;
  - probe `is_int`/`is_integer`/`is_long`, `is_string`, `is_bool`,
    `is_float`/`is_double`/`is_real`, `is_null` su literal scalari;
- i fold restano volutamente conservativi: niente parsing parziale stile PHP
  per stringhe numeriche miste, niente `strval(float)` per evitare differenze
  di formatting;
- esteso il test dei builtin statici EIR AOT con conversioni e type-probe,
  mantenendo assenza di bridge, helper eval runtime e `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_more_static_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 24/24;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.21 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora statement `switch` quando subject,
  case expressions e body appartengono al subset EIR sicuro;
- il supporto riusa il lowering EIR normale, inclusi `break`, fallthrough dei
  case e flush finale delle variabili locali create dal frammento eval;
- il folding condiviso dei builtin statici ricorre dentro subject, condizioni
  dei case, body dei case e body default;
- il collector "reads before writes" tratta `switch` in modo conservativo:
  non promuove le variabili scritte in qualche case a definite-assigned dopo lo
  switch, per evitare misclassificazioni quando manca il default o c'e'
  fallthrough condizionale;
- il gate rifiuta `default` prima di case successivi, perche' l'AST corrente
  separa `default` dai case e il lowering EIR non conserva quel fallthrough
  sorgente. Quei frammenti restano bridge fallback;
- aggiunti test per:
  - `switch` con `default` in coda, `case strlen("ab")`, `break`, flush finale
    di `$x`, assenza di `__elephc_eval_execute`;
  - `switch` con `default` prima di un case successivo, che resta fallback e
    preserva output PHP `2DF`;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_switch -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests test_eval_switch_matches_default_and_fallthrough -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 26/26;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 3/7.22 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora gli operatori numerici `/` e `**`
  quando gli operandi appartengono al subset EIR sicuro;
- lo stesso gate accetta gli operatori bitwise `&`, `|`, `^`, gli shift
  `<<`/`>>` e l'unario `~`, riusando il normale lowering EIR invece di
  introdurre codegen eval-specifico;
- gli assignment compound gia' normalizzati dal parser possono quindi usare il
  path AOT anche per `/=`, `%=`, `&=`, `|=`, `^=`, `<<=` e `>>=`, con flush
  finale nello scope eval quando il frammento crea o aggiorna variabili;
- il folding condiviso dei builtin statici ricorre anche dentro l'operando di
  `~`, in linea con gli altri operatori unari supportati;
- aggiunti test per:
  - divisione e potenza in frammenti no-scope, senza bridge, eval context o
    link a `elephc_magician`;
  - `/=` e `%=` con variabile locale creata dal frammento e visibile dopo
    `eval`;
  - bitwise, shift e operatori compound bitwise/shift con sync finale dello
    scope;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_division_and_pow_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_bitwise_shift_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 29/29.

Aggiornamento Milestone 3/7.23 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora anche l'operatore spaceship `<=>`
  quando gli operandi appartengono al subset EIR sicuro;
- il supporto riusa `Op::Spaceship` e il normale lowering EIR, senza aggiungere
  codegen specifico per eval;
- aggiunto test con:
  - confronti no-scope interi e float;
  - confronto su variabile caller `$a` tramite direct read params;
  - assenza di `__elephc_eval_*`, runtime bridge e `elephc_magician`;
- rinominato il test runtime generico da `test_eval_spaceship_execute_through_bridge`
  a `test_eval_spaceship_executes`, per non descrivere piu' come bridge un caso
  che ora puo' passare dall'AOT;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_spaceship_uses_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_spaceship_executes -- --nocapture`: passato.

Aggiornamento Milestone 4/7.24 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  anche un sottoinsieme conservativo di builtin numerici con risultato `float`:
  - `floor()` e `ceil()` su literal numerici finiti;
  - `sqrt()` su literal numerici finiti non negativi;
  - `round()` a un solo argomento su literal numerici finiti;
- il fold evita casi potenzialmente divergenti o non ancora rappresentati in
  modo stabile nel subset eval AOT: precisione esplicita di `round`, `NaN`,
  `INF`, `sqrt()` negativo, stringhe numeriche parziali e interi non
  rappresentabili esattamente come `f64`;
- il programma foldato contiene `FloatLiteral`, quindi il normale gate EIR AOT
  e il normale lowering EIR gestiscono il frammento senza codegen specifico per
  eval;
- aggiunto test namespaced/case-insensitive con `FLOOR`, `ceil`, `sqrt` e
  `round`, verificando funzione EIR AOT, assenza di helper eval, assenza di
  runtime bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_numeric_static_builtins_use_eir_aot_without_magician -- --nocapture`: passato.

Aggiornamento Milestone 0/7.25 - 2026-07-06:

- il planner condiviso `EvalAotPlan` espone ora una ragione conservativa di
  fallback leggibile, invece di distinguere solo parse error e fallback
  generico;
- il classifier target-independent riconosce le principali famiglie che devono
  restare sul bridge finche' non sono modellate nell'AOT:
  - include/require;
  - declaration runtime;
  - `global`/`static`;
  - references/by-ref;
  - dynamic calls;
  - dynamic class/member access;
  - object/member access;
  - array/iterable;
  - try/throw;
  - control-flow o scope non supportato;
- il marker assembly del fallback literal mantiene il prefisso stabile
  `eval literal AOT fallback`, ma ora include anche la ragione quando il
  planner puo' classificarla;
- aggiornato il test fallback `foreach ([1] as $x)` per verificare il marker
  `array/iterable semantics need bridge fallback`;
- verifiche:
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo check --tests`: passato.

Aggiornamento Milestone 4/7.26 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  `count()` su array literal completamente statici;
- il fold resta volutamente conservativo:
  - supporta solo la forma a un argomento, senza `COUNT_RECURSIVE`;
  - richiede che tutti i valori dell'array siano literal/fold statici senza
    side effect;
  - rifiuta spread e chiavi non letterali;
  - per array associativi normalizza le chiavi stringa-intero e deduplica le
    chiavi finali, cosi' `["1" => "a", 1 => "b"]` conta come PHP;
- il folder ricorre ora dentro array literal e assoc literal prima di tentare
  i fold builtin, cosi' array statici con elementi gia' foldabili restano
  side-effect-free;
- aggiunto test namespaced/case-insensitive con `COUNT([1, 2, 3])`, assoc
  statici, chiavi duplicate normalizzate e array annidati top-level, verificando
  funzione EIR AOT, assenza di helper eval, assenza di runtime bridge e assenza
  di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_array_count_uses_eir_aot_without_magician -- --nocapture`: passato.

Aggiornamento Milestone 4/7.27 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  anche un sottoinsieme conservativo di builtin stringa:
  - `substr()` su stringhe ASCII literal, offset non negativo e lunghezza
    opzionale non negativa;
  - `str_repeat()` su stringhe ASCII literal e repeat count non negativo, con
    limite statico sul risultato foldato per evitare di gonfiare il binario in
    compile time;
- i casi byte/locale sensibili restano volutamente fuori dal fold: stringhe
  non ASCII, offset/lunghezze negative e risultati statici troppo grandi
  continuano a usare il fallback disponibile;
- aggiunto test namespaced/case-insensitive con `SUBSTR`, `substr`,
  `str_repeat` e `strlen(str_repeat(...))`, verificando funzione EIR AOT,
  assenza di helper eval, assenza di runtime bridge e assenza di
  `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_string_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 33/33.

Aggiornamento Milestone 4/7.28 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  anche trasformazioni ASCII monargomento:
  - `ucfirst()` e `lcfirst()` su stringhe ASCII literal;
  - `trim()`, `ltrim()`, `rtrim()` e alias `chop()` con maschera PHP default;
- il fold resta limitato alla forma senza `charlist` esplicito e rifiuta
  stringhe non ASCII, cosi' non duplica la semantica completa delle maschere
  custom o di stringhe byte non rappresentabili stabilmente nel subset;
- aggiunto test namespaced/case-insensitive con first-character case, trim
  bilaterale e trim laterali, verificando funzione EIR AOT, assenza di helper
  eval, assenza di runtime bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_ascii_text_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 34/34.

Aggiornamento Milestone 4/7.29 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  anche predicate stringa binari su literal ASCII:
  - `str_contains()`;
  - `str_starts_with()`;
  - `str_ends_with()`;
- il risultato viene riscritto a `BoolLiteral` prima del gate EIR, quindi
  ternari/condizioni nel frammento possono restare sul percorso funzione EIR
  senza emettere helper runtime di ricerca stringa;
- il fold rifiuta stringhe non ASCII per mantenere il sottoinsieme byte-stable
  gia' usato dalle altre trasformazioni statiche stringa;
- aggiunto test namespaced/case-insensitive con match positivo, negativo e
  needle vuoto, verificando funzione EIR AOT, assenza di helper eval, assenza
  di runtime bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_string_predicates_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 35/35.

Aggiornamento Milestone 4/7.30 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora
  `array_key_exists()` su array literal completamente statici;
- la normalizzazione delle chiavi statiche e' stata centralizzata in
  `static_array_key_ids()`, riusata anche da `count()`:
  - array numerici producono chiavi `0..len-1`;
  - array associativi normalizzano stringhe-intero come PHP;
  - duplicate key vengono deduplicate nella vista finale;
- il fold resta limitato a chiavi literal `int`/`string` e array literal senza
  side effect, lasciando sul fallback chiavi dinamiche, spread o array non
  statici;
- aggiunto test namespaced/case-insensitive con chiavi presenti/mancanti,
  normalizzazione `"1"`/`1`, array numerici e associativi, verificando funzione
  EIR AOT, assenza di helper eval, assenza di runtime bridge e assenza di
  `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_array_count_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 36/36.

Aggiornamento Milestone 4/7.31 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT normalizza
  ora gli argomenti tramite `src/types/call_args::plan_call_args()` e le
  signature canoniche `builtin_call_sig()` prima di provare il fold;
- questo abilita named arguments e static associative spread nei fold AOT senza
  duplicare le regole PHP di matching parametri, duplicate detection,
  ordinamento named/positional e default trailing;
- i fold restano conservativi: spread dinamici o materializzazioni non
  statiche producono espressioni normalizzate non foldabili e quindi restano
  fallback come prima;
- aggiunto test con:
  - `strlen(string: "...")`;
  - `strlen(...["string" => "..."])`;
  - `count(value: [...])`;
  - `array_key_exists(array: ..., key: ...)`;
  - `str_contains(needle: ..., haystack: ...)`;
  - `str_repeat(times: ..., string: ...)`;
  - `substr(length: ..., string: ..., offset: ...)`;
  verificando funzione EIR AOT, assenza di helper eval, assenza di runtime
  bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_builtin_named_args_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_scalar_builtins_use_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 37/37.

Aggiornamento Milestone 5/7.32 - 2026-07-06:

- il gate condiviso per chiamate statiche a funzioni utente dentro literal eval
  AOT normalizza ora gli argomenti con `plan_call_args()` prima di verificare
  tipi scalar-register e arita';
- questo abilita named arguments nelle chiamate utente statiche AOT senza
  mantenere un controllo locale sugli `ExprKind::NamedArg` sorgenti;
- il controllo resta conservativo:
  - niente by-ref;
  - niente variadic user function;
  - ritorni solo `int`/`bool`/`float`/`string`;
  - argomenti normalizzati solo se restano literal scalar supportati;
- aggiunto test con funzione utente `join_named(string $left, string $right,
  bool $bang): string` chiamata da eval con named args fuori ordine,
  verificando funzione EIR AOT, assenza di helper eval, assenza di runtime
  bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_user_function_named_args_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 38/38.

Aggiornamento Milestone 5/7.33 - 2026-07-06:

- il gate condiviso per chiamate statiche a funzioni utente dentro literal eval
  AOT accetta ora anche default scalar dei parametri quando il call-site omette
  argomenti opzionali;
- per chiamate posizionali senza spread il gate estende gli argomenti con i
  default trailing gia' presenti nella `FunctionSig`, e poi applica lo stesso
  controllo scalar-register usato per gli argomenti espliciti;
- per chiamate named/static-spread il gate usa `plan_call_args(...,
  trim_trailing_defaults = false)` cosi' i default intermedi vengono
  materializzati dal planner condiviso;
- il supporto resta conservativo: default non scalar, by-ref, variadic e tipi
  non `int`/`bool`/`float`/`string` restano fuori dal subset AOT;
- aggiunto test con funzione utente `greet_default(string $name,
  string $suffix = "!", bool $loud = true): string` chiamata da eval sia con
  argomenti posizionali omessi sia con named args fuori ordine e default
  intermedio, verificando funzione EIR AOT, assenza di helper eval, assenza di
  runtime bridge e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_user_function_defaults_use_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 39/39.

Aggiornamento Milestone 3/7.34 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora incrementi/decrementi locali
  (`$i++`, `++$i`, `$i--`, `--$i`) quando la variabile e' nota come local int
  definita dal frammento eval;
- la classificazione AOT traccia ora `EirLocalFacts` invece del solo set di
  variabili assegnate:
  - `assigned` indica variabili definite nel path corrente;
  - `int_locals` indica variabili sicuramente intere, usate per accettare
    inc/dec senza aprire casi PHP string/float/bool non modellati;
- i facts vengono propagati conservativamente:
  - i loop usano una copia dei facts per il body/update;
  - gli `if` con `else` mantengono solo facts presenti in tutti i branch;
  - i `switch` non promuovono nuovi facts oltre il blocco, come prima per le
    assegnazioni;
- aggiunto test con post/pre increment e decrement, `for ($j++)` e
  `for (--$k)` in literal eval, verificando funzione EIR AOT, flush scope
  finale e assenza di `__elephc_eval_execute`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_local_inc_dec_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 40/40.

Aggiornamento Milestone 3/7.35 - 2026-07-06:

- il gate condiviso EIR AOT accetta ora i construct `isset()` ed `empty()`
  quando restano nel subset scalare gia' modellato dal normale lowering EIR;
- `isset()` viene abilitato solo per variabili locali sicuramente assegnate
  nel frammento eval:
  - niente named args o spread;
  - niente variabili mancanti del caller;
  - niente offset, property, nullsafe o object probes finche' la semantica lazy
    specifica non viene modellata nel piano AOT;
- `empty()` viene abilitato per un singolo argomento senza named/spread quando
  l'argomento e' una variabile locale assegnata o un'espressione gia' accettata
  dal gate EIR AOT;
- aggiunto test con `$zero`, `$blank`, `$value` e `$nullish` definiti dentro
  literal eval, verificando `isset`, `empty`, flush finale dello scope e
  assenza di `__elephc_eval_execute`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_local_isset_empty_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 41/41.

Aggiornamento Milestone 3/7.36 - 2026-07-06:

- il gate EIR AOT accetta ora `print` anche quando e' usato come espressione,
  non solo come `ExprStmt(print ...)`;
- la classificazione mantiene la semantica side-effectful di `print` delegando
  al normale lowering EIR, ma usa il fatto che `print` ritorna `1` per
  mantenere il tracking `int_locals` su assegnamenti come `$x = print "A";`;
- aggiunto test con `$x = print "A";` ed `echo print "B";` dentro literal eval,
  verificando output, flush finale dello scope e assenza di
  `__elephc_eval_execute`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_print_expression_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 42/42.

Aggiornamento Milestone 3/7.37 - 2026-07-06:

- il gate EIR AOT accetta ora espressioni con soppressione errori (`@expr`)
  quando l'espressione interna e' gia' nel subset AOT;
- il lowering resta quello EIR normale (`ErrorSuppressBegin` /
  `ErrorSuppressEnd`) e quindi non introduce un percorso speciale nel bridge
  eval;
- il tracking `int_locals` propaga il tipo intero attraverso `@expr` quando il
  valore interno e' noto intero, ad esempio `$x = @intval("4");`;
- aggiunto test con `@strlen("ab")` e `$x = @intval("4")` dentro literal eval,
  verificando output, flush finale dello scope e assenza di
  `__elephc_eval_execute`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_error_suppress_expression_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 43/43.

Aggiornamento Milestone 3/7.38 - 2026-07-06:

- il gate EIR AOT per `isset()` ed `empty()` accetta ora anche variabili lette
  dallo scope caller tramite direct read params, non solo variabili locali gia'
  assegnate nel frammento eval;
- le variabili caller mancanti continuano a essere materializzate come
  `Mixed null`, come gia' avveniva per `??`, quindi:
  - `isset($missing)` resta `false`;
  - `empty($missing)` resta `true`;
  - non serve creare eval scope/context runtime;
- il supporto resta ristretto alle variabili semplici: offset, property,
  nullsafe/object probes e named/spread args restano fuori dal subset AOT fino
  a modellazione lazy dedicata;
- aggiunto test con `$missing`, `$nullish`, `$zero` e `$blank` dal caller,
  verificando direct read params, assenza di helper `__elephc_eval_*`, assenza
  di runtime bridge, assenza di `elephc_magician` e output PHP corretto;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scope_isset_empty_uses_direct_params_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 6/7.40 - 2026-07-06:

- aggiunto un sottoinsieme direct local-store per literal eval write-only con
  assegnamenti statici a variabili semplici, ad esempio
  `eval('$created = "yes";'); echo $created;`;
- il classifier condiviso in `src/eval_aot.rs` riconosce questi frammenti e
  restituisce l'elenco ordinato delle write statiche con categoria scalare
  (`null`, `bool`, `int`, `float`, `string`);
- `src/ir_lower/expr/mod.rs` salta la eval barrier per questi frammenti, quindi
  non dichiara gli slot nascosti `EvalContext` / `EvalScope` /
  `EvalGlobalScope` quando non servono;
- `src/ir_lower/program.rs` evita sia la funzione AOT sintetica con
  `EvalScopeSet` sia il flag `features.eval` per lo stesso subset;
- `src/codegen_ir/lower_inst/builtins/eval.rs` emette store diretti nei local
  slot del caller per tipi scalari supportati, incluso `Mixed` tramite boxing
  core runtime, senza `__elephc_eval_scope_set` e senza
  `__elephc_eval_execute`;
- aggiunto test `test_literal_eval_scalar_store_uses_direct_local_write_without_magician`,
  con verifica di output `yes`, marker direct-local-store, assenza di
  `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute` e assenza di
  `elephc_magician` tra le librerie richieste;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scalar_store_uses_direct_local_write_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Gap residuo Milestone 6 - 2026-07-06:

- i frammenti con scope-write EIR piu' ricchi che non rientrano nel subset
  local-scalar direct-sync usano ancora il path `EvalScopeSet`;
- `src/ir_lower/program.rs` marca correttamente ogni `EvalScopeGet` /
  `EvalScopeSet` come `features.eval = true`, quindi quei casi continuano a
  linkare `elephc_magician` finche' lo scope glue AOT non verra' separato dal
  bridge interpretato o sostituito da direct write/reload piu' generale.

Aggiornamento Milestone 6/7.41 - 2026-07-06:

- aggiunto un classifier condiviso per il sottoinsieme local-scalar AOT
  (`int`/`bool`, assegnamenti semplici, `if`, `while`, `break`, `continue`,
  `echo`, `print`, `return` e chiamate statiche gia' supportate dal vecchio
  path local-scalar);
- quando il frammento rientra in questo subset, `src/ir_lower/program.rs`
  evita di generare la funzione EIR scope-aware che avrebbe introdotto
  `EvalScopeSet`, e lo scan runtime non abilita `features.eval`;
- `src/ir_lower/expr/mod.rs` evita la eval barrier per lo stesso subset solo
  se le write statiche possono essere ignorate perche' non materializzate nel
  caller oppure scritte in local PHP compatibili (`Mixed`/union, `int`, `bool`,
  o tagged int);
- `src/codegen_ir/lower_inst/builtins/eval.rs` ha ora una finalizzazione
  `local scalar with direct local sync`: il frammento gira sui suoi slot
  temporanei e, a fine esecuzione, copia le variabili definite nei local slot
  del caller senza `ElephcEvalScope`;
- il test `test_literal_eval_local_while_uses_aot_without_execute_bridge`
  legge `$sum` dopo `eval`, verificando output `55:55`, marker direct-sync,
  assenza di `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute`,
  assenza di helper eval runtime e assenza di `elephc_magician`;
- il test `test_literal_eval_prime_loop_uses_aot_without_execute_bridge`
  verifica ora il benchmark dei primi con path local-scalar direct-sync,
  output `454396537`, assenza di `__elephc_eval_scope_set`,
  assenza di `__elephc_eval_execute` e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_local_while_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_prime_loop_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 6/7.42 - 2026-07-06:

- aggiunto un sottoinsieme direct read/write per literal eval boxed con
  assegnamenti interi che leggono la stessa variabile del caller, ad esempio
  `eval('$x = $x + 1;')`;
- il classifier condiviso accetta solo espressioni intere strette
  (`literal int`, `$target`, unary minus e `+`, `-`, `*`, `%`) e non copre
  ancora `Mixed`, stringhe, concat, altri nomi caller o conversioni PHP
  implicite;
- `src/ir_lower/program.rs` salta la funzione EIR scope-aware e non abilita
  `features.eval` solo quando il target e' un local PHP `int` inizializzato
  prima dell'eval;
- `src/ir_lower/expr/mod.rs` evita la eval barrier per lo stesso subset solo
  se il local e' gia' nello snapshot di inizializzazione e ha tipo `int`;
- `src/codegen_ir/lower_inst/builtins/eval.rs` emette il marker
  `eval literal AOT compiled direct local read/write stores`, carica il local
  caller, calcola l'espressione intera target-aware e riscrive direttamente lo
  slot locale senza `EvalScopeGet`/`EvalScopeSet`;
- il test `test_literal_eval_scope_read_write_uses_aot_without_execute_bridge`
  ora verifica output `2`, assenza di ogni `__elephc_eval_*` in user/runtime
  assembly e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_scope_read_write_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 6/7.43 - 2026-07-06:

- esteso il subset local-scalar direct-sync a `do/while` e `for`, con
  semantica di `continue` corretta:
  - in `do/while`, `continue` salta al controllo finale della condizione;
  - in `for`, `continue` salta alla clausola `update` prima di tornare alla
    condizione;
- il classifier condiviso in `src/eval_aot.rs` riconosce ora `do/while`,
  `for` e builtin statici interi foldabili, come `strlen("...")`, nello stesso
  subset usato dal backend local-scalar;
- `src/codegen_ir/lower_inst/builtins/eval.rs` rappresenta ed emette
  `DoWhile` e `For` nel mini-codegen local-scalar esistente, senza introdurre
  `EvalScopeSet` o helper eval runtime;
- i test `test_literal_eval_do_while_uses_aot_without_execute_bridge`,
  `test_literal_eval_for_loop_uses_aot_without_execute_bridge`,
  `test_literal_eval_local_inc_dec_uses_aot_without_execute_bridge` e
  `test_literal_eval_static_scalar_builtins_in_local_body_use_aot` verificano
  marker `local scalar with direct local sync`, assenza di
  `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute`, assenza di
  helper `__elephc_eval_*` nel runtime e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_do_while_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_for_loop_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 6/7.44 - 2026-07-06:

- esteso il subset local-scalar direct-sync a `print` usato come espressione:
  il backend emette l'output dell'operando, poi materializza il valore PHP
  `1` nel registro risultato;
- `@expr` viene accettato nel subset local-scalar quando `expr` e' gia'
  supportato dal subset stesso; questo copre i builtin statici foldabili nei
  test senza introdurre nuova semantica di error reporting;
- `src/eval_aot.rs` e `src/codegen_ir/lower_inst/builtins/eval.rs` sono stati
  allineati: il classifier vede `print` come side effect di echo + risultato
  `int`, e il backend rappresenta `Print` come espressione local-scalar;
- i test `test_literal_eval_print_expression_uses_aot_without_execute_bridge`
  e `test_literal_eval_error_suppress_expression_uses_aot_without_execute_bridge`
  verificano ora il marker `local scalar with direct local sync`, assenza di
  `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute`, assenza di
  helper `__elephc_eval_*` nel runtime e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_print_expression_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_error_suppress_expression_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 6/7.45 - 2026-07-06:

- esteso il subset local-scalar direct-sync agli operatori interi bitwise e
  shift: `~`, `&`, `|`, `^`, `<<`, `>>`;
- il classifier condiviso accetta solo operandi `int`, evitando conversioni
  PHP implicite non modellate;
- il backend local-scalar emette lowering target-aware:
  - AArch64: `mvn`, `and`, `orr`, `eor`, `lsl`, `asr`;
  - x86_64: `not`, `and`, `or`, `xor`, `shl`, `sar`, con shift count in
    `cl`;
- il test `test_literal_eval_bitwise_shift_uses_aot_without_execute_bridge`
  verifica ora il marker `local scalar with direct local sync`, assenza di
  `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute`, assenza di
  helper `__elephc_eval_*` nel runtime e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_bitwise_shift_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44;
  - `cargo fmt`: passato con warning noti di rustfmt stabile su `ignore`.

Aggiornamento Milestone 4/7.39 - 2026-07-06:

- il folder condiviso dei builtin statici pure per literal eval AOT copre ora:
  - `gettype()` su literal `int`, `float`, `string`, `bool` e `null`, con le
    spelling PHP stabili `integer`, `double`, `string`, `boolean`, `NULL`;
  - `is_scalar()` sugli stessi literal scalari/null;
- il fold resta conservativo e non folda array/object/argomenti non literal,
  cosi' non elimina side effect di valutazione non modellati;
- esteso il test dei builtin statici con `GETTYPE(1.25)`, `gettype(null)`,
  `IS_SCALAR("x")` e `is_scalar(null)`, mantenendo funzione EIR AOT, assenza
  di helper eval e assenza di `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_more_static_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 3/7.46 - 2026-07-06:

- il percorso local-scalar direct-sync accetta ora `switch` quando subject,
  case e body appartengono al subset `int`/`bool` gia' supportato;
- il lowering conserva il fallthrough PHP tra case consecutivi ed emette un
  target `break` locale allo switch;
- `continue` dentro lo switch resta fallback conservativo per evitare semantiche
  ambigue rispetto ai loop esterni;
- `default` prima di un `case` resta fallback perche' il lowerer local-scalar
  non ricostruisce ancora quell'ordine di esecuzione PHP;
- il test `test_literal_eval_switch_uses_aot_without_execute_bridge` verifica
  ora marker `eval literal AOT compiled local scalar with direct local sync`,
  assenza di `__elephc_eval_scope_set`, assenza di `__elephc_eval_execute`,
  assenza di helper runtime `__elephc_eval_` e assenza di `elephc_magician`;
- il test `test_literal_eval_switch_default_before_case_uses_bridge_fallback`
  copre il fallback obbligatorio per `default` prima di `case`;
- residui noti dopo questa tranche:
  - `test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge`
    e' AOT senza bridge ma usa ancora sync scope boxed, perche' `/=` produce
    float PHP anche quando il divisore e' esatto; portarlo al direct-sync
    richiede storage/boxing float o un modello valore piu' ricco;
  - `test_literal_eval_local_isset_empty_uses_aot_without_execute_bridge` e'
    AOT senza bridge ma usa ancora sync scope boxed, perche' il direct-sync
    attuale non ha storage locale diretto per string/null usati da
    `isset()`/`empty()`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_switch_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_switch_default_before_case_uses_bridge_fallback -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 6/7.47 - 2026-07-06:

- esteso il percorso local-scalar direct-sync a variabili locali `string` e
  `null` con tipo stabile, usando slot temporanei piu' larghi per conservare
  pointer/length delle stringhe oltre al flag `defined`;
- aggiunto lowering diretto per `isset()` e `empty()` su variabili gia'
  assegnate nel frammento eval:
  - `isset()` controlla `defined` e rifiuta `null`;
  - `empty()` implementa truthiness PHP per `int`, `bool`, `null`, stringa
    vuota e stringa `"0"`;
- aggiunto supporto local-scalar per ternario con rami omogenei, necessario per
  forme comuni come `echo empty($x) ? "Y" : "n";`;
- il flush diretto verso i local slot del caller ora puo' materializzare anche
  stringhe e null senza passare da `__elephc_eval_scope_set`;
- il caso write-only `$created = "yes"` viene ora assorbito dallo stesso path
  local-scalar direct-sync, restando senza scope eval, senza bridge e senza
  `elephc_magician`;
- il test `test_literal_eval_local_isset_empty_uses_aot_without_execute_bridge`
  verifica ora marker `eval literal AOT compiled local scalar with direct local
  sync`, assenza di `__elephc_eval_scope_set`, assenza di
  `__elephc_eval_execute`, assenza di helper runtime `__elephc_eval_`, assenza
  di `elephc_magician` e output `InZBvL:x`;
- residuo noto dopo questa tranche:
  - `test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge`
    resta AOT senza bridge ma usa sync scope boxed, perche' `/=` richiede
    rappresentazione float corretta nel direct-sync o una migrazione piu'
    ampia al modello EIR multi-return/scope-write;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_local_isset_empty_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_scalar_store_uses_direct_local_write_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 6/7.48 - 2026-07-06:

- chiuso il residuo `/=` + `%=` nel percorso local-scalar direct-sync:
  `test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge`
  ora usa marker `eval literal AOT compiled local scalar with direct local sync`
  e non emette piu' `__elephc_eval_scope_set`;
- il classifier condiviso in `src/eval_aot.rs` accetta ora `float`, `/` come
  risultato `float` e `%` su operandi numerici con risultato `int`;
- i cambi tipo local-scalar sono permessi solo nel flusso lineare del
  frammento, mentre branch/loop/switch restano conservativi per evitare tipi
  finali path-dependent;
- il lowering local-scalar in
  `src/codegen_ir/lower_inst/builtins/eval.rs` conserva il tipo al punto di
  lettura della variabile, quindi uno stesso slot puo' transitare correttamente
  da `int` a `float` e tornare a `int`;
- aggiunti storage/load temporanei per `float`, divisione target-aware
  `d0/d1` o `xmm0/xmm1`, modulo con coercizione numerica a int e boxing
  eval-runtime/core-runtime dei float;
- il gating in `src/ir_lower/program.rs` considera supportato anche il
  direct-sync verso slot caller `float`;
- dopo questa tranche non restano asserzioni positive su
  `__elephc_eval_scope_set` nei test `literal_eval_`: i casi rimasti sono
  asserzioni negative;
- verifiche:
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 44/44.

Aggiornamento Milestone 6/7.49 - 2026-07-06:

- esteso il path boxed direct read/write per literal eval a read-modify-write
  numerici con risultato `float`, per esempio:
  `<?php $x = 20.0; eval('$x = $x / 2;'); echo $x;`;
- il classifier condiviso riconosce ora `float`, divisione e modulo anche nel
  sottoinsieme read/write, mentre il gating runtime resta conservativo per
  `Mixed`/`Union` e accetta solo casi con ultimo store numerico noto;
- il lowerer assembly per direct read/write puo' emettere divisione float,
  modulo intero e boxing del risultato prima dello store quando il local del
  caller usa storage boxed;
- `src/ir_lower/expr/mod.rs` evita `apply_eval_barrier()` anche per read/write
  diretti su local `float`, quindi non dichiara piu' hidden eval
  context/scope e non trascina helper `__elephc_eval_*`;
- aggiunto
  `test_literal_eval_float_scope_read_write_uses_direct_locals_without_magician`,
  che verifica marker `eval literal AOT compiled direct local read/write
  stores`, assenza di `__elephc_eval_` in user/runtime asm, assenza di
  `elephc_magician` e output `10`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_float_scope_read_write_uses_direct_locals_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 45/45.

Aggiornamento Milestone 3/7.50 - 2026-07-06:

- chiuso il fallback conservativo per `switch` con `default` prima di un
  `case` nel percorso local-scalar AOT;
- il parser AST conserva `default` separato dai `case`, quindi il nodo AOT
  local-scalar memorizza ora `default_index`, ricostruito dagli span del
  frammento eval;
- il lowerer emette i confronti dei `case` come prima, ma dispone le label dei
  body in ordine sorgente (`case/default/case`), preservando il fallthrough PHP
  quando il default non e' ultimo;
- il classifier condiviso `literal_fragment_local_scalar_writes_with_static_calls`
  accetta lo stesso caso, evitando `apply_eval_barrier()` e lo scan runtime
  eval quando il codegen puo' gia' emettere AOT diretto;
- il test
  `test_literal_eval_switch_default_before_case_uses_aot_without_magician`
  verifica ora marker `eval literal AOT compiled local scalar with direct local
  sync`, assenza di `__elephc_eval_execute`, assenza di helper
  `__elephc_eval_`, assenza di `elephc_magician` e output `2DF`;
- verifica:
  - `cargo test --test codegen_tests test_literal_eval_switch_default_before_case_uses_aot_without_magician -- --nocapture`: passato.
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 45/45.

Aggiornamento Milestone 3/7.51 - 2026-07-06:

- chiuso anche il fallback conservativo per `continue` dentro `switch` nel
  percorso local-scalar AOT;
- il veto era duplicato nel classifier condiviso e nel parser del mini-path
  local-scalar; ora entrambi lasciano decidere alla validazione normale di
  `Continue(level)` con lo stack dei target;
- lo switch local-scalar era gia' emesso con `continue_label == break_label`,
  come il lowering EIR normale: quindi `continue` targetta l'uscita dello
  switch, mentre `continue 2` targetta il loop esterno;
- aggiunto
  `test_literal_eval_switch_continue_uses_aot_without_magician`, che verifica
  `continue` e `continue 2` nello stesso frammento, marker direct-sync,
  assenza di `__elephc_eval_`, assenza di `elephc_magician` e output `adbcd`;
- verifica:
  - `cargo test --test codegen_tests test_literal_eval_switch_continue_uses_aot_without_magician -- --nocapture`: passato.
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 46/46.

Aggiornamento Milestone 3/7.52 - 2026-07-06:

- promosso il caso `switch` con `default` prima di un `case` anche nel percorso
  eval no-scope che passa da funzione EIR AOT, non solo nel mini-lowerer
  local-scalar;
- il lowering EIR standard dello `switch` dispone ora i body `case/default`
  secondo l'ordine sorgente ricostruito dagli span, preservando il fallthrough
  PHP quando il `default` non e' ultimo;
- corretto anche l'optimizer AST: la materializzazione dei percorsi di switch
  non assume piu' che `default` sia sempre dopo tutti i `case`, quindi le
  riscritture/fold di switch con un solo case non perdono piu' il fallthrough
  `default -> case successivo`;
- il gate `eval_aot` permette ora il caso no-scope quando gli span consentono
  di ricostruire in modo sicuro la posizione del `default`;
- aggiunti:
  - `test_switch_default_before_case_falls_through_in_source_order`;
  - `test_literal_eval_switch_default_before_case_no_scope_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests switch_default_before_case -- --nocapture`: 3/3;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 47/47;
  - `git diff --check`: passato.

Aggiornamento Milestone 3/7.53 - 2026-07-06:

- completata la stessa correzione `default` source-order anche nel CFG
  optimizer dello `switch`, usato da reachability, DCE e analisi dei tail path;
- `build_switch_cfg` mantiene gli indici dei `case` invariati, ma calcola i
  successori di fallthrough considerando la posizione sorgente del `default`:
  un `case` prima del `default` cade nel `default`, e un `default` prima di un
  `case` cade nel `case` successivo;
- aggiunto
  `test_build_switch_cfg_tracks_default_before_later_case_successors`, che
  verifica il grafo `case 1 -> default -> case 2` e la reachability entrando dal
  blocco `default`;
- verifiche:
  - `cargo test -p elephc --lib test_build_switch_cfg_tracks_default_before_later_case_successors -- --nocapture`: passato;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests switch_default_before_case -- --nocapture`: 3/3;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 47/47;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.54 - 2026-07-06:

- promosso un sottoinsieme statico di array literal nel percorso eval EIR AOT:
  accessi immediati come `[1, 2, 3][1]` e
  `["name" => "Ada"]["name"]`;
- il gate resta conservativo: non abilita `foreach`, mutazioni array, append,
  array letti dallo scope del caller o array literal restituiti direttamente;
- aggiunti helper nel classifier per riconoscere solo receiver `ArrayLiteral` /
  `ArrayLiteralAssoc` con chiavi statiche `int|string` e valori gia'
  EIR-safe;
- aggiunto
  `test_literal_eval_static_array_literal_read_uses_eir_aot_without_magician`,
  che verifica marker `eval literal AOT compiled EIR function`, assenza di
  `__elephc_eval_execute`, assenza di helper `__elephc_eval_`, assenza di
  `elephc_magician` e output `2:Ada`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_array_literal_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests array_literal_and -- --nocapture`: 3/3;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 48/48;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.55 - 2026-07-06:

- promosso anche il return di array literal statici dal percorso eval EIR AOT,
  per esempio `eval('return ["a", "b"];')` e
  `eval('return ["left" => "L", "right" => "R"];')`;
- il return resta boxed `Mixed` attraverso il normale lowering EIR della
  funzione eval AOT, quindi il caller puo' indicizzare il valore restituito
  senza bridge eval;
- il gate e' stato ristretto per gli `ArrayLiteralAssoc`: accetta solo coppie
  con chiavi esplicite stabili `int` o stringhe non intere PHP, e rifiuta le
  chiavi sintetiche generate dal parser per entry associative non keyate;
- il vincolo evita di promuovere forme con semantica PHP del next automatic key,
  come `["2" => "two", "tail"]`, che restano correttamente sul fallback bridge
  finche' il lowering EIR non modella il cursore automatico PHP completo;
- aggiunto
  `test_literal_eval_static_array_literal_return_uses_eir_aot_without_magician`,
  che verifica marker `eval literal AOT compiled EIR function`, assenza di
  `__elephc_eval_execute`, assenza di helper `__elephc_eval_`, assenza di
  `elephc_magician` e output `ab:LR`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_array_literal_return_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_assoc_array_literal_unkeyed_entries_use_next_key -- --nocapture`: passato;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests array_literal -- --nocapture`: 35/35;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 49/49;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.56 - 2026-07-06:

- promosso il vecchio test dei commenti eval da semplice verifica output
  "through bridge" a guardrail esplicito del percorso EIR AOT;
- il caso coperto usa un frammento con commenti `//`, `#`, `/* ... */` e
  `__LINE__`; e' sicuro per l'AOT attuale perche' `__LINE__` viene abbassato
  dal parser a `IntLiteral`, senza richiedere il pass globale delle magic
  constants;
- il test rinominato
  `test_literal_eval_comments_and_line_magic_use_eir_aot_without_magician`
  verifica marker `eval literal AOT compiled EIR function`, assenza di
  `__elephc_eval_execute`, assenza di helper `__elephc_eval_`, assenza di
  `elephc_magician` e output `4`;
- le altre magic constants non abbassate dal parser richiedono una sostituzione
  contestuale esplicita prima del lowering del frammento eval;
- verifica:
  - `cargo test --test codegen_tests test_literal_eval_comments_and_line_magic_use_eir_aot_without_magician -- --nocapture`: passato.

Aggiornamento Milestone 7/3.57 - 2026-07-06:

- aggiunto un planner eval AOT contestuale:
  `plan_literal_fragment_with_source_path_and_static_calls`;
- il planner contestuale parse-a il frammento literal e applica
  `magic_constants::substitute_file_and_scope_constants` quando il modulo EIR
  espone `source_path`, trasformando `__FILE__`, `__DIR__`,
  `__NAMESPACE__`, `__CLASS__`, `__TRAIT__`, `__FUNCTION__` e `__METHOD__`
  in literal prima di classificare il frammento;
- il `LoweringContext` porta ora il `source_path` canonico del modulo, cosi'
  anche la decisione `eval_literal_needs_barrier` usa la stessa classificazione
  contestuale dello scan modulo e del backend;
- aggiornati i call-site EIR contestuali in:
  - `src/ir_lower/expr/mod.rs`, per evitare una barrier eval quando il
    frammento con magic constants e' fully AOT;
  - `src/ir_lower/program.rs`, per materializzare la funzione eval AOT e per
    non marcare `RuntimeFeatures::eval` quando non serve;
  - `src/codegen_ir/lower_inst/builtins/eval.rs`, per marker/fallback e
    chiamate scope-read coerenti con il piano contestuale;
- promossi a EIR AOT senza bridge:
  - `__FILE__` / `__DIR__` dentro eval literal, con metadata di call-site;
  - magic constants di scope top-level eval (`__CLASS__`, `__NAMESPACE__`,
    `__TRAIT__`, `__FUNCTION__`, `__METHOD__`) che restano stringhe vuote
    anche se l'eval e' chiamato da un metodo namespaced;
- aggiunti/rinominati i test:
  - `test_literal_eval_magic_file_and_dir_use_eir_aot_without_magician`;
  - `test_literal_eval_scope_magic_constants_use_eir_aot_without_magician`;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests test_literal_eval_magic_file_and_dir_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_scope_magic_constants_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 52/52;
  - `cargo test --test codegen_tests eval_magic -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests scope_magic -- --nocapture`: 1/1;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.58 - 2026-07-06:

- promosso il caso PHP `echo` con lista separata da virgole nel percorso EIR
  AOT per eval literal;
- il parser rappresenta `echo "a", "b", "c";` come
  `StmtKind::Synthetic([...Echo...])`; il classifier eval AOT ora tratta
  `Synthetic` come una sequenza di statement sia nel percorso no-scope EIR sia
  nel percorso scope-write lineare;
- questo evita il fallback bridge per sintassi che non aggiunge semantica
  dinamica, ma solo raggruppa statement gia' supportati;
- unificati i vecchi test output-only per comma-echo, `print` statement e
  `return print` in
  `test_literal_eval_echo_comma_and_print_use_eir_aot_without_magician`;
- il test verifica marker `eval literal AOT compiled EIR function`, assenza di
  `__elephc_eval_execute`, assenza di helper `__elephc_eval_`, assenza di
  `elephc_magician` e output `abc:x:y1`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_echo_comma_and_print_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 53/53;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `git diff --check`: passato.

Aggiornamento fallback policy - 2026-07-06:

- separato il vecchio test legacy `array(...)` eval in due guardrail espliciti:
  - `test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician`;
  - `test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge`;
- i due casi ora hanno policy distinte:
  - `array(...)` statico viene normalizzato dal parser nello stesso AST di `[]`
    e puo' riusare il gate AOT esistente per literal array/read;
  - l'assegnamento con chiave esplicita seguita da elemento non keyato dipende
    dalla semantica PHP del next automatic key, che non va approssimata;
- verifiche:
  - `cargo test --test parser_tests parse_legacy`: 3/3;
  - `cargo test --test codegen_tests test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato.

Aggiornamento Milestone 7/3.59 - 2026-07-06:

- chiuso il fallback sintattico per la vecchia forma PHP `array(...)` quando il
  contenuto e' staticamente supportato;
- estratto il parsing degli array in `src/parser/expr/arrays.rs`, usato sia da
  `[...]` sia da `array(...)`, cosi' il resto della pipeline vede sempre
  `ExprKind::ArrayLiteral` o `ExprKind::ArrayLiteralAssoc`;
- `parse_named_expr` intercetta `array(...)` non qualificato e
  case-insensitive prima della normale logica di function-call, evitando che
  `=>` venga trattato come errore degli argomenti;
- il test
  `test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician`
  verifica marker `eval literal AOT compiled EIR function`, assenza di
  `__elephc_eval_execute`, assenza di helper `__elephc_eval_`, assenza di
  `elephc_magician` e output `b:Ada`;
- resta bridge il caso
  `test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge`,
  perche' scrive nello scope eval e dipende dalla semantica di next-key in un
  assegnamento array non ancora modellato nel percorso AOT scope-write;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test parser_tests parse_legacy`: 3/3;
  - `cargo test --test codegen_tests test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 54/54;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.60 - 2026-07-06:

- promosso il sottoinsieme di nested array literal statici nel gate eval EIR
  AOT no-scope;
- `expr_is_eir_static_array_value_safe` ora tratta `ArrayLiteral` e
  `ArrayLiteralAssoc` come sorgenti array statiche ricorsive, continuando a
  rifiutare `Spread` e chiavi associative sintetiche generate dal parser;
- questo consente forme come:
  - `eval('return [[10, 20], ["name" => "Ada"]];')`;
  - `eval('return ARRAY(array(10, 20), array("name" => "Ada"));')`;
- il test static-array-return ora verifica anche output nested `20:Ada`,
  mentre il test legacy-array verifica la forma case-insensitive `ARRAY(...)`
  e nested `array(...)` senza bridge;
- resta invariato il guardrail bridge per `array(2 => "two", "tail")` dentro
  assegnamento scope-write, perche' quello dipende ancora dal cursore PHP
  `next_auto_key` in un percorso di sincronizzazione scope piu' ampio;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_array_literal_return_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 54/54;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.61 - 2026-07-06:

- promosso in eval EIR AOT il sottoinsieme statico di array associativi con
  elementi non keyati dopo chiavi intere o stringhe-intere note;
- il parser degli array ora mantiene un cursore `next_auto_key` allineato a PHP
  8.4 per:
  - prima chiave intera negativa, dove il prossimo indice diventa `key + 1`;
  - chiavi stringa intere come `"2"`, normalizzate a chiave intera;
  - chiavi stringa non intere come `"02"`, che non avanzano il cursore;
  - chiavi negative successive che non abbassano un cursore gia' avanzato;
- il gate eval AOT ricostruisce lo stesso cursore prima di accettare chiavi
  sintetiche generate dal parser; se una chiave sintetica non corrisponde al
  cursore ricostruito, il frammento resta fallback;
- restano volutamente fuori da questa tranche chiavi `null`, `bool` e `float`
  nel gate AOT, perche' ampliano la superficie di coercioni/diagnostiche
  PHP-observable; il test runtime bridge esistente continua a coprire quei casi;
- aggiunto
  `test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician`,
  che verifica `[2 => ...]`, `[-2 => ...]`, `["2" => ...]`, `["02" => ...]`
  e `array(2 => ..., ...)` senza `__elephc_eval_execute`, senza helper eval
  bridge e senza `elephc_magician`;
- verifiche:
  - `php -r` su PHP 8.4.19 per confermare i casi `-2`, `2.7`, `true`,
    `false`, `null`, `"02"` e `"2"`;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test parser_tests next_key`: 2/2;
  - `cargo test --test parser_tests negative_key_does_not_decrease -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_assoc_array_literal_unkeyed_entries_use_next_key -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 55/55;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.62 - 2026-07-06:

- esteso il gate eval EIR AOT anche alle chiavi booleane statiche negli array
  associativi;
- il parser calcola gia' `true` come chiave intera `1` e `false` come chiave
  intera `0` per il cursore `next_auto_key`; il gate ora accetta
  `ExprKind::BoolLiteral` come chiave statica e aggiorna lo stesso cursore;
- il test parser
  `test_parse_assoc_array_next_key_after_boolean_keys` verifica che
  `[true => "yes", "tail", false => "no", "end"]` generi chiavi sintetiche
  `2` e `3`;
- il test
  `test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician`
  copre ora anche `[true => "yes", "tail"][2]` e
  `[false => "no", "tail"][1]` senza bridge;
- restano fuori dal gate AOT `float` e `null` keys:
  - `float` puo' emettere diagnostiche PHP-observable di conversione implicita;
  - `null` richiede chiave stringa vuota e non e' ancora nel subset di key
    materialization ammesso dal gate;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test parser_tests boolean_keys -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 55/55;
  - `cargo test --test codegen_tests test_eval_assoc_array_literal_unkeyed_entries_use_next_key -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.63 - 2026-07-06:

- esteso il gate eval EIR AOT alle chiavi `null` statiche negli array
  associativi;
- `null` non avanza il cursore `next_auto_key`, coerentemente con PHP 8.4:
  viene materializzato come chiave stringa vuota `""`, mentre l'entry non
  keyata successiva usa ancora la chiave automatica corrente;
- il backend EIR `HashSet` accetta ora `PhpType::Void` come chiave hash,
  materializzandola come stringa vuota persistente su entrambi i target
  supportati dal lowerer (`aarch64` e `x86_64`);
- il test parser
  `test_parse_assoc_array_null_key_does_not_advance_auto_key` verifica che
  `[null => "empty", "tail"]` mantenga `null` come chiave esplicita e generi
  la chiave sintetica `0` per `tail`;
- il test
  `test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician`
  copre ora anche `[null => "empty"][""]` e
  `[null => "empty", "tail"][0]` senza bridge eval;
- resta fuori dal gate AOT la chiave `float`, perche' in PHP 8.4 puo'
  produrre diagnostiche osservabili di conversione implicita quando perde
  precisione;
- verifiche:
  - `php -r` su PHP 8.4.19 per confermare che `null` diventa chiave `""` e non
    avanza l'auto-key;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test parser_tests null_key -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_assoc_array_literal_unkeyed_entries_use_next_key -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 55/55;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.64 - 2026-07-06:

- aggiunto un guardrail esplicito per gli assegnamenti di array statici nello
  scope eval tramite funzione EIR AOT;
- il percorso e' gia' nativo rispetto al bridge `execute`: frammenti come
  `eval('$items = ["a", "b"];')`, `eval('$map = ["name" => "Ada"];')` e
  `eval('$legacy = array("x", "y");')` vengono abbassati a funzione EIR e
  sincronizzati nello scope con `__elephc_eval_scope_set`;
- il test
  `test_literal_eval_static_array_scope_write_uses_eir_aot_scope_helpers`
  verifica:
  - marker `eval literal AOT compiled EIR function`;
  - presenza di `__elephc_eval_scope_set`;
  - assenza di `__elephc_eval_execute`;
  - output runtime `b:Ada:xy`;
- questa tranche non chiude ancora Milestone 6: gli helper di eval scope sono
  ancora forniti da `elephc_magician`, quindi il test documenta esplicitamente
  il link corrente alla libreria invece di dichiarare un percorso fully
  magician-free;
- in questa tranche restava bridge il caso con lettura interna della variabile
  array appena assegnata (`$items = array(...); echo $items[3];`), perche' il
  gate EIR consentiva array access statici su literal ma non ancora su
  variabili locali note come array;
- verifiche:
  - compilazione temporanea del frammento scope-write array per ispezionare
    marker assembly e output;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_static_array_scope_write_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 56/56;
  - `cargo test --test codegen_tests test_eval_legacy_array_literal_next_key_scope_assignment_uses_bridge -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.65 - 2026-07-06:

- chiuso il gap della tranche precedente: il classificatore EIR AOT ora tiene
  traccia anche delle variabili locali sicuramente assegnate da array literal
  statici;
- `EirLocalFacts` conserva `array_locals` accanto a `assigned` e `int_locals`;
  un assegnamento mantiene il fact solo se il valore e' un array literal
  staticamente materializzabile dal gate AOT, altrimenti lo rimuove;
- il merge dei branch conserva un local array fact solo quando la variabile e'
  presente in tutti i rami, quindi un array access successivo e' ammesso solo
  se l'assegnamento e' definitivamente avvenuto;
- `expr_is_eir_static_array_source_safe` accetta ora anche variabili locali
  tracciate come array statici, permettendo frammenti come
  `eval('$items = array(2 => "two", "tail",); echo $items[3];')` senza
  `__elephc_eval_execute`;
- il test bridge precedente e' stato promosso a regressione positiva:
  `test_literal_eval_legacy_array_literal_next_key_scope_assignment_uses_eir_aot_scope_helpers`
  verifica marker EIR AOT, presenza di `__elephc_eval_scope_set`, assenza del
  bridge execute e output runtime `tail`;
- il percorso continua a linkare `elephc_magician` per gli helper eval-scope:
  questa tranche elimina il bridge di esecuzione, non ancora la dipendenza
  dagli helper di scope;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_legacy_array_literal_next_key_scope_assignment_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 57/57;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.66 - 2026-07-06:

- esteso il gate eval EIR AOT alle chiavi float statiche solo quando PHP le
  converte a intero senza diagnostiche osservabili;
- sono accettati float literal finiti e integral-valued, inclusa la forma
  negativa (`2.0 => ...`, `-2.0 => ...`), mentre float con parte frazionaria,
  `NAN` e `INF` restano fallback bridge;
- il cursore `next_auto_key` del gate usa lo stesso intero normalizzato, quindi
  `[2.0 => "two", "tail"][3]` e `[-2.0 => "minus", "tail"][-1]` passano da
  funzione EIR AOT senza helper eval;
- `test_eval_static_array_fractional_float_key_uses_bridge_fallback` blocca il
  caso `2.7 => ...` sul bridge, perche' PHP 8.4 emette
  `Implicit conversion from float ... to int loses precision`;
- verifiche:
  - `php -r` su PHP 8.4.19 per confermare conversioni silenziose di `2.0` e
    `-2.0`, e deprecation per `2.7`, `-2.7`, `NAN`, `INF`;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_static_array_fractional_float_key_uses_bridge_fallback -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 57/57;
  - `cargo test --test codegen_tests test_eval_assoc_array_literal_unkeyed_entries_use_next_key -- --nocapture`: passato;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.67 - 2026-07-06:

- allineato il fold statico di `array_key_exists()` alla normalizzazione delle
  chiavi gia' usata dal gate array AOT;
- `static_array_key_fold_id()` ora riusa `static_integer_array_key_value()` per
  int, bool, stringhe intere, float integrali e forme negative supportate;
- `null` viene normalizzato come chiave stringa vuota `s:`, mentre stringhe non
  intere restano chiavi stringa;
- il fold resta conservativo per float frazionari, `NAN` e `INF`, cosi' non
  elimina diagnostiche PHP-observable;
- il test
  `test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician`
  copre ora anche `true`, `false`, `null`, `2.0` e `-2.0` in AOT senza helper
  eval e senza `elephc_magician`;
- lo stesso normalizzatore alimenta `count()` sugli array statici: il test
  `test_literal_eval_static_array_count_uses_eir_aot_without_magician` copre
  ora collisioni `true`/`1`, `false`/`0`, `null`/`""`, `2.0`/`2` e
  `-2.0`/`-2`;
- verifiche:
  - `php -r` su PHP 8.4.19 per confermare `array_key_exists()` con `true`,
    `false`, `null`, `2.0` e la deprecation di `2.7`;
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_array_count_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 57/57;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.68 - 2026-07-06:

- esteso il gate EIR AOT di `isset()` oltre le sole variabili semplici:
  accetta ora anche `ArrayAccess` quando l'accesso e' gia' static-array safe
  secondo `expr_is_eir_function_safe`;
- questo abilita `isset(["name" => "Ada"]["name"])` senza bridge, ma continua a
  escludere array da scope caller, oggetti `ArrayAccess` e accessi dinamici non
  gia' modellati dal subset statico;
- `empty()` non ha richiesto cambio perche' gia' delegava al controllo
  espressione generico nel caso non-variable;
- il test
  `test_literal_eval_static_array_literal_read_uses_eir_aot_without_magician`
  copre ora:
  - key presente con valore non-null -> `isset` true;
  - key presente con valore `null` -> `isset` false;
  - key mancante -> `isset` false;
  - `empty()` true su stringa vuota letta da static-array access;
  - `empty()` false su stringa non vuota letta da static-array access;
  mantenendo assenza di helper eval, runtime bridge e `elephc_magician`;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_static_array_literal_read_uses_eir_aot_without_magician -- --nocapture`: passato, poi ripassato dopo l'estensione `empty`;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 57/57;
  - `git diff --check`: passato.

Aggiornamento Milestone 7/3.69 - 2026-07-06:

- abilitato `foreach` EIR AOT per sorgenti static-array literal non vuote,
  sia indexed sia associative, quando il frammento usa lo scope-aware EIR AOT;
- il gate resta conservativo:
  - `foreach` by-ref resta fallback;
  - in questa tranche `foreach` su array letto dallo scope caller restava bridge
    fallback, poi superato dal Milestone 4/7.89;
  - array statici vuoti restano esclusi per non dichiarare key/value come
    definite quando PHP non esegue il body;
- `collect_scope_reads_before_writes` ora modella `foreach` come assegnazione
  di `value_var` e, se presente, `key_var` prima del body: le letture di
  `$item`, `$key` e `$value` dentro il body non vengono piu' classificate come
  reads dallo scope caller;
- dopo il loop, key/value vengono considerate definite solo per sorgenti literal
  statiche non vuote, mantenendo conservativo il caso dinamico;
- corretto il lowering EIR di `EvalScopeSet`: quando il valore da pubblicare
  nello scope e' gia' un `Mixed`/`Union`, il backend fa `__rt_incref` e passa
  `EVAL_SCOPE_FLAG_OWNED`; cosi' lo scope possiede una cell separata e il
  cleanup dei locals della funzione AOT non invalida il valore riletto dal
  caller;
- questa correzione e' necessaria per casi come
  `foreach (["a" => 1, "b" => 2] as $key => $value)`, dove `$key` e' un
  `Mixed` string key che deve restare visibile dopo `eval`;
- test aggiunto:
  `test_literal_eval_static_foreach_uses_eir_aot_scope_helpers`, che verifica
  output corretto, assenza di `__elephc_eval_execute`, uso di
  `__elephc_eval_scope_set`, e persistenza di `$item`, `$key`, `$value` dopo
  eval;
- aggiornato `test_eval_codegen_requires_eval_bridge` per coprire il fallback
  storico di `foreach` su array proveniente dallo scope caller, superato dal
  Milestone 4/7.89;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo test --test codegen_tests test_literal_eval_static_foreach_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 58/58.

Gap residuo Milestone 6/7.70 - 2026-07-06:

- il link a `elephc_magician` resta attivo per frammenti EIR AOT che usano
  `EvalScopeGet`/`EvalScopeSet`, anche quando non chiamano
  `__elephc_eval_execute`;
- la causa e' che `RuntimeFeatures::eval` oggi e' sovraccarico:
  - abilita il bridge eval dinamico e quindi `pcre2-*` + `elephc_magician`;
  - abilita anche hidden eval scope locals, scope helper calls, class metadata
    extra, `$argv`/`$argc` storage e probe di late static binding legati a eval;
- rimuovere `features.eval = true` da `EvalScopeGet`/`EvalScopeSet` non e'
  sufficiente: le chiamate generated continuerebbero a referenziare
  `__elephc_eval_scope_new/free/get/set`, simboli oggi esportati da
  `elephc_magician`;
- piano tecnico per chiudere Milestone 6 in modo pulito:
  1. dividere `RuntimeFeatures::eval` in almeno due feature, per esempio
     `eval_bridge` e `eval_scope`;
  2. mantenere `eval_bridge` per `__elephc_eval_execute`, eval function/class
     dynamic calls, callable/class/property helpers consumati da magician e
     PCRE richiesto da codice dinamicamente parsato;
  3. introdurre un path `eval_scope` separato per gli helper scope-only usati da
     EIR AOT;
  4. spostare o reimplementare in core runtime i soli helper scope-only
     `__elephc_eval_scope_new`, `__elephc_eval_scope_free`,
     `__elephc_eval_scope_get`, `__elephc_eval_scope_set` e la semantica
     ownership/dirty/unset minima che serve al caller reload;
  5. lasciare in `elephc_magician` il bridge completo e gli helper che richiedono
     parser/interprete dinamico;
  6. aggiornare `required_libraries_for_runtime_features()` affinche'
     `eval_scope` non aggiunga `elephc_magician` ne' PCRE;
  7. aggiungere regressioni per scope-read/write EIR AOT e `foreach` statico che
     verifichino assenza di `__elephc_eval_execute` e assenza di
     `elephc_magician` quando nessun fallback dinamico rimane;
- alternativa parziale ma meno generale: aggiungere direct caller-local sync per
  alcuni frammenti write-only oggi passati da `EvalScopeSet`; questo riduce
  casi specifici ma non elimina il bisogno di helper scope-only per EIR AOT con
  read/write dinamicamente visibili.

Aggiornamento Milestone 6/7.71 - 2026-07-06:

- introdotto lo split logico di `RuntimeFeatures::eval` in
  `RuntimeFeatures::{eval_bridge, eval_scope}`;
- `EvalScopeGet`, `EvalScopeSet` e la presenza di hidden eval scope state ora
  richiedono `eval_scope`, mentre `__elephc_eval_execute`, fallback literal,
  eval dynamic calls e helper dinamici restano sotto `eval_bridge`;
- i consumer che servono solo al bridge dinamico, come metadata extra,
  superglobal storage per `$argv`/`$argc`, probe late-static e override static
  property/called-class, ora guardano `eval_bridge` invece della vecchia feature
  unica;
- testato empiricamente il caso scope-only senza bridge libraries: il link
  fallisce perche' lo staticlib Rust trascina ancora oggetti che referenziano
  wrapper `__elephc_eval_*` e simboli PCRE;
- per questo motivo `eval_scope` oggi continua deliberatamente a linkare
  `pcre2-posix`, `pcre2-8` ed `elephc_magician`, ed emette ancora i bridge
  wrappers quando servono helper scope-only;
- questo non chiude Milestone 6, ma isola il debito residuo: il prossimo taglio
  deve spostare `__elephc_eval_scope_new/free/get/set` e la semantica minima di
  ownership/dirty scope nel core runtime, poi `eval_scope` potra' smettere di
  richiedere `elephc_magician` e PCRE;
- test aggiornato in `runtime_features`:
  `test_eval_scope_runtime_features_keep_bridge_libraries_until_core_scope_runtime`;
- verifiche:
  - `cargo fmt`: passato con i warning rustfmt gia' noti sull'opzione stabile
    `ignore`;
  - `cargo check --tests`: passato;
  - `cargo test --lib test_eval_scope_runtime_features_keep_bridge_libraries_until_core_scope_runtime -- --nocapture`: unit test passato prima di interrompere il resto del traversal workspace;
  - `cargo test --test codegen_tests test_literal_eval_static_foreach_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_array_scope_write_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 58/58.

Aggiornamento Milestone 6/7.72 - 2026-07-06:

- implementato un provider core runtime per gli helper `eval_scope` usati dal
  percorso AOT scope-only:
  - `__elephc_eval_scope_new`;
  - `__elephc_eval_scope_free`;
  - `__elephc_eval_scope_get`;
  - `__elephc_eval_scope_set`;
  - `__elephc_eval_value_null`, necessario per reload Mixed mancanti;
- `eval_scope && !eval_bridge` ora emette questi helper dal core runtime e non
  richiede piu' `pcre2-*` ne' `elephc_magician`;
- `eval_bridge` continua a usare il provider completo in `elephc_magician`, in
  modo da evitare doppie definizioni di simboli e mantenere nel bridge dinamico
  gli helper che dipendono da parser/interprete eval;
- aggiunta una barriera di lowering scope-only per literal eval AOT che hanno
  bisogno di `EvalScopeGet`/`EvalScopeSet` ma non del contesto dinamico: viene
  dichiarato solo l'hidden local `EvalScope`, evitando riferimenti a
  `__elephc_eval_context_free`;
- la finalizzazione dell'assembly EIR ora emette helper metadata/reflection,
  callable e dynamic eval solo quando `eval_bridge` e' davvero richiesto;
- aggiornate le regressioni scope-only per verificare assenza di
  `elephc_magician` e presenza degli helper core `__elephc_eval_scope_*`;
- limite residuo: il provider core copre solo `new/free/get/set` e il valore
  `null` minimo. Alias globali, `unset`, clear-dirty e semantiche piu'
  dinamiche restano bridge-only finche' non vengono portate nel subset AOT;
- verifiche:
  - `cargo test --lib test_eval_scope_runtime_features_omit_bridge_libraries -- --nocapture`: unit test passato prima di interrompere il resto del traversal workspace;
  - `cargo test --test codegen_tests test_literal_eval_static_array_scope_write_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_static_foreach_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_legacy_array_literal_next_key_scope_assignment_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 58/58.

Aggiornamento Milestone 4/7.73 - 2026-07-06:

- il classifier AOT EIR ora distingue una prima categoria di builtin statici
  runtime-safe, separata dai fold compile-time su soli literal;
- abilitato `strlen()` dentro literal eval EIR AOT quando l'argomento e'
  posizionale e arriva gia' al backend come `Str` o boxed `Mixed`, per esempio
  una variabile del caller passata tramite direct read params;
- questo copre casi come:

  ```php
  $s = "abcd";
  echo eval('return strlen($s);');
  ```

  senza passare da `__elephc_eval_execute` e senza linkare `elephc_magician`;
- il gate resta conservativo:
  - niente named/spread args in questo nuovo path runtime-safe;
  - nessuna estensione implicita agli altri builtin;
  - argomenti non string/Mixed restano fuori dal subset finche' non sono
    modellati con la stessa semantica del type checker/backend ordinario;
- test aggiunto:
  `test_literal_eval_strlen_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_strlen_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 59/59;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.74 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con `count()` su array
  concreti gia' modellabili dal subset EIR AOT;
- il nuovo caso coperto e' `count($items)` quando `$items` e' una local del
  frammento assegnata da static array literal, quindi il backend ordinario puo'
  leggere la lunghezza dell'array senza bridge;
- poiche' le variabili create dentro `eval` restano visibili nel caller, il
  path corretto e' scope-only EIR AOT: usa `__elephc_eval_scope_set` core per
  pubblicare `$items`/`$map`, ma non crea `EvalContext`, non chiama
  `__elephc_eval_execute` e non linka `elephc_magician`;
- il gate resta conservativo:
  - niente `count($callerVar)` su direct read param `Mixed`, finche' non viene
    modellata la diagnostica per non-array;
  - niente named/spread args in questo runtime-safe path;
  - array provenienti da sorgenti dinamiche restano fuori dal subset;
- test aggiunto:
  `test_literal_eval_local_array_count_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_local_array_count_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 60/60;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.75 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con `intval()` quando
  l'argomento puo' raggiungere il backend ordinario come scalar, stringa,
  null-like o boxed `Mixed`;
- il caso principale coperto e' una variabile del caller passata al frammento
  eval tramite direct read params:

  ```php
  $s = "42";
  echo eval('return intval($s) + 8;');
  ```

- questo path usa una funzione EIR AOT interna con parametri Mixed diretti,
  quindi non crea scope runtime, non chiama `__elephc_eval_execute` e non
  linka `elephc_magician`;
- il gate resta conservativo:
  - niente named/spread args;
  - array locali/statici non sono accettati come argomento `intval()`;
  - altri builtin di cast restano esclusi finche' non vengono verificati
    contro il lowerer EIR ordinario;
- test aggiunto:
  `test_literal_eval_intval_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_intval_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 61/61;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.76 - 2026-07-06:

- aggiunto supporto EIR ordinario per `floatval()` su `Mixed`/`Union`, usando
  il runtime helper gia' esistente `__rt_mixed_cast_float`;
- estesa la whitelist dei builtin statici runtime-safe con `floatval()` sugli
  stessi argomenti scalar-safe gia' ammessi per `intval()`;
- il caso eval coperto e':

  ```php
  $s = "1.5";
  echo eval('return floatval($s) + 2.25;');
  ```

  che ora usa direct read params verso una funzione EIR AOT interna, senza
  scope runtime, senza `__elephc_eval_execute` e senza `elephc_magician`;
- aggiunta anche una regressione non-eval per provare il backend ordinario:
  `floatval(json_decode("2.5")) + 0.5`;
- il gate resta conservativo:
  - niente named/spread args;
  - array locali/statici restano esclusi dagli argomenti `floatval()`;
  - `boolval()`/`strval()` non sono stati allargati in questa tranche;
- test aggiunti:
  - `test_json_decode_float_value_can_be_floatval`;
  - `test_literal_eval_floatval_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_float_value_can_be_floatval -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_floatval_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests json_decode_float -- --nocapture`: 3/3;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 62/62;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.77 - 2026-07-06:

- aggiunto supporto EIR ordinario per `boolval()` su `Mixed`/`Union`, usando
  il runtime helper gia' esistente `__rt_mixed_cast_bool`;
- estesa la whitelist dei builtin statici runtime-safe con `boolval()` sugli
  stessi argomenti scalar-safe gia' ammessi per `intval()`/`floatval()`;
- il caso eval coperto e':

  ```php
  $s = "0";
  echo eval('return boolval($s) ? "bad" : "ok";');
  ```

  che ora usa direct read params verso una funzione EIR AOT interna, senza
  scope runtime, senza `__elephc_eval_execute` e senza `elephc_magician`;
- aggiunta anche una regressione non-eval per provare il backend ordinario:
  `boolval(json_decode("true"))`, `boolval(json_decode("false"))` e la
  truthiness speciale PHP della stringa `"0"`;
- il gate resta conservativo:
  - niente named/spread args;
  - array locali/statici restano esclusi dagli argomenti `boolval()`;
  - `strval()` non e' stato allargato in questa tranche;
- test aggiunti:
  - `test_json_decode_bool_value_can_be_boolval`;
  - `test_literal_eval_boolval_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_bool_value_can_be_boolval -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_boolval_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests json_decode_bool -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 63/63;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.78 - 2026-07-06:

- verificato che `strval()` passa gia' dal cast a stringa EIR ordinario, che
  supporta `Mixed`/`Union` tramite `__rt_mixed_cast_string` e la gestione
  object-aware di `__toString()`;
- estesa la whitelist dei builtin statici runtime-safe con `strval()` sugli
  stessi argomenti scalar-safe gia' ammessi per `intval()`/`floatval()`/
  `boolval()`;
- il caso eval coperto e':

  ```php
  $s = false;
  echo eval('return "[" . strval($s) . "]";');
  ```

  che ora usa direct read params verso una funzione EIR AOT interna, senza
  scope runtime, senza `__elephc_eval_execute` e senza `elephc_magician`;
- aggiunta anche una regressione non-eval per provare `strval()` su payload
  `Mixed` da `json_decode()`:
  - numero intero;
  - `true`;
  - `false` come stringa vuota;
  - stringa JSON;
- il gate resta conservativo:
  - niente named/spread args;
  - array locali/statici restano esclusi dagli argomenti `strval()`;
  - object/string-context dinamici restano affidati al normale lowerer EIR e
    non vengono allargati nel classifier oltre i casi scalar-safe;
- test aggiunti:
  - `test_json_decode_scalar_value_can_be_strval`;
  - `test_literal_eval_strval_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_scalar_value_can_be_strval -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_strval_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests json_decode_str -- --nocapture`: 4/4;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 64/64;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.79 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con type probes scalari:
  - `gettype()`;
  - `is_int()` / `is_integer()` / `is_long()`;
  - `is_float()` / `is_double()` / `is_real()`;
  - `is_bool()`;
  - `is_null()`;
  - `is_scalar()`;
  - `is_string()`;
- il caso eval coperto e':

  ```php
  $i = 42;
  $f = 1.5;
  $b = false;
  $n = null;
  $s = "hi";
  echo eval('return gettype($i) . ":" .
      (is_integer($i) ? "I" : "bad") .
      (is_double($f) ? "D" : "bad") .
      (is_bool($b) ? "B" : "bad") .
      (is_null($n) ? "N" : "bad") .
      (is_scalar($s) ? "S" : "bad") .
      (is_string($s) ? "T" : "bad");');
  ```

  che ora usa direct read params verso una funzione EIR AOT interna, senza
  scope runtime, senza `__elephc_eval_execute` e senza `elephc_magician`;
- corretto un bug nel fold custom dei builtin statici di `eval_aot`: `is_*()`
  su argomenti non literal veniva foldato a `false`; ora i non-literal
  ritornano `None` e arrivano al lowerer EIR runtime-safe;
- aggiunta anche una regressione non-eval per provare `gettype()` e gli
  `is_*()` su payload `Mixed` da `json_decode()`;
- il gate resta conservativo:
  - niente named/spread args;
  - argomenti limitati agli stessi casi scalar-safe usati dai cast runtime-safe;
  - `is_array()`/`is_object()`/`is_iterable()` restano fuori da questa tranche;
- test aggiunti:
  - `test_json_decode_scalar_type_predicates_accept_mixed`;
  - `test_literal_eval_type_probes_scope_read_use_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_scalar_type_predicates_accept_mixed -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_type_probes_scope_read_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 65/65;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.80 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con `is_array()` solo
  per sorgenti array gia' materializzabili dal percorso EIR AOT:
  - literal array statici;
  - variabili locali del frammento assegnate da literal array;
- il caso eval coperto e':

  ```php
  echo eval('$a = [1, 2]; return is_array($a) ? "A" : "bad";');
  ```

  che ora passa dalla funzione EIR AOT interna e non chiama
  `__elephc_eval_execute`;
- a differenza dei type probe scalari su read-only caller scope, questo caso
  usa ancora `__elephc_eval_scope_set`: l'assegnazione `$a = [...]` dentro
  `eval` deve creare/aggiornare `$a` nello scope del chiamante secondo
  semantica PHP;
- aggiunta una regressione non-eval per provare `is_array()` su payload
  `Mixed` da `json_decode()`;
- il gate resta conservativo:
  - niente named/spread args;
  - niente `is_array($callerVar)` direct-read finche' non esiste un path sicuro
    per distinguere array/scalari dal valore scope letto;
  - niente `is_object()`/`is_iterable()` in questa tranche;
- test aggiunti:
  - `test_json_decode_array_value_can_be_is_array`;
  - `test_literal_eval_is_array_local_array_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_array_value_can_be_is_array -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_is_array_local_array_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 66/66;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.81 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con `is_iterable()`
  solo per la parte array-safe gia' coperta da `is_array()`:
  - literal array statici;
  - variabili locali del frammento assegnate da literal array;
- il caso eval coperto e':

  ```php
  echo eval('$a = [1, 2]; return is_iterable($a) ? "T" : "bad";');
  ```

  che passa dalla funzione EIR AOT interna, non chiama
  `__elephc_eval_execute` e non linka `elephc_magician`;
- resta intenzionalmente escluso `is_iterable($callerVar)` direct-read quando
  il valore puo' essere oggetto/Iterator, perche' quel caso richiede ancora una
  prova scope/object-safe separata;
- resta fuori anche `is_object()` in eval AOT runtime-safe;
- aggiunta una regressione non-eval per provare `is_iterable()` su payload
  `Mixed` array da `json_decode()`;
- test aggiunti:
  - `test_json_decode_array_value_can_be_is_iterable`;
  - `test_literal_eval_is_iterable_local_array_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_array_value_can_be_is_iterable -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_is_iterable_local_array_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 67/67;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.82 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con `is_object()` per:
  - valori scalar/literal/array gia' gestiti dal subset EIR, con risultato
    staticamente falso quando non sono oggetti;
  - caller-scope reads passati come direct read params `Mixed`, cosi' il
    lowerer ordinario puo' controllare il tag object a runtime senza
    `eval_scope`;
- il caso eval coperto e':

  ```php
  $o = json_decode("{}");
  $i = 42;
  echo eval('return (is_object($o) ? "O" : "bad") . ":" .
      (is_object($i) ? "bad" : "I");');
  ```

  che passa dalla funzione EIR AOT interna con direct read params, senza
  `__elephc_eval_execute`, senza `__elephc_eval_scope_*` e senza
  `elephc_magician`;
- resta esclusa la creazione/lettura di proprieta' oggetto dentro eval AOT,
  perche' quello e' un tema object/member semantics separato;
- aggiunta una regressione non-eval per provare `is_object()` su payload
  `Mixed` object da `json_decode()`;
- test aggiunti:
  - `test_json_decode_object_value_can_be_is_object`;
  - `test_literal_eval_is_object_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_object_value_can_be_is_object -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_is_object_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 68/68;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.83 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con:
  - `is_numeric()` su valori scalar-safe e caller-scope reads direct-param;
  - `is_resource()` su valori scalar-safe e caller-scope reads direct-param;
- il caso eval coperto e':

  ```php
  $n = "42";
  $s = "abc";
  $h = fopen("php://memory", "r+");
  echo eval('return (is_numeric($n) ? "N" : "bad") .
      (is_numeric($s) ? "bad" : "S") . ":" .
      (is_resource($h) ? "H" : "bad");');
  ```

  che passa dalla funzione EIR AOT interna con direct read params, senza
  `__elephc_eval_execute`, senza `__elephc_eval_scope_*` e senza
  `elephc_magician`;
- il gate resta conservativo:
  - array locali/statici non vengono aperti per `is_numeric()` anche se PHP
    restituirebbe staticamente `false`;
  - `is_nan()`/`is_finite()`/`is_infinite()` restano fuori da questa tranche
    perche' richiedono coercione numerica/float, non solo type-probe;
- aggiunta una regressione non-eval per provare `is_numeric()` su payload
  `Mixed` scalari da `json_decode()`;
- test aggiunti:
  - `test_json_decode_scalar_value_can_be_is_numeric`;
  - `test_literal_eval_numeric_resource_probes_scope_read_use_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_json_decode_scalar_value_can_be_is_numeric -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_numeric_resource_probes_scope_read_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 69/69;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.84 - 2026-07-06:

- estesa la whitelist dei builtin statici runtime-safe con:
  - `is_nan()`;
  - `is_finite()`;
  - `is_infinite()`;
- il supporto e' volutamente limitato ad argomenti numerici provati dentro il
  frammento AOT:
  - literal `int`/`float`/`bool`, inclusi `NAN`, `INF` e `-INF`;
  - variabili locali del frammento assegnate da valori `int`/`float`;
  - cast espliciti `(int)`, `(float)`, `(bool)` gia' abbassabili dal subset EIR;
- il caso eval coperto e':

  ```php
  echo eval('$nan = NAN; $inf = INF; $num = 2.5;
  return (is_nan($nan) ? "N" : "bad") .
      (is_infinite($inf) ? "I" : "bad") .
      (is_finite($num) ? "F" : "bad") .
      (is_finite($inf) ? "bad" : "f");');
  ```

  che passa dalla funzione EIR AOT interna, non chiama
  `__elephc_eval_execute` e non linka `elephc_magician`;
- aggiunto un guardrail esplicito per non usare direct read params su stringhe
  del caller:

  ```php
  $s = "abc";
  echo eval('return is_finite($s) ? "bad" : "ok";');
  ```

  resta bridge fallback, perche' PHP 8.4 lancia `TypeError` per stringhe non
  numeriche in queste funzioni mentre il lowerer `Mixed` passerebbe da cast a
  float;
- test aggiunti:
  - `test_literal_eval_float_predicates_local_values_use_eir_aot_without_magician`;
  - `test_literal_eval_float_predicates_scope_string_use_bridge_fallback`;
- verifiche:
  - `php -r` su PHP 8.4 per confermare il comportamento TypeError delle
    stringhe non numeriche con `is_nan()`/`is_finite()`/`is_infinite()`;
  - `cargo test --test codegen_tests test_literal_eval_float_predicates_local_values_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_float_predicates_scope_string_use_bridge_fallback -- --nocapture`: passato;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 71/71;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.85 - 2026-07-06:

- esteso il path EIR AOT direct-read per `is_array()` quando l'argomento e'
  una variabile dello scope chiamante:
  - il classifier `eval_aot` ora considera sicuro il probe `is_array($x)` anche
    quando `$x` arriva come read-param `Mixed`;
  - il lowering IR accetta array/hash tra i tipi che possono essere passati come
    parametri diretti al frammento eval AOT;
  - il codegen eval allinea la whitelist dei locals sincronizzabili, evitando il
    disallineamento in cui il plan rimuoveva il barrier ma la funzione AOT non
    trovava la sorgente `$items`;
- caso coperto:

  ```php
  $items = [1, 2];
  $n = 42;
  echo eval('return (is_array($items) ? "A" : "bad") . ":" .
      (is_array($n) ? "bad" : "N");');
  ```

  passa tramite funzione EIR AOT con direct read params, non chiama
  `__elephc_eval_execute`, non alloca `__elephc_eval_context_new` e non linka
  `elephc_magician`;
- `is_iterable()` su variabili caller-scope e' stato lasciato fuori da questa
  tranche durante il primo giro; il passo successivo 4/7.86 completa quel caso
  dopo aver allineato le whitelist array/object tra classifier, lowering e
  codegen;
- test aggiunto:
  - `test_literal_eval_is_array_scope_read_uses_eir_aot_without_magician`;
- regressione verificata:
  - `test_literal_eval_is_iterable_local_array_uses_eir_aot_without_magician`
    resta verde, quindi il supporto eval-local array per `is_iterable()` non e'
    stato ridotto;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_is_array_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_is_iterable_local_array_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 72/72;
  - `cargo check --tests`: passato;
  - `git diff --check`: passato;
  - scansione whitespace/conflitti sui file toccati: pulita.

Aggiornamento Milestone 4/7.86 - 2026-07-06:

- esteso anche `is_iterable()` sul path EIR AOT direct-read per valori dello
  scope chiamante:
  - array/list e hash arrivano come `Mixed` e passano dai tag runtime 4/5;
  - oggetti arrivano come `Mixed` con tag runtime 6 e vengono verificati tramite
    i metadati `Iterator`/`IteratorAggregate` gia' usati dal lowerer EIR;
  - scalari caller-scope restano falsi senza fallback;
- il supporto ha richiesto di tenere coerenti tre predicati separati:
  - `eval_aot` per decidere se generare la funzione scope-read AOT;
  - `ir_lower::expr` per evitare il barrier eval completo quando i read params
    sono davvero passabili;
  - `codegen_ir::lower_inst::builtins::eval` per ritrovare le sorgenti locali
    array/object al momento della chiamata direct-param;
- caso coperto:

  ```php
  class EvalAotDirectIterator implements Iterator { /* metodi Iterator */ }
  $items = [1, 2];
  $iterator = new EvalAotDirectIterator();
  $n = 42;
  echo eval('return (is_iterable($items) ? "A" : "bad") .
      (is_iterable($iterator) ? "I" : "bad") .
      (is_iterable($n) ? "bad" : "N");');
  ```

  produce `AIN`, usa la funzione EIR AOT con direct read params, non chiama
  `__elephc_eval_execute`, non alloca `__elephc_eval_context_new` e non linka
  `elephc_magician`;
- test aggiunto:
  - `test_literal_eval_is_iterable_scope_read_uses_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_is_iterable_scope_read_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 73/73;
  - `cargo check --tests`: passato;
  - `git diff --check`: passato;
  - scansione whitespace/conflitti sui file toccati: pulita.

Aggiornamento Milestone 4/7.87 - 2026-07-06:

- consolidata la copertura dei type-probe read-only su valori dello scope
  chiamante non scalari:
  - `gettype($items)` con `$items` array caller-scope restituisce `array`;
  - `gettype($o)` con `$o` object/Mixed caller-scope restituisce `object`;
  - `is_scalar($items)` e `is_scalar($o)` restano falsi senza fallback;
- non e' stato necessario nuovo lowering: il lavoro 4/7.85 e 4/7.86 aveva gia'
  allineato boxing direct-param, type whitelist e source lookup per array/object;
  questa tranche rende esplicita la regressione nel test esistente;
- test esteso:
  - `test_literal_eval_type_probes_scope_read_use_eir_aot_without_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_type_probes_scope_read_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 73/73;
  - `cargo check --tests`: passato;
  - `git diff --check`: passato;
  - scansione whitespace/conflitti sui file toccati: pulita.

Aggiornamento verifica target-aware - 2026-07-06:

- aggiunta evidenza Linux x86_64 Docker per i nuovi casi direct-read su scope
  caller non scalare:
  - `./scripts/test-linux-x86_64.sh literal_eval_is_array_scope`: passato;
  - `./scripts/test-linux-x86_64.sh literal_eval_is_iterable_scope`: passato;
  - `./scripts/test-linux-x86_64.sh literal_eval_type_probes_scope`: passato;
- i runner Docker x86_64 sono usciti senza container residui;
- la verifica locale Linux ARM64 per i casi direct-read resta demandata alla
  matrice CI o a filtri Docker dedicati prima di cambiare quel sottoinsieme.

Aggiornamento verifica target-aware prime-loop - 2026-07-06:

- il test prime-sum literal eval (`100000`, output `454396537`, no bridge, no
  `elephc_magician`) e' passato sul target locale macOS ARM64:
  - `cargo test --test codegen_tests codegen::eval::test_literal_eval_prime_loop_uses_aot_without_execute_bridge -- --exact --nocapture`;
- lo stesso filtro e' passato su Linux x86_64 via Docker:
  - `./scripts/test-linux-x86_64.sh test_literal_eval_prime_loop_uses_aot_without_execute_bridge`;
- lo stesso filtro e' passato su Linux ARM64 via Docker:
  - `./scripts/test-linux-arm64.sh test_literal_eval_prime_loop_uses_aot_without_execute_bridge`;
- i run Docker hanno emesso solo i warning preesistenti su `libc::time_t` in
  `elephc-magician`.

Aggiornamento benchmark prime-loop - 2026-07-06:

- ricostruito `target/release/elephc` con `cargo build --release`;
- eseguito benchmark manuale fuori repo, senza aggiungere il caso alla suite
  permanente, sullo stesso workload prime-sum fino a `100000`;
- output verificato identico in tutti i casi: `454396537`;
- mediane su 12 run:
  - Elephc standard: `13.35 ms`;
  - Elephc via eval literal AOT: `9.74 ms`;
  - PHP standard: `97.77 ms`;
  - PHP via eval: `117.76 ms`;
- RSS sample `Elephc via eval`: `1572864` byte di maximum resident set size.

Aggiornamento Milestone 4/7.88 - 2026-07-06:

- esteso il path EIR AOT direct-read per `count($callerArray)` senza aprire
  `count($callerScalar)`:
  - il plan `eval_aot` registra ora un set di read scope che devono essere
    array-like quando il frammento contiene `count($nome)` su una variabile
    proveniente dal caller;
  - il classifier accetta `count($scopeRead)` solo come builtin `count()`, senza
    promuovere genericamente `$scopeRead` a sorgente array per altri usi come
    `$scopeRead[0]`;
  - il lowering AST->EIR, la feature discovery e il backend EIR controllano il
    vincolo contro i tipi locali del caller prima di scegliere il direct-param
    AOT;
  - se il caller ha uno scalare, il call-site resta sul bridge e continua a
    linkare `elephc_magician`;
- test aggiunti:
  - `test_literal_eval_count_scope_read_array_uses_eir_aot_without_magician`;
  - `test_literal_eval_count_scope_read_scalar_keeps_bridge_fallback`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_count_scope_read_array_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_count_scope_read_scalar_keeps_bridge_fallback -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_count -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests array_count_uses_eir_aot_without_magician -- --nocapture`: 2/2;
  - `cargo check --tests`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 75/75;
  - `git diff --check`: passato;
  - scansione whitespace/conflitti sui file toccati: pulita.

Aggiornamento Milestone 4/7.89 - 2026-07-06:

- abilitato `foreach ($callerArray as ...)` nel path EIR AOT scope-aware quando
  la sorgente letta dallo scope caller soddisfa il vincolo array-like gia'
  usato per `count($callerArray)`;
- il classifier registra il read array-constrained per la sorgente del
  `foreach`, ma non marca key/value come definitely-assigned dopo il loop se la
  sorgente non e' una static-array literal non vuota;
- il lowering/codegen controllano il vincolo array anche per il path con runtime
  eval scope, cosi' `foreach ($callerScalar as ...)` resta fallback bridge;
- corretto il preambolo EIR di `foreach`: l'inizializzazione tecnica dei loop
  locals a boxed `null` ora aggiorna solo lo slot locale e non pubblica nello
  scope eval, altrimenti `foreach ([] as $kept)` sovrascriveva il caller con
  `null`;
- il call site scope-aware pre-flusha anche i nomi scritti gia' presenti nel
  caller, oltre ai nomi letti, in modo che una scrittura condizionale non
  eseguita lasci invariato il valore dopo il reload;
- test aggiunti:
  - `test_literal_eval_foreach_scope_array_uses_eir_aot_scope_helpers`, che
    verifica output corretto, `__elephc_eval_scope_get/set`, assenza di
    `__elephc_eval_execute`, assenza di `elephc_magician`, caso array vuoto e
    key/value su array associativo del caller;
  - `test_literal_eval_foreach_scope_scalar_keeps_bridge_fallback`, che mantiene
    il bridge su sorgente scalare;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_foreach_scope_array_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests test_literal_eval_foreach_scope_scalar_keeps_bridge_fallback -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_foreach -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests eval_foreach -- --nocapture`: 9/9;
  - `cargo test --test codegen_tests test_literal_eval_static_foreach_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 77/77;
  - `cargo check --tests`: passato;
  - `./scripts/test-linux-x86_64.sh literal_eval_foreach`: 2/2 sui test
    `codegen_tests`, ripassato dopo l'estensione key/value associativa;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.90 - 2026-07-06:

- esteso il path EIR AOT direct-param per i predicati IEEE float
  `is_nan()`, `is_infinite()` e `is_finite()` quando l'argomento letto dallo
  scope caller e' un local inizializzato di tipo `int` o `float`;
- il planner registra ora `float_predicate_read_constraints`, separato dai
  vincoli array usati da `count()`/`foreach`, cosi' il classifier puo' accettare
  `$scopeRead` ma lowering, feature scan e backend verificano il tipo concreto
  del caller prima di scegliere AOT;
- il vincolo resta conservativo: stringhe del caller, ref-bound locals,
  superglobal/global alias e locals non inizializzati restano fallback bridge,
  preservando i `TypeError` PHP-observable;
- test aggiunto:
  - `test_literal_eval_float_predicates_scope_numeric_use_eir_aot_without_magician`,
    che copre `NAN`, `INF`, float finite e int dal caller via direct read params
    senza `__elephc_eval_execute`, senza eval-scope helper e senza
    `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_float_predicates -- --nocapture`: 3/3, ripassato dopo `rustfmt`;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 78/78;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs src/ir_lower/expr/mod.rs src/ir_lower/program.rs src/codegen_ir/lower_inst/builtins/eval.rs tests/codegen/eval.rs`: passato con i warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.91 - 2026-07-06:

- abilitato `foreach ([])` / `foreach ([] as ...)` nel path EIR AOT no-scope;
- il planner ora distingue le sorgenti foreach staticamente vuote:
  - la sorgente array viene ancora analizzata;
  - il body resta richiesto nel subset EIR perche' viene comunque abbassato dal
    backend;
  - reads/writes del body non vengono registrati nello scope eval, perche' il
    body e' runtime-irraggiungibile;
  - key/value non vengono registrati come scritture e quindi non vengono flushati
    nel caller;
- questo rimuove il vecchio fallback per array statici vuoti senza cambiare il
  comportamento dei foreach statici non vuoti, che continuano a sincronizzare
  key/value tramite eval-scope helper;
- test aggiunto:
  - `test_literal_eval_static_empty_foreach_uses_eir_aot_without_scope_helpers`,
    che verifica EIR AOT, assenza di `__elephc_eval_*`, assenza di
    `elephc_magician` e preservazione della variabile caller `$kept`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_static_empty_foreach_uses_eir_aot_without_scope_helpers -- --nocapture`: passato, ripassato dopo `rustfmt`;
  - `cargo test --test codegen_tests test_literal_eval_static_foreach_uses_eir_aot_scope_helpers -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_foreach -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_ -- --nocapture`: 79/79;
  - `cargo check --tests`: passato.

Aggiornamento Milestone 4/7.92 - 2026-07-06:

- esteso il subset EIR AOT per `array_key_exists()` su array letti dallo scope
  caller:
  - `array_key_exists(1, $callerIndexed)` puo' usare direct read params quando il
    caller ha un local array inizializzato;
  - `array_key_exists("name", $callerAssoc)` puo' usare direct read params quando
    il caller ha un local associative-array inizializzato;
- il planner registra un vincolo separato `assoc_array_read_constraints`, oltre
  al vincolo array-like gia' usato da `count()`/`foreach`, cosi' string-key probes
  su indexed array restano bridge fallback invece di entrare in un path AOT con
  semantica numeric-string incompleta;
- aggiunto lowering target-aware di `array_key_exists()` su receiver `Mixed`:
  - unbox del receiver;
  - tag `4` -> `__rt_array_key_exists` per chiavi `int`/`bool`;
  - tag `5` -> `__rt_hash_get` per chiavi `int`/`bool`/`string`;
  - altri tag -> `false`;
  - il payload array/hash viene preservato su stack temporaneo mentre la chiave
    viene materializzata, senza aggiungere un nuovo helper runtime globale;
- il gate resta conservativo:
  - chiavi dinamiche (`array_key_exists($k, $items)`) restano fuori da questa
    tranche;
  - `null`/`float` keys sui caller-scope arrays restano fuori finche' il backend
    runtime non modella tutta la normalizzazione PHP;
  - caller scalari e string-key probes su indexed arrays restano fallback bridge;
- test aggiunti:
  - `test_literal_eval_array_key_exists_scope_read_array_uses_eir_aot_without_magician`,
    che copre indexed array, associative array, chiave presente con valore `null`,
    miss, direct-read EIR AOT, assenza di `__elephc_eval_*` e assenza di
    `elephc_magician`;
  - `test_literal_eval_array_key_exists_scope_read_scalar_keeps_bridge_fallback`;
  - `test_literal_eval_array_key_exists_string_key_indexed_scope_keeps_bridge_fallback`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_array_key_exists_scope_read_array_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_array_key_exists_ -- --nocapture`: 3/3;
  - `cargo test --test codegen_tests test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 7/7;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs src/ir_lower/expr/mod.rs src/ir_lower/program.rs src/codegen_ir/lower_inst/builtins/eval.rs src/codegen_ir/lower_inst/builtins/arrays/key_exists.rs tests/codegen/eval.rs`: passato con i warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.93 - 2026-07-06:

- chiuso un pezzo del gap lasciato in 4/7.92: `array_key_exists()` su array letti
  dallo scope caller ora puo' restare in EIR AOT anche con:
  - chiave statica `null`, limitata a receiver associativi per modellare la
    normalizzazione PHP a chiave stringa vuota;
  - chiavi statiche `float` integralmente rappresentabili, incluse forme negate
    come `-2.0`, abbassate a integer key prima del probe indexed/hash;
- il lowering target-aware e' stato esteso su AArch64 e x86_64:
  - indexed arrays convertono le chiavi float con `fcvtzs`/`cvttsd2si` prima di
    chiamare `__rt_array_key_exists`;
  - associative arrays materializzano `null` come stringa vuota e float integrali
    come integer key prima di chiamare `__rt_hash_get`;
  - receiver `Mixed` accetta ora chiavi `int`/`bool`/`float` su indexed/hash e
    forza `string`/`null` solo sul path hash-only;
- il gate resta conservativo sui float frazionari (`2.7`, ecc.): restano bridge
  fallback per preservare la deprecation PHP sulla conversione float->int con
  perdita di precisione;
- test aggiunti:
  - `test_literal_eval_array_key_exists_scope_read_null_and_float_keys_use_eir_aot`,
    che copre `1.0` su indexed array, `null` su assoc array e `-2.0` su assoc
    array via direct-read EIR AOT, senza `__elephc_eval_*` e senza
    `elephc_magician`;
  - `test_literal_eval_array_key_exists_fractional_float_key_keeps_bridge_fallback`,
    che mantiene `array_key_exists(2.7, $items)` sul bridge;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_array_key_exists_ -- --nocapture`: 5/5;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 9/9;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs src/codegen_ir/lower_inst/builtins/arrays/key_exists.rs tests/codegen/eval.rs`: passato con i warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.94 - 2026-07-06:

- il gate EIR AOT per builtin runtime ora normalizza named arguments tramite
  `builtin_call_sig()` + `plan_call_args()` prima di applicare i controlli gia'
  esistenti sugli argomenti in ordine firma;
- la normalizzazione e' condivisa anche dal collector dei vincoli array, cosi'
  `array_key_exists(array: $map, key: "name")` registra il vincolo associativo
  sul parametro `array` corretto e non sulla posizione sorgente;
- questa tranche resta volutamente fixed-arity/no-spread:
  - spread/unpack continuano a restare fallback;
  - builtin con default opzionali restano da abilitare uno alla volta quando il
    backend EIR accetta la forma materializzata;
- test aggiunto:
  - `test_literal_eval_named_runtime_builtins_use_eir_aot_without_magician`,
    che copre `boolval(value: $flag)`,
    `array_key_exists(array: $map, key: "name")` e
    `array_key_exists(key: "missing", array: $map)` via direct-read EIR AOT,
    senza `__elephc_eval_*` e senza `elephc_magician`;
- verifiche:
  - `cargo test --test codegen_tests test_literal_eval_named_runtime_builtins_use_eir_aot_without_magician -- --nocapture`: passato;
  - `cargo test --test codegen_tests literal_eval_array_key_exists_ -- --nocapture`: 5/5;
  - `cargo test --test codegen_tests literal_eval_named_runtime_builtins -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 9/9;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs tests/codegen/eval.rs`: passato con i
    warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.95 - 2026-07-06:

- chiuso il caso opzionale piu' piccolo lasciato aperto in 4/7.94:
  `count(value: $items)` e `count(value: $items, mode: 0)` possono ora restare
  in EIR AOT quando `$items` e' un array/hash noto dal caller;
- il classifier accetta `count()` normalizzato con uno o due argomenti solo se
  il `mode` opzionale e' il default literal `0`; il collector dei vincoli array
  registra comunque il vincolo sul parametro `value`;
- il lowerer EIR `count` accetta ora uno o due operandi e ignora il secondo
  solo dopo aver verificato che sia `ConstI64(0)`;
- il `mode` ricorsivo/non-zero resta fallback bridge finche' l'EIR non modella
  `COUNT_RECURSIVE` per array annidati, oggetti `Countable` e `Mixed`;
- test aggiunti/aggiornati:
  - `test_literal_eval_count_named_default_mode_uses_eir_aot_without_magician`,
    che copre `count(value: $items)` e `count(value: $items, mode: 0)` via
    direct-read EIR AOT senza helper eval e senza `elephc_magician`;
  - `test_literal_eval_count_named_recursive_mode_keeps_bridge_fallback`, che
    mantiene `count(value: $items, mode: 1)` sul bridge;
  - `test_literal_eval_named_runtime_builtins_use_eir_aot_without_magician` ora
    include anche `count(value: $items)`;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_count_named -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_count -- --nocapture`: 4/4;
  - `cargo test --test codegen_tests literal_eval_named_runtime_builtins -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 9/9;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs src/codegen_ir/lower_inst/builtins.rs tests/codegen/eval.rs`: passato con i warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.96 - 2026-07-06:

- il gate EIR AOT per builtin runtime non scarta piu' gli spread prima del
  planner: ora passa sempre da `plan_call_args()` quando la chiamata contiene
  named args o spread;
- gli spread statici espandibili dal planner, per esempio
  `count(...["value" => $items])`,
  `boolval(...["value" => $flag])` e
  `array_key_exists(...["array" => $map, "key" => "name"])`, possono quindi
  restare in direct-read EIR AOT senza bridge;
- gli spread dinamici o non espansi dal planner restano fallback bridge perche'
  `normalize_eir_runtime_builtin_args()` rifiuta ancora ogni `Spread` residuo
  dopo la normalizzazione;
- test aggiunti:
  - `test_literal_eval_static_spread_runtime_builtins_use_eir_aot_without_magician`,
    che copre `count`, `boolval` e `array_key_exists` con spread statico
    associativo via direct-read EIR AOT, senza helper eval e senza
    `elephc_magician`;
  - `test_literal_eval_dynamic_spread_runtime_builtin_keeps_bridge_fallback`,
    che mantiene `count(...$args)` sul bridge;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_static_spread_runtime_builtins -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_dynamic_spread_runtime_builtin -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_named_runtime_builtins -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_count_named -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 9/9;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs tests/codegen/eval.rs`: passato con i
    warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 5/7.97 - 2026-07-06:

- il gate per chiamate statiche a funzioni utente dentro literal eval AOT ora
  rifiuta esplicitamente gli spread rimasti dinamici dopo `plan_call_args()`,
  invece di dipendere solo dal controllo finale sui literal scalar;
- gli spread statici espandibili dal planner, per esempio
  `join_static_spread(...["right" => "B", "left" => "A"])`, restano nel subset
  EIR AOT e possono materializzare anche default scalar opzionali;
- gli spread dinamici, per esempio `join_dynamic_spread(...$args)`, restano sul
  bridge finche' il percorso EIR AOT non modella runtime unpack, evaluation
  order e controlli di arita'/named args dinamici;
- test aggiunti:
  - `test_literal_eval_static_user_function_static_spread_args_use_aot_without_magician`,
    che copre named static spread e default scalar in una user function via EIR
    AOT, senza helper eval e senza `elephc_magician`;
  - `test_literal_eval_static_user_function_dynamic_spread_args_keep_bridge_fallback`,
    che mantiene lo spread dinamico sul bridge;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_static_user_function_static_spread_args -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_static_user_function_dynamic_spread_args -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_static_user_function_named_args -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_static_user_function_defaults -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_static_scalar_user_functions -- --nocapture`: 1/1;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs tests/codegen/eval.rs`: passato con i
    warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/7.98 - 2026-07-06:

- chiuso il fallback per `array_key_exists("1", $items)` quando `$items` e' un
  indexed array caller-scope: la string key ora puo' restare in direct-read EIR
  AOT;
- il backend EIR condiviso di `array_key_exists()` normalizza le string key su
  array indicizzati con `__rt_hash_normalize_key()`:
  - stringhe intere canoniche, come `"1"`, diventano bounds-check tramite
    `__rt_array_key_exists`;
  - stringhe non intere o con leading zero non canonico, come `"x"` e `"01"`,
    restituiscono `false` sugli indexed array senza provare un hash lookup;
- il dispatch Mixed indexed/assoc ora tratta `Str` come chiave valida anche sul
  ramo indexed; `null` resta hash-only per la semantica della chiave `""`;
- test aggiunti/aggiornati:
  - `test_literal_eval_array_key_exists_string_key_indexed_scope_uses_eir_aot`,
    che copre `"1"`, `"x"` e `"01"` via direct-read EIR AOT senza helper eval e
    senza `elephc_magician`;
  - `test_array_key_exists_indexed_string_keys`, che copre lo stesso
    comportamento nel backend EIR condiviso fuori da `eval`;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_array_key_exists_string_key_indexed_scope -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests test_array_key_exists_indexed_string_keys -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests array_key_exists -- --nocapture`: 10/10;
  - `./scripts/test-linux-x86_64.sh array_key_exists`: passato, inclusi 10/10
    codegen filtrati, 1/1 error test filtrato, 2/2 smoke test filtrati e 1/1
    test `elephc-magician` filtrato; warning preesistenti su `libc::time_t`;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs src/codegen_ir/lower_inst/builtins/arrays/key_exists.rs tests/codegen/eval.rs tests/codegen/arrays/indexed/search_merge_union.rs`: passato con i warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

Aggiornamento Milestone 4/5/7.99 - 2026-07-06:

- il gate EIR AOT di literal eval ora riconosce `call_user_func()` e
  `call_user_func_array()` quando il callback e' una stringa literal risolta a
  un builtin gia' sicuro per AOT oppure a una user function tipizzata gia'
  supportata dal subset statico;
- `call_user_func_array()` con array literal viene convertito nello stesso
  formato di argomenti usato dal lowering callable esistente: chiavi stringa
  diventano named args, chiavi intere restano positional, e chiavi dinamiche
  restano fallback;
- i `call_user_func*()` statici verso builtin puri foldabili vengono foldati
  prima del controllo AOT, per esempio `call_user_func("strtoupper", "az")`
  diventa direttamente una literal e non dipende dal bridge;
- le user function statiche AOT richiedono ora parametri e return type
  dichiarati: le funzioni non tipizzate restano sul bridge, evitando il
  mismatch in cui il planner module-level le accettava ma il lowerer del
  frammento AOT generava un `EvalFunctionCall` senza eval context locale;
- fallback conservato:
  - callback variabile, per esempio `call_user_func($fn, "abcd")`, resta sul
    bridge;
  - callback static method string o array callable non sono stati aperti in
    questa tranche;
  - user function non tipizzate continuano a usare il dispatch eval esistente;
- test aggiunti:
  - `test_literal_eval_static_call_user_func_builtin_uses_aot_without_magician`,
    che copre builtin statici via `call_user_func()` e
    `call_user_func_array()` con direct-read EIR AOT, senza helper eval e senza
    `elephc_magician`;
  - `test_literal_eval_static_call_user_func_user_function_uses_aot_without_magician`,
    che copre callback a user function tipizzata con positional, named e
    default args via EIR AOT;
  - `test_literal_eval_dynamic_call_user_func_callback_keeps_bridge_fallback`,
    che mantiene il callback variabile sul bridge;
- verifiche:
  - `cargo test --test codegen_tests literal_eval_static_call_user_func -- --nocapture`: 2/2;
  - `cargo test --test codegen_tests literal_eval_dynamic_call_user_func_callback -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests test_eval_fragment_call_user_func -- --nocapture`: 8/8;
  - `cargo test --test codegen_tests literal_eval_static_user_function_uses_aot_without_execute_bridge -- --nocapture`: 1/1;
  - `cargo test --test codegen_tests literal_eval_static_scalar_user_functions -- --nocapture`: 1/1;
  - `cargo check --tests`: passato;
  - `rustfmt --check src/eval_aot.rs tests/codegen/eval.rs`: passato con i
    warning gia' noti sull'opzione stabile `ignore`;
  - `git diff --check`: passato.

## Architettura proposta

### 1. Frammenti AOT come funzioni native interne

Ogni `eval('...')` supportato deve generare una funzione interna univoca, ad
esempio:

```text
__elephc_eval_aot_<module_id>_<eval_id>
```

ABI logica:

```text
eval_aot_fn(eval_context, eval_scope, eval_global_scope) -> boxed Mixed
```

Le implementazioni possono materializzare questi argomenti tramite helper ABI
target-aware esistenti. La funzione AOT restituisce sempre un boxed Mixed:

- valore di `return expr;` se il frammento ritorna;
- boxed `null` se il frammento termina senza `return`;
- status/fatal gestito dagli helper esistenti se un helper runtime fallisce.

Il call site di `eval()` deve:

1. riconoscere che il frammento literal ha una funzione AOT;
2. preparare context/scope/global scope come il bridge, ma solo quanto serve;
3. flushare nello scope le variabili del caller lette o scritte dal frammento;
4. chiamare la funzione AOT;
5. ricaricare dal dynamic scope le variabili che il frammento puo' aver scritto;
6. saltare completamente `__elephc_eval_execute`.

### 2. Analisi del frammento

Introdurre una fase compile-time dedicata:

```text
literal fragment string
  -> tokenizzazione/parsing come PHP fragment
  -> name/magic-constant handling compatibile
  -> analisi AOT eligibility
  -> raccolta read/write/declare/call effects
  -> lowering AOT o fallback reason
```

Questa fase deve produrre un oggetto simile a:

```rust
EvalAotPlan {
    function_symbol,
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    creates_unknown_vars: bool,
    needs_eval_context: bool,
    needs_global_scope: bool,
    fallback_reason: Option<EvalAotFallbackReason>,
}
```

Motivo: il call site deve sapere quali locals sincronizzare senza trattare ogni
eval literal come barriera massima quando non serve.

### 3. Non duplicare tutto il codegen a mano

Il primo subset scalar AOT oggi e' in `eval.rs` con un lowering manuale. Per
completare davvero AOT, il piano deve spostarsi verso uno dei due approcci:

Approccio preferito:

- abbassare il frammento eval a una funzione EIR interna;
- riusare `src/ir_lower/`, `src/ir_passes/` e `src/codegen_ir/`;
- aggiungere primitive EIR esplicite per scope eval solo dove il normale
  modello di locals statici non basta.

Approccio temporaneo ammesso solo per sbloccare il benchmark dei primi:

- estendere il lowering AOT manuale a un mini-subset int/bool/control-flow;
- trattarlo come ponte di breve durata;
- mantenere il piano di convergenza verso EIR-function AOT.

Il completamento finale non deve lasciare un grande secondo compiler manuale
in `src/codegen_ir/lower_inst/builtins/eval.rs`.

## Milestone

### Milestone 0 - Baseline e guardrail

Obiettivo: rendere misurabile il problema e impedire regressioni di fallback.

Deliverable:

- aggiungere benchmark temporaneo o fixture non-CI per "somma primi fino a
  100000" con quattro varianti:
  - Elephc standard;
  - Elephc literal eval;
  - PHP standard;
  - PHP eval;
- aggiungere test assembly che conferma lo stato pre-AOT per il frammento con
  `while`/`if`/`break`: oggi deve contenere `__elephc_eval_execute`;
- aggiungere test fallback per un costrutto esplicitamente non supportato, cosi'
  il fallback resta verificato quando AOT cresce.

Comandi:

- `cargo test --test codegen_tests test_eval_prime_loop_literal_fallback_before_full_aot`
- benchmark manuale con heap sufficiente per il fallback, solo come baseline.

### Milestone 1 - Funzione AOT interna per frammenti self-contained

Obiettivo: generare una funzione nativa interna per literal eval che non legge
ne' scrive variabili del caller.

Subset:

- scalar locals interni al frammento;
- assignment semplice e compound `+=`;
- `echo`;
- `return`;
- `while`;
- `if`;
- `break`;
- `continue`;
- int/bool values;
- operatori `+`, `-`, `*`, `%`, `<=`, `<`, `>=`, `>`, `==`, `!=`, `&&`, `||`
  dove gia' supportati dal parser/typechecker o dove abbassabili senza
  divergere da PHP.

Esempio target:

```php
eval('$sum = 0; $i = 1; while ($i <= 10000) { $sum += $i; $i += 1; } echo $sum;');
```

Requisiti:

- assembly deve contenere `eval literal AOT compiled`;
- assembly non deve contenere `__elephc_eval_execute`;
- output deve combaciare con PHP;
- se il frammento termina senza `return`, `eval()` deve restituire `null`.

### Milestone 2 - Scope read/write efficiente

Obiettivo: supportare frammenti AOT che leggono e scrivono variabili del caller
senza interpretazione runtime.

Esempi:

```php
$a = 10;
echo eval('return $a + 20;');
```

```php
$a = 10;
eval('$a = $a + 20;');
echo $a;
```

Requisiti:

- analisi read/write del frammento;
- flush solo delle variabili lette/scritte;
- reload solo delle variabili scritte o potenzialmente create;
- `global` e alias globali restano fallback finche' non sono modellati;
- variable variables (`$$name`), `unset`, references e by-ref restano fallback
  fino a supporto esplicito.

### Milestone 3 - Controllo di flusso per benchmark dei primi

Obiettivo: rendere AOT il benchmark:

```php
eval('$sum = 0; $n = 2; while ($n <= 100000) { ... if (...) { break; } ... } echo $sum;');
```

Requisiti:

- loop annidati;
- `break` che esce dal loop interno corretto;
- `if` con branch;
- modulo e confronti interi;
- nessuna chiamata a `__elephc_eval_execute`;
- tempo runtime vicino al codice Elephc standard e sensibilmente sotto PHP.

Acceptance iniziale:

- output `454396537`;
- `Elephc via eval` non richiede heap da centinaia di MB;
- RSS indicativo vicino a Elephc standard, non al fallback magician;
- `Elephc via eval` non oltre 2x Elephc standard per questo benchmark come
  primo target ragionevole.

### Milestone 4 - Chiamate statiche a builtins

Obiettivo: permettere a literal eval AOT di chiamare builtins noti.

Esempi:

```php
eval('echo strlen("abc");');
eval('$x = intval("42"); echo $x + 1;');
eval('echo abs(-10);');
```

Regole:

- chiamate con nome statico e risoluzione builtin/funzione gia' nota possono
  essere AOT;
- chiamate dinamiche (`$f()`), `call_user_func`, method calls, static calls
  late-bound restano fallback inizialmente;
- namespace fallback deve seguire le regole gia' usate dal resolver/typechecker;
- named/spread args devono usare `src/types/call_args/`, non una logica nuova.

Test:

- builtin case-insensitive dentro eval literal AOT;
- namespaced call con fallback builtin quando applicabile;
- arg count/type error uguale al path statico o fallback controllato.

### Milestone 5 - Funzioni statiche gia' note

Obiettivo: supportare chiamate a funzioni definite staticamente prima del punto
eval.

Esempio:

```php
function inc($x) { return $x + 1; }
echo eval('return inc(41);');
```

Fallback iniziale:

- funzioni dichiarate dentro eval;
- classi dichiarate dentro eval;
- duplicate declarations;
- dynamic function names;
- closure/callable dinamici.

### Milestone 6 - Riduzione fallback magician

Obiettivo: evitare di linkare `libelephc-magician` quando tutti gli eval del
programma sono literal e pienamente AOT.

Requisiti:

- il program usage scan distingue:
  - eval dinamico;
  - literal eval fallback;
  - literal eval fully AOT;
- programmi con solo eval fully AOT non richiedono `elephc_magician`;
- test assembly/link metadata per assenza di `__elephc_eval_execute`,
  `__elephc_eval_context_new` e libreria `elephc_magician` quando non servono.

### Milestone 7 - Integrazione con pipeline EIR

Obiettivo: sostituire il mini-AOT manuale con un vero lowering del frammento a
funzione EIR interna.

Lavoro richiesto:

- rappresentare eval fragment come `Function` EIR interna con ABI speciale;
- dichiarare locals del frammento separati dai locals del caller;
- introdurre istruzioni o builtins EIR per:
  - `eval_scope_get`;
  - `eval_scope_set`;
  - `eval_return_null`;
  - eventuale `eval_status_check`;
- far passare la funzione AOT attraverso validator, optimizer, regalloc e
  backend target-aware;
- rimuovere o ridurre il lowering manuale in `eval.rs`.

Questo milestone e' quello che rende "completo" il percorso AOT in modo
manutenibile.

## Fallback policy

Un frammento literal deve restare fallback se contiene:

- codice non parseabile come fragment PHP;
- include/require;
- declaration di funzioni/classi/interfacce/trait/enum finche' non registrate
  staticamente nel contesto eval;
- `global`, `static`, references/by-ref, `unset`, variable variables;
- dynamic calls, dynamic class names, object/method/property access non
  supportati;
- eccezioni/throw/try finche' non modellati nel frammento AOT;
- costrutti non supportati dal normale EIR backend;
- qualunque comportamento per cui non esiste test PHP-parity.

Il fallback deve essere esplicito in assembly con un marker che includa una
ragione leggibile quando possibile.

## File probabili

- `src/ir/`
- `src/ir_lower/`
- `src/ir_passes/`
- `src/codegen_ir/lower_inst/builtins/eval.rs`
- `src/codegen_ir/`
- `src/codegen/program_usage/`
- `src/types/call_args/`
- `src/types/checker/`
- `src/name_resolver/`
- `src/resolver/`
- `tests/codegen/eval.rs`
- `tests/codegen/optimizer/`
- `benchmarks/magician/cases/` solo se si decide di promuovere i benchmark di
  accettazione nella suite permanente.

## Test richiesti

Assembly/codegen tests:

- literal eval con `while` self-contained usa AOT e non bridge;
- literal eval con `if`/`break` usa AOT e produce output corretto;
- literal eval nested loops prime-sum usa AOT e non bridge;
- literal eval senza `return` ritorna `null`;
- literal eval con `return` ritorna il valore senza uscire dal caller;
- literal eval legge `$a` dal caller;
- literal eval scrive `$a` nel caller;
- literal eval crea `$created` visibile dopo eval;
- unsupported dynamic eval non emette marker AOT;
- unsupported literal eval emette fallback e chiama bridge;
- programma con soli eval fully AOT non linka magician, quando Milestone 6 e'
  implementato.

Target-sensitive checks:

- focused macOS ARM64 codegen tests;
- focused Linux x86_64 Docker test per prime-loop AOT;
- focused Linux ARM64 Docker test per prime-loop AOT.

Benchmark checks:

- prime-sum `100000`:
  - output `454396537`;
  - no `__elephc_eval_execute`;
  - RSS non vicino al fallback da centinaia di MB;
  - runtime molto sotto PHP CLI e vicino a Elephc standard.

## Criteri di completamento

Il percorso AOT per eval puo' essere considerato completo solo quando:

1. ogni literal eval supportato viene compilato a funzione nativa interna;
2. il benchmark dei primi fino a `100000` passa via AOT senza bridge;
3. scope read/write del caller funziona per variabili note a compile time;
4. `return`/`null` eval semantics sono PHP-compatible;
5. builtins statici comuni funzionano via AOT;
6. i fallback non supportati restano corretti e verificati;
7. programmi senza fallback eval non linkano magician;
8. la soluzione non embedda il compilatore nel binario finale;
9. i test focused passano sui tre target supportati o hanno CI equivalente;
10. `git diff --check` passa.

## Rischi principali

- Duplicare troppo codegen in `eval.rs` e creare un secondo backend difficile da
  mantenere.
- Trattare eval come codice statico normale e perdere semantica di scope.
- Dimenticare che `eval('$x + 1;')` ritorna `null`, non l'ultima espressione.
- Saltare i fallback per costrutti dinamici e produrre miscompilazioni.
- Ottimizzare il benchmark dei primi con un percorso troppo speciale invece di
  completare il meccanismo generale.
- Supportare solo ARM64 e lasciare x86_64/Linux indietro.

## Ordine consigliato

1. Stabilizzare baseline e test fallback.
2. Implementare funzione AOT interna self-contained con locals int/bool e
   control-flow.
3. Far passare il benchmark dei primi via AOT.
4. Integrare scope read/write del caller nel nuovo modello a funzione AOT.
5. Aggiungere builtins statici.
6. Portare il lowering AOT verso funzione EIR interna riusando la pipeline.
7. Ridurre il link a magician quando tutti gli eval sono fully AOT.

## Aggiornamento: metodi statici pubblici

Tranche completata:

- il planner AOT per literal eval accetta predicate separati per funzioni e
  metodi statici;
- `ir_lower::program`, `ir_lower::expr` e il lowering codegen ricostruiscono lo
  stesso piano con metadati di classe coerenti;
- entrano in AOT solo chiamate a metodi statici nominali, `public`, con
  signature scalare completamente dichiarata e argomenti normalizzabili dal
  planner condiviso;
- metodi statici non tipizzati o non coperti restano sul bridge magician.

Verifiche locali:

- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_method_uses_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_untyped_static_method_keeps_bridge_fallback -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_eval_fragment_dispatches_aot_static_methods -- --exact --nocapture`
- `cargo check --tests`
- `rustfmt --check src/eval_aot.rs src/ir_lower/program.rs src/ir_lower/expr/mod.rs src/codegen_ir/lower_inst/builtins/eval.rs tests/codegen/eval.rs`
- `git diff --check -- src/eval_aot.rs src/ir_lower/program.rs src/ir_lower/expr/mod.rs src/codegen_ir/lower_inst/builtins/eval.rs tests/codegen/eval.rs .plans/elephc-eval-aot-complete-plan.md`

## Aggiornamento: callback a metodi statici

Tranche completata:

- il classificatore AOT per literal eval riconosce callback statiche
  compile-time in forma `"Class::method"` e `["Class", "method"]`;
- `call_user_func()` e `call_user_func_array()` possono restare nel percorso
  EIR AOT quando il metodo statico target e' pubblico, tipizzato e supportato
  dagli stessi predicate delle chiamate statiche dirette;
- callback statiche verso metodi non tipizzati o non supportati continuano a
  usare il fallback magician.

Verifiche locali:

- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_method_callbacks_use_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_untyped_static_method_callback_keeps_bridge_fallback -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_call_user_func_user_function_uses_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_method_uses_aot_without_magician -- --exact --nocapture`

## Aggiornamento: callback statici con Class::class

Tranche completata:

- il classificatore AOT accetta anche callable array statici in forma
  `[NamedClass::class, "method"]`;
- il supporto resta limitato a receiver nominali, lasciando fuori `self::class`,
  `static::class` e `parent::class` finche' il gate AOT non riceve il contesto
  di classe necessario;
- la stessa copertura AOT verifica string callback, callable array con stringa,
  callable array con `Class::class`, `call_user_func()` e
  `call_user_func_array()`.

Verifiche locali:

- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_method_callbacks_use_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_untyped_static_method_callback_keeps_bridge_fallback -- --exact --nocapture`

## Aggiornamento: callback first-class statici

Tranche completata:

- il gate AOT riconosce callback first-class compile-time in
  `call_user_func()` e `call_user_func_array()` per funzioni note e metodi
  statici nominali;
- la classificazione generale di un first-class callable come valore resta
  conservativa: il supporto vale solo quando il callable e' usato come callback
  statico immediato;
- la copertura verifica sia `call_user_func()` sia `call_user_func_array()` con
  first-class callable a funzione utente e metodo statico;
- i predicate esistenti continuano a escludere metodi non tipizzati, receiver
  non nominali e signature fuori subset.

Verifiche locali:

- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_call_user_func_user_function_uses_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_static_method_callbacks_use_aot_without_magician -- --exact --nocapture`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_untyped_static_method_callback_keeps_bridge_fallback -- --exact --nocapture`

## Chiusura piano - 2026-07-06

Audit dei criteri di completamento:

1. I literal eval nel subset supportato entrano in percorsi AOT nativi e i
   fallback restano marcati con motivo leggibile.
2. Il benchmark prime-sum fino a `100000` passa senza bridge, senza
   `elephc_magician`, con output `454396537`.
3. Scope read/write del caller e variabili create da eval sono coperti dai test
   direct-local/direct-param/scope-helper senza linkare magician quando il
   frammento e' fully AOT.
4. `return`, fallthrough/null e output side-effect sono coperti da test dedicati.
5. Builtin statici comuni, chiamate a funzioni note, metodi statici pubblici,
   `call_user_func*()` statici e first-class callback statici sono coperti dal
   percorso AOT.
6. I fallback non supportati restano verificati: eval dinamico, callback
   dinamici, signature non tipizzate, spread dinamici, casi array/object non
   modellati e semantiche PHP conservative.
7. I programmi con solo eval fully AOT non linkano `elephc_magician`; il test
   prime-loop verifica anche assenza di helper runtime `__elephc_eval_*`.
8. La soluzione non embedda il compilatore nel binario finale: l'AOT avviene nel
   compilatore e i binari fully AOT non linkano il bridge magician.
9. Il prime-loop AOT e' passato su macOS ARM64 locale, Linux x86_64 Docker e
   Linux ARM64 Docker con filtro dedicato.
10. `git diff --check` passa sull'intero worktree corrente; zero file risultano
    modificati solo da whitespace rispetto a `git diff -w`.

Ultime verifiche locali registrate:

- `cargo build --release`
- `cargo test --test codegen_tests codegen::eval::test_literal_eval_prime_loop_uses_aot_without_execute_bridge -- --exact --nocapture`
- `./scripts/test-linux-x86_64.sh test_literal_eval_prime_loop_uses_aot_without_execute_bridge`
- `./scripts/test-linux-arm64.sh test_literal_eval_prime_loop_uses_aot_without_execute_bridge`
- benchmark manuale prime-sum fuori repo su 12 run:
  - Elephc standard `13.35 ms`;
  - Elephc via eval literal AOT `9.74 ms`;
  - PHP standard `97.77 ms`;
  - PHP via eval `117.76 ms`;
  - RSS sample Elephc via eval `1572864` byte.
- `python3 scripts/benchmark_magician.py --case algebra_heavy --iterations 5 --warmup 1`
  - Elephc native `2.84 ms`;
  - Elephc eval `4.90 ms`;
  - PHP native `78.54 ms`;
  - PHP eval `80.17 ms`.
- `git diff --check`
- `comm -23 <(git diff --name-only | sort) <(git diff -w --name-only | sort) | wc -l`
