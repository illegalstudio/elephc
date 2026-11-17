<?php
interface R61Marker {}
class R61Base {}
class R61Both extends R61Base implements R61Marker {}
function r61Marker(R61Marker $value): void { echo "marker\n"; }
function r61Finally(?R61Base $value): int {
    if ($value === null) { return 0; }
    if ($value instanceof R61Marker) {
        try { return 1; }
        finally { r61Marker($value); }
    }
    return 0;
}
function r61Catch(?R61Base $value): int {
    if ($value === null) { return 0; }
    if ($value instanceof R61Marker) {
        try { throw new Exception('edge'); }
        catch (Exception $error) { r61Marker($value); }
        return 1;
    }
    return 0;
}
function r61Joined(?R61Base $value, bool $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R61Marker) {
        try {
            if ($flag) { throw new Exception('first'); }
            throw new Exception('second');
        } catch (Exception $error) {
            r61Marker($value);
        } finally {
            r61Marker($value);
        }
        return 1;
    }
    return 0;
}
function r61Loop(?R61Base $value): int {
    if ($value === null) { return 0; }
    if ($value instanceof R61Marker) {
        for ($i = 0; $i < 1; $i++) {
            try { continue; }
            finally { r61Marker($value); }
        }
        do {
            try { break; }
            finally { r61Marker($value); }
        } while (false);
        r61Marker($value);
        return 1;
    }
    return 0;
}
var_dump(r61Finally(new R61Both()), r61Finally(null));
var_dump(r61Catch(new R61Both()), r61Catch(null));
var_dump(r61Joined(new R61Both(), true), r61Joined(new R61Both(), false));
var_dump(r61Loop(new R61Both()), r61Loop(null));
