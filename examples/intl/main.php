<?php
// Format numbers, sort user-facing text, and render localized messages with ext/intl.

$price = new NumberFormatter('en_US', NumberFormatter::CURRENCY);
echo $price->formatCurrency(42.5, 'USD'), "\n";

$collator = new Collator('en_US');
$names = ['Émile', 'alice', 'Bob', 'charlie'];
$collator->sort($names);
echo implode(', ', $names), "\n";

$messages = new MessageFormatter('en_US', '{count, plural, one {# item} other {# items}}');
echo $messages->format(['count' => 2]), "\n";

$format = new IntlDateFormatter(
    'en_US',
    IntlDateFormatter::FULL,
    IntlDateFormatter::NONE,
    'UTC',
    IntlDateFormatter::GREGORIAN,
    'yyyy-MM-dd',
);
$date = new DateTimeImmutable('2026-11-06 12:00:00', new DateTimeZone('UTC'));
echo $format->format($date), "\n";

$zone = IntlTimeZone::createTimeZone('Europe/Paris');
echo $zone->getID(), ' ', $zone->getRawOffset(), "\n";

$normalized = Normalizer::normalize("cafe\u{0301}", Normalizer::FORM_C);
echo grapheme_strlen($normalized), ' ', $normalized, "\n";
