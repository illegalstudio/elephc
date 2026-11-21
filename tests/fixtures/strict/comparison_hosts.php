<?php
function publicHostLess(mixed $left, mixed $right): bool { return $left < $right; }
function publicHostMappedLess(mixed $left): bool { return publicHostLess($left, 1.0); }
function publicHostBadOrder(mixed $left, mixed $right): mixed { return true; }
$less = publicHostLess(...);
$name = $argc > 0 ? 'publicHostLess' : 'publicHostLess';
try { $less(1, 1.0); } catch (TypeError $hostA) { echo 'A'; unset($hostA); }
try { $name(1, 1.0); } catch (TypeError $hostB) { echo 'B'; unset($hostB); }
try { (new ReflectionFunction('publicHostLess'))->invoke(1, 1.0); }
catch (TypeError $hostC) { echo 'C'; unset($hostC); }
try { (new ReflectionFunction('publicHostLess'))->invokeArgs([1, 1.0]); }
catch (TypeError $hostD) { echo 'D'; unset($hostD); }
try { call_user_func('publicHostLess', 1, 1.0); }
catch (TypeError $hostE) { echo 'E'; unset($hostE); }
try { call_user_func_array('publicHostLess', [1, 1.0]); }
catch (TypeError $hostF) { echo 'F'; unset($hostF); }
try { array_map('publicHostMappedLess', [1]); }
catch (TypeError $hostG) { echo 'G'; unset($hostG); }
$hostItems = [2, 1]; $sort = usort(...);
try { $sort($hostItems, publicHostBadOrder(...)); }
catch (TypeError $hostH) { echo 'H'; unset($hostH); }
try { (new ReflectionFunction('array_udiff'))->invokeArgs([[2], [1], publicHostBadOrder(...)]); }
catch (TypeError $hostI) { echo 'I'; unset($hostI); }
$automatic = array_unique(...);
try { $automatic([1, 1]); }
catch (TypeError $hostJ) { echo 'J'; unset($hostJ); }
echo '|';
$fragment = $argc > 0 ? '
function opaqueHostLess(mixed $left, mixed $right): bool { return $left < $right; }
$opaqueLess = opaqueHostLess(...);
try { publicHostLess(1, 1.0); } catch (TypeError $opaqueA) { echo "N"; unset($opaqueA); }
try { opaqueHostLess(1, 1.0); } catch (TypeError $opaqueB) { echo "E"; unset($opaqueB); }
try { $less(1, 1.0); } catch (TypeError $opaqueC) { echo "C"; unset($opaqueC); }
try { $opaqueLess(1, 1.0); } catch (TypeError $opaqueD) { echo "L"; unset($opaqueD); }
try { (new ReflectionFunction("publicHostLess"))->invoke(1, 1.0); }
catch (TypeError $opaqueE) { echo "R"; unset($opaqueE); }
try { call_user_func("opaqueHostLess", 1, 1.0); }
catch (TypeError $opaqueF) { echo "U"; unset($opaqueF); }
echo "|"; echo $less("10", "9") ? "L" : "X";
echo $opaqueLess("10", "9") ? "L" : "X";
' : '';
eval($fragment);
gc_collect_cycles();
