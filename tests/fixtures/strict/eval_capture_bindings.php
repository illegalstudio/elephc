<?php
$code=$argc>0?'
function captureInteger(int $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureNumeric(int|float $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureMixed(mixed $value,mixed $next): void {
    $write=static function() use($value,$next): void {$value=$next;echo "M",gettype($value),"|";};
    $write();
}
function captureNullable(?int $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureReference(int $value,mixed $next): void {
    $write=static function() use(&$value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureFresh(int $value): void {
    $write=static function() use($value): void {unset($value);$value=2.5;echo "N",gettype($value),"|";};
    $write();
}
function makeCapturedWriter(int $value): callable {
    return static function(mixed $next) use($value): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
}
captureInteger(1,2.5);captureNumeric(1,2.5);captureMixed(1,2.5);
captureNullable(null,2);captureNullable(null,2.5);captureReference(1,2.5);captureFresh(1);
$writer=makeCapturedWriter(1);$writer(2.5);
$bound=$writer->bindTo(null,null);$bound(2.5);
function captureNested(int $value,mixed $next): void {
    $outer=static function() use($value,$next): void {
        $inner=static function() use($value,$next): void {
            try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
            echo gettype($value),"|";
        };
        $inner();
    };
    $outer();
}
function captureString(string $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureArray(array $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureCallbackOriginal(): int {return 1;}
function captureCallbackReplacement(): int {return 2;}
function captureCallable(callable $value,mixed $next): void {
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();
}
function captureResource(mixed $next): void {
    $value=tmpfile();$owner=$value;
    $write=static function() use($value,$next): void {
        try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    };
    $write();fclose($owner);
}
class CaptureNominalFactory {
    static function make(self $value): callable {
        return static function(mixed $next) use($value): void {
            try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
            echo get_class($value),"|";
        };
    }
}
captureNested(1,2.5);captureString("kept",1);captureArray([1],1);captureArray([1],[2.5]);
captureCallable("captureCallbackOriginal","missingCapturedCallback");
captureCallable("captureCallbackOriginal","CAPTURECALLBACKREPLACEMENT");
captureResource(1);captureResource(false);
$nominalWrite=CaptureNominalFactory::make(new CaptureNominalFactory());$nominalWrite(new stdClass());
$nominalBound=$nominalWrite->bindTo(null,null);$nominalBound(new stdClass());
':'';
eval($code);
gc_collect_cycles();
