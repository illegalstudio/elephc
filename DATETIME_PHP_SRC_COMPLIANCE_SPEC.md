---
title: "DateTime php-src Compliance Spec v3.0"
description: "Conformité ext/date vérifiée contre php-src 8.5 HEAD, timelib et tzdb 2026.3."
---

# DateTime php-src Compliance Spec v3.0

## Référence normative

- Branche Elephc: `feat/datetime-php-src-compliance`.
- php-src: commit `47b563cbb856ec19155aacc3246931dfacbebd21` (`PHP 8.5.10-dev`).
- Sources: `ext/date/php_date.stub.php`, `ext/date/php_date.c`, `ext/date/lib/` et les PHPT
  de `ext/date/tests/`.
- Base timezone: timelib et IANA tzdb `2026.3`, vendus dans `crates/elephc-tz`.
- Cibles Elephc: macOS AArch64, Linux AArch64 et Linux x86_64.
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

### Parsing et arithmétique

Elephc compile la copie vendue du timelib de php-src pour:

- `strtotime()`, `date_parse()` et `date_parse_from_format()`;
- `DateTime*::createFromFormat()` et `getLastErrors()`;
- la grammaire ISO et relative de `DateInterval`;
- la grammaire ISO de `DatePeriod`;
- `DateTime*::add()`/`sub()` et les distinctions wall/civil de php-src.

Les structures C sont couvertes par des assertions de taille/offset pour les trois cibles
64 bits. Les objets timelib et leurs `tz_info` suivent le contrat d'ownership du code vendu.

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
- arithmétique, microsecondes, DST, ownership, COW et parité AArch64/x86_64.

Commande locale principale:

```bash
ELEPHC_PHP_CHECK=1 cargo test --test codegen_tests datetime -- --nocapture
```

Le contrôle PHP intégré peut utiliser un PHP système différent. Les verdicts de l'audit sont donc
épinglés sur le binaire construit depuis le commit php-src normatif ci-dessus; les différences
d'environnement Xdebug ou de version tzdb d'un PHP système ne constituent pas l'oracle.

## Validation indépendante

Les trois relectures Ollama ont reçu le stub php-src, les sources/diffs Elephc et les tests
pertinents en ligne:

- Kimi K2.7: surface, signatures, constantes et Reflection;
- GLM-5.2: ABI timelib, parsing, DateInterval, DatePeriod et ownership;
- MiniMax M3: diagnostics, sérialisation, propriétés virtuelles, Reflection et parité de cibles.

Chaque alerte a été rejouée contre le php-src normatif. Les faux positifs documentés pendant la
revue incluaient l'appartenance de `strptime` à `standard`, le type 2 de `GMT`, le rejet de la
soustraction d'un weekday relatif, le contrat de `timelib_time_dtor`, les clés optionnelles de
`date_parse()["relative"]` et les appels `timelib_update_ts(..., NULL)` de DatePeriod.

Les écarts confirmés par la revue — hooks Reflection manquants, identifiants IANA sans slash,
zones militaires attachées, `format("T"/"e")`, signatures nullable/unions et messages TypeError —
ont été corrigés et verrouillés par régression.

## Critère de clôture

La branche est publiable uniquement si:

- le gate datetime complet est vert;
- `cargo test -p elephc-tz` est vert;
- `cargo check --tests` et `cargo build` sont sans warning;
- `git diff --check` est propre;
- les trois revues ne conservent aucun écart confirmé;
- la CI valide les trois cibles supportées.

Aucune limitation résiduelle connue de la surface `ext/date` auditée n'est acceptée sur cette
branche. Toute divergence future contre le commit normatif doit rouvrir ce document et ajouter une
régression avant correction.
