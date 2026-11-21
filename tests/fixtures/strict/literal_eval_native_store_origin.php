<?php
function reverseLiteralOrigin(int $seed): void {
    eval('$created = 2.5;');
    try { $created = $seed; echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created, '|';
}
reverseLiteralOrigin($argc);

function scopeOwnedReplacement(string $text): string { echo 'O'; return $text . '!'; }
function scopeValidNativeStore(): void {
    eval('$created = 2.5;'); $created = 3.5;
    try { eval('$created = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created, '|';
}
function scopeOwnedNativeStore(): void {
    eval('$created = 2.5;');
    try { $created = scopeOwnedReplacement('bad'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created, '|';
}
function scopeArrayNativeStore(mixed $replacement): void {
    eval('$created = [1];');
    try { $created = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($created), '|';
}
scopeValidNativeStore(); scopeOwnedNativeStore(); scopeArrayNativeStore(1);

function scopeOpenNativeStore(): void {
    mixed $created = 2.5; eval('$created = 2.5;');
    $created = 1; echo gettype($created), ':', $created, '|';
}
function scopeNativeValueCopy(): void {
    eval('$created = 2.5;'); $copy = $created; $copy = 3.5;
    echo $created, ':', $copy, '|';
}
function replaceScopedFloat(float &$target, float $replacement): void { $target = $replacement; }
function scopeTypedNativeReference(): void {
    eval('$created = 2.5;'); replaceScopedFloat($created, 4.5);
    try { eval('$created = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created, '|';
}
function scopeNativeArrayElements(): void {
    eval('$created = [1];'); $created = ["ok"];
    try { eval('$created = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created[0], '|';
}
scopeOpenNativeStore(); scopeNativeValueCopy(); scopeTypedNativeReference(); scopeNativeArrayElements();
