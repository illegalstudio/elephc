<?php
class MixedSortMethods {
 public function compare(mixed $left, mixed $right): int {
  if (func_num_args() !== 2) { throw new RuntimeException("argument count"); }
  return $left <=> 1.0;
 }
 public static function staticCompare(mixed $left, mixed $right): int {
  if (func_num_args() !== 2) { throw new RuntimeException("argument count"); }
  return $left <=> 1.0;
 }
 public function __destruct() { echo 'D'; }
}
$receiver = new MixedSortMethods(); $items = [2, 1];
try { usort($items, $receiver->compare(...)); echo 'P'; }
catch (TypeError $instanceError) { echo 'M'; unset($instanceError); }
$other = [2, 1];
try { usort($other, MixedSortMethods::staticCompare(...)); echo 'P'; }
catch (TypeError $staticError) { echo 'S'; unset($staticError); }
$strings = [str_repeat('b', 2), str_repeat('a', 2)];
try { usort($strings, $receiver->compare(...)); echo 'P'; }
catch (TypeError $stringInstanceError) { echo 'A'; unset($stringInstanceError); }
$staticStrings = [str_repeat('d', 2), str_repeat('c', 2)];
try { usort($staticStrings, MixedSortMethods::staticCompare(...)); echo 'P'; }
catch (TypeError $stringStaticError) { echo 'B'; unset($stringStaticError); }
unset($receiver); gc_collect_cycles();
