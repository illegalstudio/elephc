<?php declare(strict_types=0);
try { echo [1][9]; } catch (Throwable $error) { echo get_class($error), '|'; }
try { echo 'abc'[9]; } catch (Throwable $error) { echo 'S'; }
function indexedHoldSlot(&$slot): void { $slot = 1; }
$a = ['x'=>1]; indexedHoldSlot($a['x']); $heldValue = $a['x']; unset($a['x']);
try { echo $a['x']; } catch (OutOfBoundsException $error) { echo 'R'; }
echo ':', $heldValue, '|';
$probe = []; $probe['new'] = $probe['new'] ?? 7; echo $probe['new'];
$nested = []; unset($nested['missing']['inner']);
$nested['outer']['inner'] = 8; echo ':', $nested['outer']['inner'], '|';
function indexedCreateSlot(&$slot): void { $slot = 9; }
$created = ['existing'=>0]; indexedCreateSlot($created['created']); echo $created['created'], '|';
try { $a['absent'] += 1; } catch (OutOfBoundsException $error) { echo 'C'; }
$words = explode(',', 'one,two');
try { echo $words[8]; } catch (OutOfBoundsException $error) { echo 'B'; }
