---
title: "DateTime php-src Compliance Spec v4.0"
description: "Conformité ext/date vérifiée contre php-src 8.5.10-dev, timelib et tzdb 2026.3."
---

# DateTime php-src Compliance Spec v4.0

## Référence normative

- Branche Elephc: `feat/datetime-php-src-compliance`.
- php-src: commit `47b563cbb856ec19155aacc3246931dfacbebd21` (`PHP 8.5.10-dev`).
- Sources: `ext/date/php_date.stub.php`, `ext/date/php_date.c`, `ext/date/lib/` et les PHPT
  de `ext/date/tests/`.
- Base timezone: timelib et IANA tzdb `2026.3`, vendus dans `crates/elephc-tz`.
- Cibles Elephc: macOS AArch64, iOS ARM64, iOS Simulator ARM64, Linux AArch64 et
  Linux x86_64.
- `ext/calendar` est validé séparément et n'entre pas dans ce périmètre.

La référence est le comportement observable de php-src: surface, ordre et casse des symboles,
signatures Reflection, constantes et attributs, retours, exceptions, messages, warnings,
sérialisation, debug, parsing, arithmétique et effets de bord.

## Surface verrouillée

L'inventaire comprend les 48 fonctions exposées par `get_extension_funcs("date")`, les fonctions
standard associées (`strptime`, `gettimeofday`, `microtime`, `hrtime`), et les types suivants:

- `DateTimeInterface`;
- `DateTime` et `DateTimeImmutable`;
- `DateTimeZone`;
- `DateInterval`;
- `DatePeriod`;
- `DateError`, `DateObjectError`, `DateRangeError`, `DateException`,
  `DateInvalidTimeZoneException`, `DateInvalidOperationException`,
  `DateMalformedStringException`, `DateMalformedIntervalStringException` et
  `DateMalformedPeriodStringException`.

Les constantes globales `DATE_*`/`SUNFUNCS_*`, les constantes de format de
`DateTimeInterface`, les sélecteurs de `DateTimeZone` et les options de `DatePeriod` conservent
leurs valeurs, types, classes déclarantes et attributs `Deprecated`.

## Implémentation

### Déclarations AST sans PHP synthétique en production

Les déclarations `DateTimeInterface`, `DateTime`, `DateTimeImmutable`, `DateTimeZone`,
`DateInterval`, `DatePeriod` et les helpers timelib sont des AST Rust directs, générés et
versionnés dans:

- `src/types/checker/builtin_types/datetime/generated_declarations_timelib.rs`;
- `src/types/checker/builtin_types/datetime/generated_declarations_fallback.rs`;
- `src/tz_prelude/generated_timelib.rs`.

Le chemin de compilation ne contient aucune source PHP synthétique renvoyée au lexer/parser.
Les anciennes sources PHP et modèles déclaratifs sont conservés exclusivement sous `cfg(test)`
comme oracles de génération. Les tests comparent la structure AST complète, notamment types et
attributs de constantes, attributs de méthodes et hooks de propriétés virtuelles; seuls les spans,
le marquage historique du mode source et le padding vide des attributs de paramètres sont
normalisés.

### Parsing et arithmétique

Elephc compile la copie vendue du timelib de php-src pour:

- `strtotime()`, `date_parse()` et `date_parse_from_format()`;
- `DateTime*::createFromFormat()` et `getLastErrors()`;
- la grammaire ISO et relative de `DateInterval`;
- la grammaire ISO de `DatePeriod`;
- `DateTime*::add()`/`sub()` et les distinctions wall/civil de php-src.

Les structures C sont couvertes par des assertions compile-time exhaustives de taille,
d'alignement et d'offset sur les ABI 64 bits de toutes les cibles supportées. Les objets timelib
et leurs `tz_info` suivent le contrat d'ownership du code vendu.

### Fuseaux horaires

- Les tables location, transitions et abréviations sont générées depuis le même php-src.
- `timezone_version_get()` retourne `2026.3`.
- Les identifiants IANA avec ou sans `/`, les alias BC, les abréviations, les lettres militaires
  et les offsets avec secondes conservent le `timezone_type`, le nom public et l'offset php-src.
- `format("e")` conserve le nom public; `format("T")` conserve les zones de type 2 et résout
  l'abréviation active des zones de type 3.
- Les offsets fixes sont adaptés à la convention POSIX inversée uniquement à la frontière libc;
  cette adaptation n'altère jamais le nom PHP stocké.

### Diagnostics et suppression

Le canal de diagnostics runtime émet les `E_WARNING`/`E_DEPRECATED` date/time concernés et
l'opérateur `@` les supprime avec restauration du niveau de suppression après exceptions.
Cela couvre notamment:

- `idate()` invalide;
- `strftime()`/`gmstrftime()` et `strptime()`;
- le constructeur string de `DatePeriod`;
- `SUNFUNCS_RET_*` et `DateTimeInterface::RFC7231`;
- les hooks `__wakeup()` et les résultats ignorés de `DateTimeImmutable`;
- les pseudo-propriétés debug de `DateInterval`.

Les exceptions et messages des constructeurs, parseurs, sérialisations et arguments invalides
suivent php-src, y compris le type concret rejeté dans les `TypeError` de `add()`/`sub()` et de
`date_add()`/`date_sub()`.

### Reflection, sérialisation et debug

- Les inventaires de fonctions et de méthodes sont verrouillés en ordre/casse php-src.
- Les signatures procédurales vérifient noms, types nullables/unions, optionnalité, références,
  variadiques et retours.
- Les helpers internes `__elephc_*` restent absents de Reflection et des probes PHP.
- Les hooks `__serialize`, `__unserialize`, `__set_state` et `__wakeup` reproduisent les shapes et
  erreurs de `DateTime`, `DateTimeImmutable`, `DateTimeZone`, `DateInterval` et `DatePeriod`.
- Les déclarations utilisateur `__set_state` suivent l'ordre Zend: arité d'un paramètre fixe,
  référence interdite sur ce paramètre fixe, staticité, avertissement de visibilité, type acceptant
  `array`, puis retour objet. Un variadique de queue est autorisé par valeur ou par référence;
  classes, interfaces et traits partagent ce contrat.
- `var_dump()` utilise les handlers virtuels php-src pour les cinq familles d'objets.
- Les sept propriétés de `DatePeriod` sont virtuelles, non modifiables par l'utilisateur et
  adossées à un itérateur indépendant retourné par `getIterator()`.

## Régressions exhaustives

Les gates permanents comprennent:

- inventaire des fonctions ext/date, avec lookup insensible à la casse;
- inventaire et ordre exact des méthodes des six types principaux;
- inventaire complet des signatures procédurales;
- constantes globales/de classe, attributs `Deprecated`, `NoDiscard` et retours tentatifs;
- hiérarchie des exceptions;
- shapes Reflection, sérialisation et `var_dump`;
- diagnostics byte-positionnés de timelib et `getLastErrors()`;
- grammaires libres/ISO/relatives, DateInterval et DatePeriod;
- offsets IANA, alias, POSIX, abréviations et lettres militaires;
- nullable timestamps de `date`, `gmdate`, `strtotime`, `localtime`, `getdate`,
  `mktime` et `gmmktime`;
- portée sans valeur du cast `(void)` (statement racine et clauses init/update de `for`, jamais
  dans une expression produisant une valeur), `E_ALL` 8.4+ sans `E_STRICT`, dépréciation de
  `E_STRICT` et masque fatal observable de `error_reporting()` sous `@`;
- unary plus aligné sur Zend (`operand * 1`), y compris chaînes numériques int/float, warning
  des préfixes numériques, `TypeError` catchable et message dépendant du type runtime;
- comparaisons relationnelles de valeurs `Mixed` routées par la table d'ordre PHP
  (`PhpRelCmp`) sans troncature entière préalable;
- arithmétique, microsecondes, DST, ownership, COW et parité AArch64/x86_64.

Verdict final sur cette révision:

- filtre DateTime codegen: `319 passed; 0 failed; 0 ignored`;
- PHPT `ext/date`: 692 fichiers uniques, soit 628 sorties exactes, 15 différences limitées aux
  identifiants d'objet, 36 équivalences `EXPECT`/`EXPECTF` et 13 skips php-src;
- donc 679/679 PHPT exécutables conformes, sans différence de code de sortie ni timeout;
- suites `--lib` des membres workspace par défaut: `3259 passed; 0 failed; 3 ignored`;
- intégration CLI `error_reporting=E_ALL&~E_DEPRECATED` sous le profil PHP 8.5: `1 passed`,
  masque `22527` identique au binaire php-src normatif (GLM-5.3 a détecté puis fait corriger
  l'ancien attendu PHP 8.3 `24575` avant le gel final);
- `elephc-tz`: 36/36 sur macOS AArch64, Linux AArch64 et Linux x86_64; `cargo check`
  vert sur iOS ARM64 et iOS Simulator ARM64, ce qui évalue les assertions ABI compile-time sur
  les cinq cibles supportées;
- documentation des builtins: 0 erreur sur 522 pages utilisateur, 516 pages internes et 1106
  pages générées validées;
- assembly DateTime non vide sur les cinq cibles; archives statiques iOS device et simulator
  assemblées et liées.

Commande locale principale:

```bash
CARGO_BUILD_JOBS=1 RUST_MIN_STACK=67108864 \
  cargo test --test codegen_tests datetime -- --test-threads=1
```

Le contrôle PHP intégré peut utiliser un PHP système différent. Les verdicts de l'audit sont donc
épinglés sur le binaire construit depuis le commit php-src normatif ci-dessus; les différences
d'environnement Xdebug ou de version tzdb d'un PHP système ne constituent pas l'oracle.

## Validation indépendante

La revue finale est fail-closed et porte sur le hash SHA-256 exact de cette spec. Kimi K3 et
GLM-5.3 sont exécutés séquentiellement via Ollama, puis GPT-5.6 Sol effectue une troisième revue
indépendante en raisonnement `high`. Chaque reviewer reçoit le hash,
le commit php-src normatif, le diff Elephc, les inventaires/tests et les résultats PHPT/matrice;
il doit répondre `LOCK <hash>` ou fournir des écarts reproductibles. Tout écart doit décrire le
manque, le problème ou la faille, son impact et son reproducer, avec les liens précis vers le code
php-src, la documentation officielle et les emplacements Elephc pertinents. Toute modification de
cette spec invalide les trois locks et impose leur réexécution.

Chaque reviewer couvre l'intégralité du diff, notamment surface/Reflection, bridge
timelib/ownership, diagnostics/sérialisation et parité de cibles; chacun peut bloquer la publication
sur n'importe quel écart confirmé. Les sorties de revue sont conservées séparément comme preuve
afin de ne pas modifier la spec après verrouillage.

## Critère de clôture

La branche est publiable uniquement si:

- le gate datetime complet est vert;
- les 679 PHPT exécutables sont conformes et les 13 autres ont le même statut skip que php-src;
- les suites `--lib` des membres workspace par défaut sont sans échec;
- `cargo test -p elephc-tz` est vert;
- `cargo check --tests` et `cargo build` sont sans warning;
- le workflow `update-builtin-docs` est vert;
- `git diff --check` est propre;
- les trois revues ne conservent aucun écart confirmé;
- les cinq cibles émettent l'assembly DateTime, les deux cibles iOS lient une archive statique;
- la CI valide les exécutions natives macOS AArch64, Linux AArch64 et Linux x86_64.

Aucune limitation résiduelle connue de la surface `ext/date` auditée n'est acceptée sur cette
branche. Toute divergence future contre le commit normatif doit rouvrir ce document et ajouter une
régression avant correction.
