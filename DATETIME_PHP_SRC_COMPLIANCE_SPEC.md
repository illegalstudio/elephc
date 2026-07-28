---
title: "DateTime PHP-src Compliance Spec v2.2 (audit post-rebase)"
description: "Gap analysis confrontée à php-src/PHP 8.5.6 et relue par Kimi K2.7, Minimax M3 et GLM 5.2."
---

# DateTime PHP-src Compliance Spec (v2.2 — audit post-rebase)

- **Worktree**: `/Users/guillaumeloulier/PhpstormProjects/oss/elephc/.claude/worktrees/datetime-php-src-compliance`
- **Branch**: `feat/datetime-php-src-compliance`
- **Reference**: PHP-src `master` (PHP 8.5) `ext/date/php_date.stub.php` + `php_date.h` + `timelib`, **vérifié contre `php -r`** (PHP 8.5 local).
- **Scope**: `ext/date`. `ext/calendar` OUT of scope (déjà bit-exact).
- **Claim**: conformité élevée sur la surface implémentée, avec les écarts résiduels explicitement
  documentés en §5 et §9. Les tests Elephc qui passent ne sont pas, à eux seuls, une preuve de
  parité: le mode `ELEPHC_PHP_CHECK=1` signale certaines différences sous forme de notes non fatales.

## État autoritatif de l'audit du 28 juillet 2026

Cet audit a été refait après rebase sur `origin/main`, contre le stub officiel PHP 8.5 et un
interpréteur PHP 8.5.6 local. Les trois relectures Ollama (Kimi K2.7, GLM-5.2, MiniMax M3)
convergent sur les écarts vérifiés indépendamment.

Corrigé dans cette révision:

- `DatePeriod::$recurrences` et sa sérialisation suivent le minimum d'instances retournées, tandis
  que `getRecurrences()` conserve le compte explicite;
- le constructeur string de `DatePeriod` accepte une expression runtime;
- l'état `getLastErrors()` est partagé entre `DateTime` et `DateTimeImmutable`, avec les positions
  principales `Not enough data`/`Unexpected data`;
- `DateTimeZone::getTransitions()` utilise la vraie borne par défaut `2147483647`;
- les offsets `DateTimeZone` suivent les formes timelib, y compris le préfixe `GMT` et les
  secondes, puis sont normalisés (`+0200` → `+02:00`, `GMT+023045` → `+02:30:45`);
- `idate()` valide aussi les formats dynamiques;
- `DateTimeInterface` déclare `diff()` et les hooks de sérialisation (les tableaux associatifs
  restent typés `mixed` en interne, voir les écarts résiduels);
- `from_string`/`date_string` restent dans l'état sérialisé de `DateInterval`, pas comme propriétés
  publiques ordinaires; les formes relatives et ISO suivent les deux shapes exclusives de PHP;
- `timezone_name_from_abbr()` suit la recherche ordonnée de timelib sur la table complète, puis
  sa table de repli `(offset, isDST)`;
- `createFromFormat()` traite `O`/`P`/`T`/`e` comme des zones embarquées qui remplacent le troisième
  argument; `Z` reste un littéral;
- `setTimestamp()` remet les microsecondes à zéro, tandis que les conversions mutable/immutable et
  `setISODate()` les conservent;
- les fuseaux numériques et abréviations sont adaptés au runtime système sans perdre leur nom PHP,
  et les chaînes empruntées sont copiées avant transfert d'ownership.

Écarts résiduels documentés, non masqués par la claim: table exhaustive des messages/comptages de
`createFromFormat`, forme d'itérateur indépendante de `DatePeriod`, readonly virtuel de ses
propriétés, notices runtime, comportement warning+`null` des pseudo-propriétés `DateInterval`, et
grammaire libre timelib non exhaustive dans `date_parse()`/`strtotime()`.

## 0. Corrections de la v1 (consensus padawans, vérifiées par `php -r`)

| ID v1 | Verdict `php -r` | Correction |
|---|---|---|
| **G1** (`timestampEnd=2147483647`) | **VRAI** — le stub PHP 8.5 fixe explicitement la borne par défaut à `2147483647`, pas à `PHP_INT_MAX`. | Corrigé et verrouillé par test. |
| **G19** (`date_create` throw propagate) | **FAUX** — `date_create('totoro')` retourne `bool(false)` (le wrapper attrape `DateMalformedStringException`). | **G19 corrigé**: `date_create`/`date_create_immutable` doivent catcher l'exception du ctor et retourner `false`. P0. |
| **G24** (`createFromTimestamp` drop fractional) | **FAUX** — `createFromTimestamp(1700000000.123456)->format('u')` = `123456`. Microsecondes conservées. | **G24 corrigé**: conserver la partie fractionnaire comme microsecondes. P0 (bug si actuellement tronqué). |
| **G15** (idate false sur format invalide) | **VRAI** mais nuance: format vide → warning + `false`; format non reconnu (`"q"`) → warning + `false`. elephc n'émet pas de warnings; conserver le retour `false`. | **G15 confirmé** P0. |
| **G2** (`from_string`/`date_string`) | Ces clés existent dans la forme debug/sérialisée, mais ne sont pas des propriétés déclarées lisibles. | Stockage interne + clés de `__serialize`; pas de fausse surface publique. |
| **R2** (format expanded year) | **FAUX (bridge déjà conforme)** — `format_utc_iso` produit `-292277022657-01-27T08:29:52+00:00` (format `X`-expanded). | **R2 retiré** — bridge déjà conforme. Test de non-régression. |
| **G12** (stubs serialize) | Les signatures font partie de la surface officielle. | Méthodes réelles et retours `void`; les tableaux associatifs restent typés `mixed` à cause de la distinction interne `AssocArray`/`Array`. |

## 1. Cartographie actuelle (résumé inchangé)

Builtins core + alias procéduraux + classes synthétiques + 10 exceptions + constantes — tous implémentés (voir v1 §1).

## 2. Gap analysis v2 (révisée)

### 2.1 Gaps fonctionnels (comportement)

| ID | Surface | PHP-src attendu (`php -r` vérifié) | elephc actuel | Remède |
|---|---|---|---|---|
| **G2** | État debug/sérialisé `DateInterval` | Clés `from_string`/`date_string` dans la forme exposée par debug/sérialisation; lecture directe = warning de propriété indéfinie + `null`. | Anciennement exposées comme propriétés publiques | Conserver des champs internes et émettre les clés via `__serialize`; documenter l'absence de notices runtime. |
| **G4** | `DatePeriod` propriétés readonly publiques | `start`, `current`, `end`, `interval`, `recurrences`, `include_start_date`, `include_end_date` (readonly, virtual). | Uniquement props internes | Voir §3.4. **Décision tranchée**: **propriétés miroir** mises à jour dans ctor/`_advance`/`rewind`/`next`/`current`. Pas de `__get` (casserait Reflection et `var_dump`). Les mirror props sont mutées par le runtime natif (le `readonly` userland ne s'applique pas au code synthétique). |
| **G5** | `DatePeriod::__construct` 3e overload (string) | `new DatePeriod(string $isostr, int $options = 0)` — deprecated 8.3 mais enregistré, forward vers `createFromISO8601String`. | Non enregistré | Enregistrer le 3e overload. **Aucune notice** émise (elephc n'a pas de runtime deprecation). Test compare uniquement le résultat. |
| **G6** | `DatePeriod::getEndDate()` return | `?DateTimeInterface` | `?DateTime` | Aligner return type. |
| **G7** | `DatePeriod::getStartDate()` return | `DateTimeInterface` | `DateTime` | Aligner return type. |
| **G8** | `DatePeriod implements IteratorAggregate` | `implements IteratorAggregate` (depuis PHP 5.3, pas 8.0). | `Iterator` only | Ajouter `IteratorAggregate` à la liste `implements`. `getIterator(): Iterator` retourne le period rewound. `instanceof IteratorAggregate` → true. |
| **G9** | `DateTimeZone::__construct` validation | Lève `DateInvalidTimeZoneException` ("Unknown or bad timezone (garbage)"). | Pas de validation | Valider dans le ctor synthétique: accepter identifiants de `listIdentifiers(ALL_WITH_BC)`, offsets `+HHMM`/`-HHMM`, abbrs connus. Sinon throw `DateInvalidTimeZoneException`. |
| **G10** | `DateTime::__construct` validation | Lève `DateMalformedStringException` ("Failed to parse time string (X) at position N (c): The timezone could not be found in the database"). | Stocke sentinelle silencieusement | Garde dans le ctor: si `__elephc_strtotime_raw` retourne sentinelle, throw `DateMalformedStringException`. **Matrice de validation** en §3.7. |
| **G10b** | `DateTimeImmutable::__construct` validation | Idem G10 (`DateMalformedStringException`). | Idem | Étendre G10 aux deux classes (ctor partagé). |
| **G10c** | `DateTime::modify()` / `DateTimeImmutable::modify()` validation | Lève `DateMalformedStringException` sur string invalide (PHP 8.3+). | `strtotime` sentinelle silencieuse | Garde dans `modify()`: si `__elephc_strtotime_raw` sentinelle, throw `DateMalformedStringException`. |
| **G11** | `DateTime::createFromFormat` + `DateTimeImmutable::createFromFormat` detailed errors | `getLastErrors()` retourne `['warning_count'=>int, 'warnings'=>[int_pos=>'msg'], 'error_count'=>int, 'errors'=>[int_pos=>'msg']]`. **Clés int** (offset byte). Messages exacts PHP: "Trailing data", "The parsed date was invalid", "Data missing", "The format separator does not match", "Unexpected character", etc. **Retourne `false` si aucune erreur/warning** (sinon array). | État global partagé, `false` si clean, diagnostics principaux positionnés; comptages multi-erreurs non exhaustifs. | Stockage scalaire sûr pour l'ownership, reconstruction de la shape publique à la demande; limitation exhaustive documentée en §9. |
| **G13** | `diff()` `DateInterval::$days` | `int` après `diff()`, `false` pour interval direct. `format("%a")` → `(unknown)` si `days===false`, sinon le total. | Conforme storage | Test de régression. |
| **G15** | `idate()` return | `int` sur format valide, `false` sur format vide ou inconnu. Les tokens reconnus sont `B d G g H h I i L m N n s t U W w Y y z Z` (`O`/`P` ne le sont pas). | Ancienne validation limitée aux littéraux | Helper runtime commun aux littéraux et expressions dynamiques; pas de warning émis. |
| **G17** | `timezone_name_from_abbr` `$utcOffset`/`$isDST` | Recherche ordonnée de la table timelib; premier offset exact, puis fallback offset/DST si l'abréviation est inconnue. | Conforme via la table complète du bridge `elephc-tz` et le fallback php-src. | Verrouillé sur abréviations communes/rares, offsets ambigus et fallback sans abréviation. |
| **G19** | `date_create` / `date_create_immutable` return | `DateTime|false` — **retourne `false` sur string invalide** (catch l'exception du ctor). | Réécrit en `new DateTime()` nu | **Corriger**: la réécriture name_resolver doit wrapper dans un try/catch et retourner `false` sur `DateMalformedStringException`. Test: `date_create('totoro')` → `false`. |
| **G19b** | `date_modify` (alias procédural) return | `DateTime|false` — idem: catch l'exception de `modify()`. | Réécrit en `$d->modify()` nu | Wrapper dans try/catch `DateMalformedStringException` → `false`. Test: `date_modify($d, 'totoro')` → `false`. |
| **G19c** | `date_create_from_format` / `date_create_immutable_from_format` return | `DateTime|false` / `DateTimeImmutable|false` — `false` si `createFromFormat` retourne `false` (déjà le cas via `getLastErrors`). | Conforme (createFromFormat retourne déjà false) | Test: `date_create_from_format('Y-m-d', 'garbage') === false`. |
| **G24** | `DateTime::createFromTimestamp(int|float)` | Conserve la partie fractionnaire comme **microsecondes** (vérifié: `1700000000.123456` → `u=123456`). | Doc dit "drop fractional" | **Corriger**: si float, extraire la partie fractionnaire et setter `microsecond = (int)(frac*1000000)`. Tester. |
| **G25** | `DateTimeZone::listIdentifiers(PER_COUNTRY)` sans `$countryCode` | Lève `ValueError` ("Argument #2 ($countryCode) must be a two-letter ISO 3166-1..."). | Doc dit throws; vérifier impl | Vérifier `__elephc_list_identifiers` lève `ValueError` si `PER_COUNTRY` set et code absent. Test de régression. |

### 2.2 Gaps de surface (signatures/types)

| ID | Surface | Remède |
|---|---|---|
| **S1** | `DateInterval::$f` (float, microseconds) | Vérifier que la propriété `f` est exposée (PHP 7.1+). elephc l'a (`datetime.rs`) — audit + test. |
| **S2** | `DateTimeInterface::createFromInterface` | Méthode statique présente. Test `instanceof`. |
| **S3** | `DatePeriod::createFromISO8601String` | Méthode présente (`date_period.rs:599`). Test. |
| **S4** | `DateTimeInterface::ISO8601_EXPANDED` | Constante présente (`datetime.rs:81-98`). Test `format(X-m-d\TH:i:sP)`. |
| **S5** | `DateTime::__debugInfo()` (PHP 8.4+) | **Audit**: elephc l'expose-t-il ? Si non, ajouter (var_dump shape: `date`, `timezone_type`, `timezone`). **Décision**: elephc n'a pas de `var_dump` natif DateTime format — marquer **limitation** si absent. Vérifier. |
| **S6** | Format specifiers coverage mapping | Les 260 tests existants (`tests/codegen/system.rs`) couvrent déjà chaque specifier `date()`. **Pas de matrice paramétrique additionnelle** à coder: les tests existants sont la référence. Mapping specifier → tests existants à documenter dans le test file header. | Audit + header doc. |

### 2.3 Gaps de runtime (bit-exactness)

| ID | Surface | Verdict `php -r` | Remède |
|---|---|---|---|
| **R1** | `strtotime` 2-digit ISO `YY-MM-DD` | PHP remap (70→1970, 0→2000). elephc rejette. | Aligner: accepter + shorthand. Modifier `__rt_strtotime_iso_entry`. |
| **R2** | `getTransitions()` row 0 `time` format | **Bridge déjà conforme** (expanded year). | Test de non-régression. |
| **R3** | `getTransitions()` row 0 `ts` | PHP 64-bit: `-9223372036854775808` (= `PHP_INT_MIN` = `i64::MIN`). elephc: `i64::MIN`. | **Conforme**. Documenter. |
| **R4** | `getTransitions()` default `$timestampEnd` | PHP 8.5: `2147483647`. | Corrigé; test non-régression (185 rows Paris). |

## 3. Plan de remédiation v2

### 3.1 P0 — Comportement observable (bugs/corrections)

- **G19**: `date_create`/`date_create_immutable` catchent `DateMalformedStringException` → `false`.
- **G10/G10b**: `DateTime`/`DateTimeImmutable::__construct` lèvent `DateMalformedStringException` sur string invalide.
- **G10c**: `DateTime::modify`/`DateTimeImmutable::modify` lèvent sur string invalide.
- **G9**: `DateTimeZone::__construct` lève `DateInvalidTimeZoneException` sur id invalide.
- **G15**: `idate()` retourne `false` sur format vide/non reconnu.
- **G24**: `createFromTimestamp(float)` conserve les microsecondes.
- **R1**: `strtotime` accepte `YY-MM-DD` avec shorthand.
- **G25**: `listIdentifiers(PER_COUNTRY)` sans code → `ValueError`.

### 3.2 P1 — Surface userland manquante

- **G2**: `DateInterval::$from_string` + `$date_string`.
- **G4**: `DatePeriod` 7 propriétés readonly miroir (décision tranchée: mirror props).
- **G5**: 3e overload `DatePeriod::__construct(string)`.
- **G6/G7**: return types `getEndDate`/`getStartDate` → `?DateTimeInterface`/`DateTimeInterface`.
- **G8**: `DatePeriod implements IteratorAggregate`.
- **G11** (**repriorisé P1**): `getLastErrors()` structure complète + clés int + messages PHP. État **global** (pas par instance) — `DateTime::getLastErrors()` et `DateTimeImmutable::getLastErrors()` partagent le même état. Alias procédural `date_get_last_errors()` aussi testé.
- **G17**: `timezone_name_from_abbr` offset/DST disambiguation.
- **S1-S4**: audit + tests surface (createFromInterface, createFromISO8601String, ISO8601_EXPANDED, $f).

### 3.3 P2 — Documentation + tests de non-régression

- **G13**: `diff()` `days` test.
- **R2/R3/R4**: tests de non-régression bridge.
- **S5** (`__debugInfo`): audit; si absent, marquer limitation.

### 3.4 Détail G4 — DatePeriod propriétés readonly (décision tranchée)

**Approche: propriétés miroir**, pas `__get`. Raisons (consensus padawans):
- `__get` casserait `ReflectionProperty` et `var_dump`.
- `readonly` userland ne s'applique pas au code synthétique (les bodies PHP synthétiques elephc peuvent muter leurs propres props).

7 propriétés publiques, type, source:

| Propriété | Type | Mise à jour |
|---|---|---|
| `$start` | `?DateTimeInterface` | ctor (depuis `startTs` + `startIsImmutable`) |
| `$current` | `?DateTimeInterface` | `current()` (depuis `curTs`) — synchro à chaque appel de `current` |
| `$end` | `?DateTimeInterface` | ctor (depuis `endTs` si pas `useCount`) |
| `$interval` | `?DateInterval` | ctor (reconstruit depuis les 7 parts) |
| `$recurrences` | `int` | ctor + `getRecurrences` (count form) ou computed (end form) |
| `$include_start_date` | `bool` | ctor = `!excludeStart` |
| `$include_end_date` | `bool` | ctor = `includeEnd` |

Les mirror props sont des `ClassProperty` publiques ajoutées à `date_period_properties()`. Les méthodes `current()`/`_advance`/`rewind`/`next` les resynchronisent avant de retourner.

### 3.5 Détail G19 — wrappers procéduraux `date_create*`

La réécriture name_resolver actuelle (`expressions.rs:496`) produit `new DateTime($s)`. **Corriger**: wrapper dans try/catch:
```php
try { return new DateTime($s); }
catch (\DateMalformedStringException $e) { return false; }
```
Idem pour `date_create_immutable`. Test: `date_create('totoro') === false`.

### 3.6 Détail G11 — getLastErrors structure (messages PHP exacts)

Cible PHP (vérifié `php -r`):
```php
[
  'warning_count' => int,
  'warnings' => [int_pos => 'message', ...],   // clés int
  'error_count' => int,
  'errors' => [int_pos => 'message', ...],
]
// OU false si warning_count==0 && error_count==0
```

Messages PHP-src observés:
- `"Trailing data"` — si `$dp < $dlen` (sans `+` final).
- `"The parsed date was invalid"` — si date impossible (ex. mois 13, jour 99).
- `"Data missing"` — si format attendu mais sujet épuisé.
- `"The format separator does not match"` — séparateur mismatch.
- `"Unexpected character"` — char littéral mismatch.

Implémentation auditée: l'état est partagé sur `DateTime` et conservé sous forme de scalaires
(`count`, position et message principal), afin de ne pas retenir de tableaux refcountés dans des
propriétés statiques synthétiques. `getLastErrors()` reconstruit les tableaux PHP à la demande et
retourne `false` quand les deux compteurs sont nuls. Cette représentation couvre les diagnostics
principaux verrouillés, pas les comptages multi-erreurs exhaustifs de timelib.

### 3.7 Matrice de validation G9/G10 (cas de test)

| Entrée | Type | Comportement attendu |
|---|---|---|
| `new DateTime("now")` | valide | OK |
| `new DateTime("2024-01-01")` | valide | OK |
| `new DateTime("@1700000000")` | valide (epoch) | OK |
| `new DateTime("totoro")` | invalide | `DateMalformedStringException` |
| `new DateTime("")` | invalide | `DateMalformedStringException` |
| `new DateTimeZone("UTC")` | valide | OK |
| `new DateTimeZone("Europe/Paris")` | valide | OK |
| `new DateTimeZone("garbage")` | invalide | `DateInvalidTimeZoneException` |
| `new DateTimeZone("+0200")` | valide (offset) | OK |
| `new DateTimeZone("CET")` | valide (abbr) | OK |
| `date_create("totoro")` | invalide | `false` (catch) |
| `$d=new DateTime(); $d->modify("totoro")` | invalide | `DateMalformedStringException` |

### 3.8 Détail G15 — `idate()` specifiers reconnus

Liste exhaustive (PHP `ext/date/php_date.c`): `B d h H i I L m n N O P s t U w W y Y z Z`. Tout autre char (y compris `""`) → `false`.

## 4. Acceptance criteria (TDD)

Tests écrits **d'abord**. Les cas déclarés conformes sont confrontés à PHP 8.5. Le mode
`ELEPHC_PHP_CHECK=1` exécute PHP et signale les écarts sous forme de notes informatives; une suite
Elephc verte ne transforme pas ces notes en preuve de parité.

### 4.1 Tests P0

```rust
test_date_create_invalid_returns_false            // G19
test_date_create_immutable_invalid_returns_false  // G19
test_date_modify_invalid_returns_false             // G19b
test_date_create_from_format_invalid_returns_false // G19c
test_datetime_invalid_string_throws               // G10
test_datetime_immutable_invalid_string_throws     // G10b
test_datetime_modify_invalid_throws               // G10c
test_datetimezone_invalid_throws                  // G9
test_datetimezone_offset_valid                    // G9 (offset accepté)
test_datetimezone_abbr_valid                      // G9 (abbr accepté)
test_idate_empty_format_returns_false             // G15
test_idate_unknown_format_returns_false           // G15
test_idate_valid_format_returns_int               // G15
test_create_from_timestamp_float_keeps_micros     // G24
test_strtotime_two_digit_iso_year                 // R1
test_list_identifiers_per_country_no_code_throws  // G25
```

### 4.2 Tests P1

```rust
test_dateinterval_from_string_property             // G2
test_dateinterval_date_string_property             // G2
test_dateinterval_construct_from_string_false      // G2 (ctor init false)
test_dateinterval_f_property                       // S1
test_dateperiod_start_property                     // G4
test_dateperiod_current_property                   // G4
test_dateperiod_end_property                       // G4
test_dateperiod_interval_property                  // G4
test_dateperiod_recurrences_property               // G4
test_dateperiod_include_start_date_property       // G4
test_dateperiod_include_end_date_property          // G4
test_dateperiod_ctor_string_form                   // G5
test_dateperiod_get_end_date_returns_interface     // G6
test_dateperiod_get_start_date_returns_interface  // G7
test_dateperiod_instanceof_iterator_aggregate     // G8
test_get_last_errors_trailing_data                 // G11
test_get_last_errors_invalid_date_warning          // G11
test_get_last_errors_no_errors_returns_false      // G11 (false si clean)
test_get_last_errors_immutable                     // G11 (DateTimeImmutable partagé)
test_timezone_name_from_abbr_with_offset           // G17
test_create_from_interface                         // S2
test_create_from_iso8601_string                    // S3
test_format_iso8601_expanded                       // S4
```

### 4.3 Tests P2 (non-régression + audit)

```rust
test_diff_days_is_int                              // G13
test_diff_format_a_unknown_when_days_false        // G13
test_get_transitions_paris_185_rows                // R4 (non-régression borne sup)
test_get_transitions_row0_time_expanded_year       // R2 (non-régression bridge)
test_get_transitions_row0_ts_php_int_min           // R3
```

### 4.4 Tests de sérialisation

```rust
test_datetime_serialize                            // shape DateTime
test_datetime_unserialize                          // round-trip DateTime
test_dateinterval_serialize                        // shape ISO sans date_string
test_dateinterval_from_string_serialization_state // shape relative à deux clés
test_dateperiod_serialize                          // compte public + options
```

### 4.5 Critère de fin

- Tous les tests ci-dessus passent: `cargo test --test codegen_tests <name>`.
- `cargo test --test codegen_tests datetime`: **175 réussites, 0 échec, 0 ignored** lors de l'audit.
- `ELEPHC_PHP_CHECK=1 cargo test --test codegen_tests datetime`: mêmes **175 réussites Elephc**;
  les notes PHP restantes correspondent aux limitations listées en §9.
- `cargo build` sans warnings.
- `docs/php/datetime.md` mis à jour (limitations résolues retirées; limitations persistantes G12/G22/G23/S5 explicitement listées).
- `examples/datetime/main.php` mis à jour si pertinent.
- `ROADMAP.md`: item coché sous la version appropriée.
- `CHANGELOG.md`: entry `feat:` au prochain release.

## 5. Hors scope et limites structurelles

La sérialisation n'est plus une limitation globale: les hooks DateTime/DateTimeImmutable,
DateTimeZone, DateInterval et DatePeriod sont fonctionnels et leurs shapes observables sont testées.

| Limitation | Raison / statut |
|---|---|
| Notices runtime | Pas de canal complet `E_DEPRECATED`/`E_WARNING`; inclut strftime/gmstrftime, strptime, `SUNFUNCS_RET_*`, le ctor string DatePeriod, `RFC7231`, `idate()` invalide et les pseudo-propriétés DateInterval. |
| Attributs `#[Deprecated]` | Pas de surface reflection-visible équivalente sur les builtins synthétiques. |
| `__debugInfo()` | Pas de rendu `var_dump` DateTime équivalent. |
| Signatures de hooks | Les tableaux associatifs sont typés `mixed` dans le checker, car le backend ne convertit pas encore `AssocArray<string,mixed>` vers `Array<mixed>`. |
| `ext/calendar` | Traité séparément et hors du périmètre de cet audit. |
| `IntlDateFormatter` | Extension `intl`, hors `ext/date`. |
| 32-bit | Elephc cible les plateformes 64-bit supportées. |

## 6. Risques (consensus)

- **G4** (readonly mirror props): le type checker elephc doit accepter que des props publiques soient mutées par les méthodes synthétiques. Vérifier pas de contrainte `readonly` au niveau classe qui bloquerait. Si `is_readonly_class: false` (actuel), OK.
- **G9/G10/G10c** (validation ctor): peut casser du code existant qui catchait silencieusement. Ajouter tests valides + invalides. Matrice §3.7.
- **G19** (catch): la réécriture name_resolver doit produire un try/catch valide en EIR. Vérifier que le type checker accepte un catch de `DateMalformedStringException`.
- **G11** (messages): la liste des messages PHP n'est pas exhaustive. Cross-check `php -r` pour chaque cas de test. Risque de divergence de wording.
- **G15** (idate): la liste des specifiers reconnus doit être **exacte** (un seul char manquant ou en trop casse la parité).

## 7. Ordre d'implémentation (TDD)

1. **P0** (G19, G10/G10b/G10c, G9, G15, G24, R1, G25): tests + impl + `php -r` cross-check.
2. **P1** (G2, G4, G5, G6, G7, G8, G11, G17, S1-S4): tests + impl.
3. **P2** (G13, R2/R3/R4 non-régression, S5 audit): tests.
4. **P3** (G12 stubs + G22/G23 doc): stubs + tests + doc.
5. **Docs** (`docs/php/datetime.md`) + **ROADMAP** + **CHANGELOG** (`feat:`).

## 8. Validation padawans (consensus obtenu — v2.1)

Spec v2 → v2.1 revue par consensus de Kimi K2.7, Minimax M3, GLM 5.2 (round 2).

**Round 1 (v1→v2)**: corrections G1 (retired, PHP_INT_MAX), G19 (catch→false), G24 (keep micros), G12 (limitation not remedy), G11 (repriorisé P1), ajout S1-S5 + M-items + matrice validation G9/G10. Toutes vérifiées `php -r`.

**Round 2 (v2→v2.1)**: 3 points mineurs tranchés:
- **G15 warnings** (GLM+Kimi): PHP émet `E_WARNING` pour `idate` invalide. elephc n'a pas de système de warnings → **limitation documentée §5** (retourne `false` silencieusement).
- **G12 type d'exception** (Minimax): PHP utilise `DateObjectError` pour `__wakeup` return-type et `Error: Invalid serialization data` pour `__set_state` invalide. elephc stub un seul type (`DateInvalidOperationException`) car pas de round-trip réel → **nuance documentée §5**.
- **G22/G23 `#[Deprecated]` attributes** (Minimax): elephc ne supporte pas `#[Deprecated]` reflection-visible sur builtins synthétiques → **limitation documentée §5**.
- **G19b/G19c** (Kimi): `date_modify` alias + `date_create_from_format` alias doivent aussi catcher/retourner `false`. Ajoutés P0.
- **S6** (Minimax+Kimi): format specifiers couverture. Décision: les 260 tests existants `tests/codegen/system.rs` sont la référence. Mapping à documenter dans le test header, pas de matrice paramétrique additionnelle.
- **G11 état global** (Kimi): `DateTime::getLastErrors()` et `DateTimeImmutable::getLastErrors()` partagent l'état global. Test étendu + alias `date_get_last_errors()`.

**Verdict de la v2.1 historique**: cette première relecture avait validé le plan TDD, mais
l'affirmation de conformité complète était trop forte. Le statut ci-dessous, issu de l'audit
post-rebase, remplace ce verdict.

## 9. Statut d'implémentation après audit

### Corrections verrouillées

| Surface | Statut | Régressions |
|---|---|---|
| `DatePeriod::$recurrences` | Corrigé: compte public PHP distinct de `getRecurrences()` | `test_dateperiod_recurrences_property`, `test_dateperiod_serialize` |
| Constructeur string `DatePeriod` runtime | Corrigé par réécriture selon l'arité 1/2 | `test_dateperiod_ctor_string_form` |
| `DateInterval` pseudo-propriétés | Retirées de la surface publique; forme sérialisée conservée | `test_dateinterval_*_serialization_state`, `test_dateinterval_serialize` |
| `DateTimeInterface` | `diff`, `__wakeup`, `__serialize`, `__unserialize` ajoutés | `test_datetime_interface_diff_and_serialize_contract` |
| `getTransitions()` | Borne par défaut `2147483647` | tests transition Paris/row 0 |
| Offset `DateTimeZone` | Formes timelib, préfixe `GMT`, secondes et canonicalisation | `test_datetimezone_offset_valid` |
| `getLastErrors()` | État global partagé + positions principales | `test_datetime_get_last_errors_shared_between_classes` |
| `idate()` dynamique | Même validation runtime que les littéraux | `test_idate_dynamic_unknown_format_returns_false` |
| `timezone_name_from_abbr()` | Table timelib complète, offset exact et fallback offset/DST | `test_timezone_name_from_abbr`, `test_timezone_name_from_abbr_with_offset` |
| `createFromFormat()` zones | `O`/`P`/`T`/`e` remplacent le troisième argument; `Z` est littéral | `test_create_from_format_tz_specifiers` |
| Microsecondes | `setTimestamp()` remet à zéro; conversions et `setISODate()` conservent | `test_datetime_microseconds`, `test_datetime_create_from_object_conversions`, `test_datetime_set_isodate` |
| Ownership timezone | L'adaptateur retourne une chaîne possédée et ne libère plus le nom stocké | `test_date_create_with_timezone_arg`, filtre `timezone_` |
| Rebase `origin/main` | Constructeurs AST remis au contrat courant | `cargo check --tests`, build/tests ciblés |

### Limitations résiduelles assumées

| Surface | Écart restant |
|---|---|
| Notices/warnings | Pas de `E_DEPRECATED`/`E_WARNING` runtime pour les surfaces documentées. |
| `createFromFormat()` | Les principaux messages/positions sont reproduits, pas toute la table timelib ni tous les comptages multi-erreurs. |
| `DatePeriod` | Elephc utilise encore l'objet comme `Iterator` en plus d'`IteratorAggregate`; `getIterator()` n'est pas indépendant et les propriétés miroir ne sont pas protégées readonly. |
| `DateInterval` debug fields | La lecture directe est rejetée au lieu d'émettre un warning et de retourner `null`; la forme sérialisée reste conforme. |
| Signatures de sérialisation | Les hooks sont présents et fonctionnels, mais leur tableau associatif est typé `mixed` dans le checker: le backend ne sait pas encore convertir `AssocArray<string,mixed>` vers son `Array<mixed>` indexé. |
| Parsing libre | `date_parse_from_format()` ne reproduit pas encore chaque shape PHP; `date_parse()` n'a pas les comptages exhaustifs; `strtotime()` rejette encore certains cas timelib ambigus acceptés par PHP 8.5 (`2024/06/15`, `2024-0x-15`, suffixe de zone mono-lettre). |

### Résultat de validation

- `CARGO_INCREMENTAL=0 cargo test --test codegen_tests datetime -- --nocapture`:
  **175 réussites, 0 échec, 0 ignored**.
- Même résultat avec `ELEPHC_PHP_CHECK=1`; les notes restantes correspondent aux limitations
  ci-dessus et aux notices runtime absentes.
- Aucun `#[ignore]` datetime n'est présenté comme une réussite.
