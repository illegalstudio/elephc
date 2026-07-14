# Spec finale : Boot-before-fork pour elephc — mode `--web-worker` (handler)

Statut : **PROPOSED**. Document de design détaillé, pas un plan engagé.

## 1. Objectif

Booter le top-level PHP une fois dans le master process avant le fork, pour partager l'état de boot (container Symfony, caches, statics) via le copy-on-write du kernel. Réduire l'empreinte mémoire de N×boot_state à ~1×boot_state + N×per_request_working_set.

## 2. Flux actuel (boot-after-fork)

```
main()
  → elephc_web_run_worker(argc, argv, &boot_fn)           [server.rs:509]
    → for N workers: spawn_worker(WorkerKind::Worker { boot })
      → fork()
        → child: boot()                                    [server.rs:296]
          → top-level PHP → elephc_worker_register(handler)
            → elephc_web_worker_register(handler) [-> !]   [handler.rs:147]
              → enter_worker_loop()                         [worker_mode.rs:260]
```

Chaque worker boote indépendamment → N copies du tas de boot.

## 3. Flux proposé (boot-before-fork)

```
main()
  → elephc_web_run_worker(argc, argv, &boot_fn)
    → boot_fn()                                             [DANS LE MASTER]
      → top-level PHP → elephc_worker_register(handler)
        → elephc_web_worker_register(handler) [-> () maintenant]
          → stocke handler, RETOURNE
      → top-level PHP termine normalement
    → __rt_deactivate_exit_boundary()
    → __rt_end_boot_phase()   (marque les blocs alloués comme immortels)
    → for N workers: spawn_worker_after_boot(listen, cfg)
      → fork()
        → child: signal_ready() → enter_worker_loop()
```

## 4. Changements détaillés

### 4.1 Bridge Rust — `crates/elephc-web/src/handler.rs`

```rust
// AVANT (-> !) :
pub extern "C" fn elephc_web_worker_register(handler: WorkerHandler) -> ! {
    unsafe { /* store handler + set booted */ }
    signal_boot();
    crate::worker_mode::enter_worker_loop();
}

// APRÈS (-> ()) :
pub extern "C" fn elephc_web_worker_register(handler: WorkerHandler) {
    unsafe { /* store handler + set booted */ }
    // Ne plus signal_boot() ni enter_worker_loop() ici.
    // Retourner au master.
}
```

Ajouter un getter :
```rust
pub(crate) fn is_worker_handler_registered() -> bool {
    unsafe { *core::ptr::addr_of!(WORKER_HANDLER) }.is_some()
}
```

### 4.2 Bridge Rust — `crates/elephc-web/src/server.rs`

```rust
// AVANT :
pub extern "C" fn elephc_web_run_worker(argc, argv, boot_fn: extern "C" fn()) -> i32 {
    let args = parse_args(argc, argv, true);
    install_signal_handlers();
    let kind = WorkerKind::Worker { boot: boot_fn };
    for _ in 0..args.workers { spawn_worker(&args.listen, kind.clone(), cfg); }
    supervise(...)
}

// APRÈS :
pub extern "C" fn elephc_web_run_worker(argc, argv, boot_fn: extern "C" fn() -> i32) -> i32 {
    let args = parse_args(argc, argv, true);
    install_signal_handlers();
    
    // Phase 1 : boot dans le master avant fork
    worker_mode::set_worker_config(cfg);
    worker_mode::set_worker_listen(args.listen.clone());
    let boot_rc = boot_fn();  // top-level PHP tourne une fois
    if boot_rc != 0 || !handler::is_worker_handler_registered() {
        eprintln!("elephc-web: boot failed (exit code {})", boot_rc);
        return 1;
    }
    
    // Phase 2 : désactiver l'exit boundary + marquer les blocs comme immortels
    // (appel aux runtime helpers, voir §4.4 et §4.5)
    
    // Phase 3 : fork N workers
    let kind = WorkerKind::WorkerAfterBoot;
    for _ in 0..args.workers { spawn_worker_after_boot(&args.listen, cfg); }
    supervise(...)
}
```

Nouveau `spawn_worker_after_boot` :
```rust
fn spawn_worker_after_boot(listen: &str, cfg: WorkerConfig) -> (pid_t, Option<i32>) {
    let (rd, wr) = pipe();  // boot-signal pipe (pour startup vs runtime crash)
    match fork() {
        0 => {
            reset_signal_handlers_to_default();
            // FD cleanup : fermer tous les FDs hérités sauf stdin/stdout/stderr + pipe wr
            close_inherited_fds(wr);
            worker_mode::set_worker_config(cfg);
            worker_mode::set_worker_listen(listen);
            handler::set_boot_pipe(wr);
            handler::signal_boot();  // signal "ready" au master
            crate::worker_mode::enter_worker_loop();  // -> !
        }
        pid => { close(wr); (pid, Some(rd)) }
    }
}
```

### 4.3 Codegen — `src/codegen_ir/lower_inst/builtins/system.rs:1029`

```rust
// AVANT :
pub(super) fn lower_elephc_worker_register(ctx, inst) -> Result<()> {
    store handler to global
    call elephc_web_worker_register
    brk #0 / ud2  // unreachable
}

// APRÈS :
pub(super) fn lower_elephc_worker_register(ctx, inst) -> Result<()> {
    store handler to global
    call elephc_web_worker_register
    // NE PLUS émettre brk/ud2 — le call retourne maintenant.
    // Le top-level PHP continue et termine normalement (return 0).
}
```

### 4.4 Codegen — exit boundary pour le boot master

**Problème** : `exit()`/`die()` pendant le boot master tuerait le master.

**Solution (Option C de Kimi)** : installer l'exit boundary pour le boot handler mode aussi, avec retour de code d'erreur.

Dans `frame.rs:379` :
```rust
// AVANT :
if !ctx.web_worker { emit_web_exit_boundary(ctx); }

// APRÈS :
if !ctx.web_worker || ctx.boot_before_fork { emit_web_exit_boundary(ctx); }
```

Le `boot_fn` doit retourner `i32` (0 = succès, non-zéro = exit/die). Dans `emit_web_exit_bailout_landing` (frame.rs:426), avant le `ret`, mettre le code d'erreur dans le registre de retour :
```asm
// AArch64 :
mov w0, #1    // boot exit code
// x86_64 :
mov eax, 1    // boot exit code
```

Avant le fork, Rust appelle `__rt_deactivate_exit_boundary()` qui met `_exit_boundary_active = 0`. Après fork, chaque worker a sa propre copie COW de `_exit_boundary_active = 0`. Le worker ne réinstalle pas de boundary (le handler mode n'en a pas — `exit()` dans une requête tue le worker, comme actuellement).

### 4.5 Runtime — bit immortel pour les blocs de boot

**Problème** : le GC d'un worker peut libérer des blocs du boot state partagé via COW.

**Solution (Solution B de Kimi)** : marquer tous les blocs alloués pendant le boot comme "immortels".

#### Header du bloc

`[size:4][refcount:4][kind:8]` — le byte de kind (8 bits) utilise :
- Bits 0-6 : kind tag (1=string, 2=array, 3=hash, 4=object, 5=mixed, 6=throwable)
- Bit 7 (0x80) : persistent COW flag (existant)
- **Nouveau** : bit 6 (0x40) : immortel/boot-persistent flag

#### `__rt_in_boot_phase`

Nouveau `.comm __rt_in_boot_phase, 8, 3` (fixe dans `fixed.rs`). Mis à 1 par Rust avant `boot_fn()`, mis à 0 par `__rt_end_boot_phase()` après le boot.

#### `__rt_heap_alloc` (`heap_alloc.rs`)

Au moment de l'initialisation du refcount (line ~164), si `__rt_in_boot_phase == 1`, OR le kind byte avec `0x40` :
```asm
// AArch64 :
adrp x9, __rt_in_boot_phase@PAGE
ldr x10, [x9, __rt_in_boot_phase@PAGEOFF]
cbz x10, skip_immortal
ldr x11, [x10, #8]    // load kind word
orr x11, x11, #0x40   // set immortal bit
str x11, [x10, #8]
skip_immortal:
```

#### `__rt_gc_collect_cycles` (`gc_collect_cycles.rs`)

Dans chacune des 4 passes (Clear, Count, Mark, Free), skip les blocs dont le kind byte a le bit `0x40` :
```asm
// Au début de chaque passe, pour chaque bloc :
ldr x9, [block, #8]     // load kind word
and x9, x9, #0x40       // isolate immortal bit
cbnz x9, skip_block    // skip if immortal
```

#### `__rt_decref_any` / `decref_*`

Dans le chemin de decref, avant de libérer un bloc (quand `refcount == 0`), check le bit immortel :
```asm
ldr x9, [block, #8]
and x9, x9, #0x40
cbnz x9, skip_free     // never free immortal blocks
```

#### `__rt_end_boot_phase` (nouveau runtime helper)

```asm
__rt_end_boot_phase:
    // Set __rt_in_boot_phase = 0
    adrp x9, __rt_in_boot_phase@PAGE
    str xzr, [x9, __rt_in_boot_phase@PAGEOFF]
    ret
```

Appelé par Rust après `boot_fn()` et avant le fork.

### 4.6 FD cleanup post-fork

Dans `spawn_worker_after_boot`, l'enfant ferme tous les FDs hérités sauf stdin/stdout/stderr et le pipe wr. Sur Linux : `close_range(3, UINT_MAX, CLOSE_RANGE_UNSHARE)`. Sur macOS : boucle manuelle sur `/dev/fd/` ou `getdtablesize()`.

### 4.7 Re-seed PRNG post-fork

Si le boot PHP initialise un PRNG, chaque worker hérite du même seed. Re-seeder dans l'enfant après fork (via `getrandom()` ou équivalent).

## 5. Ce qui ne change pas

- **Mode `--web`** (classic) : inchangé. Pas de boot-before-fork (le top-level re-execute per request).
- **Mode `--web-worker=script`** : inchangé. Le top-level re-execute per request, pas de boot partagé.
- **Codegen des fonctions PHP** (hors `register` et exit boundary) : inchangé.
- **Le trampoline `elephc_worker_handle_request`** : inchangé.
- **Le prelude PHP** : inchangé (le top-level après `register` est déjà unreachable en pratique).
- **`worker_mode.rs::enter_worker_loop`** : inchangé (appelé depuis le worker, pas depuis `register`).

## 6. Tests requis

1. `boot_before_fork_handler_works` — compile `<?php elephc_worker_register(function(){echo "ok";});`, boot-before-fork, 1 worker, curl → "ok".
2. `boot_before_fork_state_shared_cow` — un static `$c = build();` construit au boot, lu par N workers, doit être identique.
3. `boot_before_fork_exit_in_boot_returns_error` — `<?php exit(1);` dans le boot → master retourne code 1, pas de fork.
4. `boot_before_fork_gc_does_not_free_boot_state` — static array construit au boot, 100 requêtes avec GC, le static est toujours lisible.
5. `boot_before_fork_respawn_inherits_boot_state` — `--max-requests 2`, 4 requêtes → le worker respawné hérite du boot state (pas de re-boot).
6. Non-régression : tous les tests `web_worker` existants doivent passer en boot-before-fork.
7. Non-régression : les tests `--web` et `--web-worker=script` ne sont pas affectés.

## 7. Risques

| Risque | Mitigation |
|---|---|
| `exit()` dans le boot → master tué | Exit boundary installé dans le master, retourne code d'erreur |
| FDs hérités par les workers | `close_range` ou boucle manuelle post-fork |
| GC libère des blocs de boot | Bit immortel (0x40) dans le kind, GC skip les blocs immortels |
| COW pages dupliquées par refcount writes | Inévitable en v1 ; mesurer le PSS pour quantifier |
| `fork()` dans un process multi-thread | Le master est single-threadé (pas de tokio, pas de thread pool) |
| PRNG identique entre workers | Re-seed post-fork |
| `_exit_jmp_buf` stale dans le worker après fork | `_exit_boundary_active = 0` avant fork ; le worker n'utilise pas le boundary en handler mode |

## 8. Estimation d'effort

| Tâche | Effort |
|---|---|
| Bridge Rust (handler.rs, server.rs, spawn_worker_after_boot) | 2-3 jours |
| Codegen system.rs (retirer brk/ud2) | 0.5 jour |
| Codegen frame.rs (exit boundary boot master + retour i32) | 1-2 jours |
| Runtime bit immortel (heap_alloc, gc_collect_cycles, decref_any, __rt_end_boot_phase) | 2-3 jours |
| FD cleanup + PRNG re-seed | 1 jour |
| Tests (7 tests + non-régression) | 2-3 jours |
| Validation 3 cibles (macOS, Linux x86_64, Linux ARM64) | 1-2 jours |
| **Total** | **~2 semaines** |

## 9. Décisions de design

| Point | Décision | Justification |
|---|---|---|
| Exit boundary en boot master | Option C (installer puis désactiver avant fork) | Protège le master sans laisser jmp_buf stale |
| GC roots | Solution B (bit immortel 0x40) | Minimal, compatible COW, GC request conservé |
| `boot_fn` signature | `extern "C" fn() -> i32` (0=succès, 1=exit) | Permet à Rust de détecter exit/die |
| `register` signature | `-> ()` (retourne) | Le master reprend le contrôle |
| FD cleanup | `close_range` Linux + boucle macOS | Ferme tout sauf stdio + pipe |
| Bit immortel | 0x40 dans le kind byte | Disponible (0x80 = COW, 0x01-0x06 = kind tag) |
| Promotion immortel | À la fin du boot (`__rt_end_boot_phase`), pas à l'alloc | Évite de marquer les temporaires du boot |

## 10. Risques bloquants identifiés par review padawans (Kimi K2.7 + Minimax M3)

### 10.1 Pollution des structures immortelles par état per-request — **BLOQUANT**

Si un bloc immortel (ex: `static $cache`) accumule des références vers des blocs per-request (objets créés pendant une requête), ces blocs per-request ne sont jamais libérés (le GC les compte comme roots car référencés par un immortel). **Fuite mémoire garantie en production.**

**Solutions** :
- **A** : Reset agressif des références per-request dans les structures immortelles à chaque fin de requête (proche du modèle PHP-FPM `ResetInterface`). Complex : nécessite de tracker les slots mutables.
- **B** : Les structures immortelles ne référencent jamais directement des blocs per-request (indirection via handles/weakrefs). Discipline de codage stricte, incompatible avec du PHP legacy.
- **C** : Sweeping séparé — garder la passe Count correcte (comptage normal), ne libérer que les blocs avec `refcount == 0` ET non référencés par un immortel.

**Recommandation** : Solution A pour v1 (reset per-request), avec un mécanisme de "purge sélective" des slots d'array statique qui contiennent des refs per-request. À spécifier dans une v0.2.

### 10.2 Mécanisme d'interception de `exit()` — **À figer**

La spec décrit l'exit boundary mais ne précise pas comment `exit()` est intercepté. Trois options :
- Override de `exit()` au link (fragile en AOT statique)
- `setjmp`/`longjmp` (leak de mémoire allouée entre setjmp et longjmp, pas d'unwinding C++)
- Instrumentation compilateur (insertion de checks aux prologues)

**Recommandation** : `setjmp`/`longjmp` (déjà implémenté pour `--web`/`--web-worker=script`). Réutiliser le mécanisme existant (`_exit_jmp_buf`), pas de nouveau mécanisme.

### 10.3 Gel du master après boot — **Invariant critique**

Après `__rt_end_boot_phase()`, le master ne doit plus toucher aucune page du heap PHP : pas de `decref`, pas d'allocation, pas d'écriture. Sinon, les nouveaux workers (respawn) héritent de pages modifiées.

**Mitigation** : `mprotect(PROT_READ)` sur le heap PHP du master après le boot, comme garde-fou. Levée juste avant le fork (ou fork via `mmap` + `MAP_PRIVATE` qui préserve le COW).

### 10.4 OPcache / locks / threads au fork

Si le runtime utilise des threads (pool, async I/O, JIT) ou des locks (OPcache, libxml), `fork()` ne duplique que le thread appelant → deadlocks potentiels.

**Mitigation** : Le master est garanti single-threadé (pas de tokio, pas de thread pool — déjà vérifié dans le code). Pas d'OPcache (compilation AOT, pas d'OPcache runtime). Audit des locks libc/extension à faire.

### 10.5 Race au démarrage / timeout

`signal_boot()` doit avoir un timeout côté master. Un worker qui freeze dans son init post-fork doit être détecté.

## 11. Estimation d'effort révisée (3-4 semaines)

| Tâche | Effort révisé |
|---|---|
| Codegen (exit boundary + retour i32 + retrait brk/ud2) | 3-4 jours |
| Runtime bit immortel (promotion à la fin du boot, pas à l'alloc) | 2-3 jours |
| GC skip immortels + gestion per-request refs (point 10.1) | 4-5 jours |
| Decref skip + audit refcount writes | 1-2 jours |
| Bridge (spawn_worker_after_boot + FD cleanup cross-platform) | 2-3 jours |
| Master freeze (mprotect + invariant) | 1-2 jours |
| PRNG re-seed + signal timeout | 1 jour |
| Tests end-to-end + stress (memory leak hunt) | 4-5 jours |
| Tests non-régression (script mode, edge cases) | 2-3 jours |
| Validation 3 cibles | 2 jours |
| **Total** | **~3-4 semaines** |

## 12. Statut

**PROPOSED — non implémenté.** La spec est techniquement valide sur l'architecture générale mais deux points bloquants (10.1 pollution per-request, 10.2 mécanisme exit) doivent être tranchés avant l'implémentation. La review padawans recommande de figer ces points dans une v0.2 avant d'attaquer le code.