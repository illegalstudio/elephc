<?php
class MixedLiteralOwner {
    public static int $retired = 0;
    public function __destruct() { self::$retired++; gc_collect_cycles(); }
}
function mixedLiteralValue(): MixedLiteralOwner { echo 'V'; gc_collect_cycles(); return new MixedLiteralOwner(); }
function mixedLiteralKey(mixed $key): mixed { return $key; }
function mixedLiteralConstruction(mixed $key): void {
    try {
        $built = [...['seed' => new MixedLiteralOwner()], $key => mixedLiteralValue(), new MixedLiteralOwner()];
        echo count($built), ':';
        unset($built);
    } catch (TypeError $error) { echo 'K'; }
}
function mixedLiteralBadSpread(mixed $source): void {
    try { $built = ['seed' => new MixedLiteralOwner(), ...$source]; echo 'bad'; }
    catch (Error $error) { echo 'S'; }
}
function mixedLiteralThrowingKey(): mixed { echo 'T'; throw new RuntimeException('key'); }
function mixedLiteralKeyFailure(): void {
    try { $built = ['seed' => new MixedLiteralOwner(), ...[1], mixedLiteralThrowingKey() => mixedLiteralValue()]; }
    catch (RuntimeException $error) { echo 'E'; }
}
mixedLiteralConstruction(false);
gc_collect_cycles();
echo '|', MixedLiteralOwner::$retired, '|';
MixedLiteralOwner::$retired = 0;
mixedLiteralBadSpread(false);
gc_collect_cycles();
echo '|', MixedLiteralOwner::$retired, '|';
MixedLiteralOwner::$retired = 0;
mixedLiteralKeyFailure();
gc_collect_cycles();
echo '|', MixedLiteralOwner::$retired, '|';
$spread = [];
for ($i = 0; $i < 40; $i++) { $spread['s' . $i] = $i; }
$grown = [...$spread, 'tail' => 42, 99];
echo count($grown), ':', $grown['tail'], ':', $grown[0], ':', count($spread), '|';
$identity = [mixedLiteralKey(1) => 10, ...[mixedLiteralKey('1') => 20], mixedLiteralKey('01') => 30, 40];
echo count($identity), ':', $identity[1], ':', $identity['1'], ':', $identity['01'], ':', $identity[2], '|';
