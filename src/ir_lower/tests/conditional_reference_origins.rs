//! Purpose:
//! Checks logical origins on incoming reference handles that detach on only one source path.
//!
//! Called from:
//! - Focused strict-local checking and native reference regressions.
//!
//! Key details:
//! - The caller's pointee is never copied into a by-value parameter shadow.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/conditional_reference_origins.php");
const CHECKED: &str = "Tinteger:2:2|2|Tstring:new:2|2|Winteger|3|Tstring|1|Tstring|string|Tstring|early|Winteger|integer|Tstring|string|";
const RAW_SOURCE: &str = r#"<?php
    extern "C" { function raw_reference_driver(): void; }
    function rawReferenceBoundary(bool $reset,int &$value,int $next):void {
        if($reset){unset($value);$value='new';}
        try{$value=$next;echo 'W';}catch(TypeError $error){echo 'T';}
        echo gettype($value),':',$value,'|';
    }
    function rawReferenceAfterJoin(bool $reset,int &$value,int $next):void {
        if($reset){unset($value);}
        $value=$next;echo gettype($value),':',$value,'|';
    }
    function rawReferenceString(bool $reset,string &$value):void {
        if($reset){unset($value);$value=2;}
        echo gettype($value),':',$value,'|';
    }
    function rawReferenceFloat(bool $reset,float &$value):void {
        if($reset){unset($value);$value='new';}
        echo gettype($value),':',$value,'|';
    }
    raw_reference_driver();
"#;

/// Each target preserves the native reference ABI while tracking an independent local binding.
#[test]
fn conditional_reference_origins_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for source in [SOURCE, RAW_SOURCE] {
            let module = super::semantic_sources::lower(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Attached writes reach the caller, detached writes keep their new origin and old aliases live.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_reference_origins_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Headerless host references keep their raw ABI until the callee detaches its local name.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_reference_origins_preserve_raw_host_arguments() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(RAW_SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let symbol = assembly.split_once("@fn name=rawReferenceBoundary symbol=").unwrap().1.lines().next().unwrap();
        let joined = assembly.split_once("@fn name=rawReferenceAfterJoin symbol=").unwrap().1.lines().next().unwrap();
        let text = assembly.split_once("@fn name=rawReferenceString symbol=").unwrap().1.lines().next().unwrap();
        let float = assembly.split_once("@fn name=rawReferenceFloat symbol=").unwrap().1.lines().next().unwrap();
        let caller = format!(r#"
            #include <stdint.h>
            #include <stdio.h>
            extern void raw_callback(int64_t,int64_t*,int64_t) __asm__("{symbol}");
            extern void joined_callback(int64_t,int64_t*,int64_t) __asm__("{joined}");
            typedef struct {{ const char *data; int64_t length; }} RawString;
            extern void text_callback(int64_t,RawString*) __asm__("{text}");
            extern void float_callback(int64_t,double*) __asm__("{float}");
            void raw_reference_driver(void) {{
                int64_t first=1; raw_callback(0,&first,2); printf("%lld|",(long long)first); fflush(stdout);
                int64_t second=1; raw_callback(1,&second,2); printf("%lld|",(long long)second); fflush(stdout);
                int64_t third=1; joined_callback(0,&third,2); printf("%lld|",(long long)third); fflush(stdout);
                int64_t fourth=1; joined_callback(1,&fourth,2); printf("%lld|",(long long)fourth); fflush(stdout);
                RawString text={{"old",3}}; text_callback(0,&text); printf("%.*s|",(int)text.length,text.data); fflush(stdout);
                text_callback(1,&text); printf("%.*s|",(int)text.length,text.data); fflush(stdout);
                double number=2.5; float_callback(0,&number); printf("%.1f|",number); fflush(stdout);
                float_callback(1,&number); printf("%.1f|",number); fflush(stdout);
            }}
        "#);
        let output = super::native_module::run_with_c_source(&module, &caller);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout),
            "Winteger:2|2|Tstring:new|1|integer:2|2|integer:2|1|string:old|old|integer:2|old|double:2.5|2.5|string:new|2.5|",
            "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Real runtime source preserves interpreted writeback and native reference origins together.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_reference_origins_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let native = r#"
        function opaqueReferenceEnd(bool $reset,int &$value,string $source,mixed $next):void {
            $old=function()use(&$value):mixed{return $value;};
            if($reset){unset($value);}
            try{eval($source);echo 'W';}catch(TypeError $error){echo 'T';}
            try{$value=$next;echo 'W';}catch(TypeError $error){echo 'T';}
            echo gettype($value),':',$value,':',$old(),'|';
        }
        $source=$argc>0?'$value="new";':'';
        $nativeFirst=1;opaqueReferenceEnd(false,$nativeFirst,$source,2);echo $nativeFirst,'|';
        $nativeSecond=1;opaqueReferenceEnd(true,$nativeSecond,$source,2);echo $nativeSecond,'|';
    "#;
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{CHECKED}TWinteger:2:2|2|WTstring:new:1|1|");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge, "exercise the actual interpreter bridge");
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Future binding ends cannot relax an incoming contract or an already established replacement.
#[test]
fn conditional_reference_origins_keep_known_invalid_store_diagnostics() {
    for source in [
        "<?php function invalidReference(bool $reset,int &$value):void {$value='bad';if($reset){unset($value);}}",
        "<?php function invalidReference(bool $reset,int &$value):void {if($reset){unset($value);$value='new';$value=2;}}",
        "<?php function invalidReference(bool $reset,int &$value):void {$value='bad';if($reset){unset($value);}eval('');}",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals"), "{error:?}");
    }
}
