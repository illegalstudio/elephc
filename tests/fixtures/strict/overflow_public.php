<?php
function overflowPublicAdd(mixed $left, mixed $right): mixed { return $left + $right; }
function overflowPublicSub(mixed $left, mixed $right): mixed { return $left - $right; }
function overflowPublicMul(mixed $left, mixed $right): mixed { return $left * $right; }
function overflowPublicPow(mixed $left, mixed $right): mixed { return $left ** $right; }
function overflowPublicNegate(mixed $value): mixed { return -$value; }
function overflowPublicAbs(mixed $value): mixed { return abs($value); }
function overflowPublicSum(array $values): mixed { return array_sum($values); }
function overflowPublicPost(mixed &$value): mixed { return $value++; }
try { overflowPublicAdd(PHP_INT_MAX, 1); echo 'X'; } catch (ArithmeticError $error) { echo 'A'; }
try { overflowPublicSub(PHP_INT_MIN, 1); echo 'X'; } catch (ArithmeticError $error) { echo 'S'; }
try { overflowPublicMul(PHP_INT_MAX, 2); echo 'X'; } catch (ArithmeticError $error) { echo 'M'; }
try { overflowPublicPow(2, 63); echo 'X'; } catch (ArithmeticError $error) { echo 'P'; }
try { overflowPublicNegate(PHP_INT_MIN); echo 'X'; } catch (ArithmeticError $error) { echo 'N'; }
try { overflowPublicAbs(PHP_INT_MIN); echo 'X'; } catch (ArithmeticError $error) { echo 'B'; }
try { overflowPublicSum([PHP_INT_MAX, 1]); echo 'X'; } catch (ArithmeticError $error) { echo 'R'; }
$value = PHP_INT_MAX;
try { overflowPublicPost($value); echo 'X'; } catch (ArithmeticError $error) { echo 'U'; }
echo $value === PHP_INT_MAX ? 'K' : 'X';
$items = [PHP_INT_MAX];
try { $items[0] += 1; echo 'X'; } catch (ArithmeticError $error) { echo 'C'; }
echo $items[0] === PHP_INT_MAX ? 'K' : 'X';
class OverflowPublicCell { public int $value = PHP_INT_MAX; }
$cell = new OverflowPublicCell();
try { $cell->value++; echo 'X'; } catch (ArithmeticError $error) { echo 'T'; }
echo $cell->value === PHP_INT_MAX ? 'K' : 'X';
$divide = intdiv(...);
try { $divide(PHP_INT_MIN, -1); echo 'X'; } catch (ArithmeticError $error) { echo 'I'; }
echo '|';
try { echo overflowPublicAdd('2', 1); } catch (TypeError $error) { echo 'D'; }
echo '|';
try { echo overflowPublicPow(2, -1); } catch (ArithmeticError $error) { echo 'Q'; }
echo '|', is_float(4 / 2) ? 'F' : 'I', '|';
$code = $argc > 0 ? '$evalOverflow = PHP_INT_MAX; try { $evalOverflow++; echo "X"; } catch (ArithmeticError $error) { echo "U"; } echo $evalOverflow === PHP_INT_MAX ? "K" : "X"; try { $evalOverflow + 1; echo "X"; } catch (ArithmeticError $error) { echo "A"; } try { abs(PHP_INT_MIN); echo "X"; } catch (ArithmeticError $error) { echo "B"; } try { array_sum([PHP_INT_MAX, 1]); echo "X"; } catch (ArithmeticError $error) { echo "R"; }' : 'return 0;';
eval($code); eval($code);
