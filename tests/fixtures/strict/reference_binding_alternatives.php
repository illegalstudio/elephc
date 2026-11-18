<?php
function writeSelectedReference(mixed &$target, mixed $replacement): void { $target = $replacement; }
function selectReferenceOrigin(bool $integer, mixed $replacement): void {
    if ($integer) { $value = 1; } else { $value = 'before'; }
    try { writeSelectedReference(target: $value, replacement: $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectReferenceOrigin(true, 'after');
selectReferenceOrigin(false, 2);
selectReferenceOrigin(true, 2);
selectReferenceOrigin(false, 'after');

function selectCallableOrigin(bool $integer, mixed $replacement): void {
    $write = writeSelectedReference(...);
    if ($integer) { $value = 1; } else { $value = 'before'; }
    try { $write($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectCallableOrigin(true, 'after');
selectCallableOrigin(false, 2);

class SelectedReferenceWriter {
    public function write(mixed &$target, mixed $replacement): void { $target = $replacement; }
    public static function writeStatic(mixed &$target, mixed $replacement): void { $target = $replacement; }
}
function selectMethodOrigin(bool $integer, mixed $replacement): void {
    $writer = new SelectedReferenceWriter();
    if ($integer) { $value = 1; } else { $value = 'before'; }
    try { $writer->write($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectMethodOrigin(true, 'after');
selectMethodOrigin(false, 2);
function selectStaticOrigin(bool $integer, mixed $replacement): void {
    if ($integer) { $value = 1; } else { $value = 'before'; }
    try { SelectedReferenceWriter::writeStatic($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectStaticOrigin(true, 'after');
selectStaticOrigin(false, 2);

function selectNumericReferenceOrigin(bool $integer, mixed $replacement): void {
    if ($integer) { $value = 1; } else { $value = 2.5; }
    try { writeSelectedReference($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectNumericReferenceOrigin(true, 2.5);
selectNumericReferenceOrigin(false, 1);
selectNumericReferenceOrigin(false, 3.5);
function selectOpenReferenceOrigin(bool $open, mixed $producer, mixed $replacement): void {
    if ($open) { $value = $producer; } else { $value = 1; }
    try { writeSelectedReference($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectOpenReferenceOrigin(true, 1, 'after');
selectOpenReferenceOrigin(false, 1, 'after');
class ConstructReferenceOrigin {
    public function __construct(mixed &$target, mixed $replacement) { $target = $replacement; }
}
function selectConstructorOrigin(bool $integer, mixed $replacement): void {
    if ($integer) { $value = 1; } else { $value = 'before'; }
    try { $created = new ConstructReferenceOrigin($value, $replacement); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
selectConstructorOrigin(true, 'after');
selectConstructorOrigin(false, 2);
selectConstructorOrigin(true, 2);
