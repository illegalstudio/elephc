//! Purpose:
//! Coverage test for the Gmagick-family API-surface throwing stubs.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every declared stub is called with type-default args (optional params
//!   omitted) inside a try/catch; the test asserts each throws a
//!   `*Exception("... not supported in elephc")`, proving the signature
//!   type-checks, is callable, and throws at runtime.

use crate::support::*;

/// Calls every Gmagick-family throwing stub and asserts each throws its
/// `*Exception("... not supported in elephc")`.
#[test]
fn test_gmagick_api_surface_all_stubs_throw() {
    let out = compile_and_run(
        r##"<?php
// --- Gmagick ---
function _cov_0() {
    $gm = new Gmagick();
    try { $gm->addnoiseimage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_1() {
    $gm = new Gmagick();
    $gmpx = new GmagickPixel();
    try { $gm->borderimage($gmpx, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_2() {
    $gm = new Gmagick();
    try { $gm->chopimage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_3() {
    $gm = new Gmagick();
    try { $gm->commentimage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_4() {
    $gm = new Gmagick();
    try { $gm->cropthumbnailimage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_5() {
    $gm = new Gmagick();
    try { $gm->cyclecolormapimage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_6() {
    $gm = new Gmagick();
    try { $gm->deconstructimages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_7() {
    $gm = new Gmagick();
    try { $gm->despeckleimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_8() {
    $gm = new Gmagick();
    try { $gm->edgeimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_9() {
    $gm = new Gmagick();
    try { $gm->enhanceimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_10() {
    $gm = new Gmagick();
    try { $gm->equalizeimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_11() {
    $gm = new Gmagick();
    $gmpx = new GmagickPixel();
    try { $gm->frameimage($gmpx, 1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_12() {
    $gm = new Gmagick();
    try { $gm->gammaimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_13() {
    $gm = new Gmagick();
    try { $gm->getfilename(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_14() {
    $gm = new Gmagick();
    try { $gm->getimagebackgroundcolor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_15() {
    $gm = new Gmagick();
    try { $gm->getimageblueprimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_16() {
    $gm = new Gmagick();
    try { $gm->getimagebordercolor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_17() {
    $gm = new Gmagick();
    try { $gm->getimagechanneldepth(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_18() {
    $gm = new Gmagick();
    try { $gm->getimagecolors(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_19() {
    $gm = new Gmagick();
    try { $gm->getimagecolorspace(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_20() {
    $gm = new Gmagick();
    try { $gm->getimagecompose(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_21() {
    $gm = new Gmagick();
    try { $gm->getimagedelay(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_22() {
    $gm = new Gmagick();
    try { $gm->getimagedepth(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_23() {
    $gm = new Gmagick();
    try { $gm->getimagedispose(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_24() {
    $gm = new Gmagick();
    try { $gm->getimageextrema(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_25() {
    $gm = new Gmagick();
    try { $gm->getimagefilename(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_26() {
    $gm = new Gmagick();
    try { $gm->getimagegamma(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_27() {
    $gm = new Gmagick();
    try { $gm->getimagegreenprimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_28() {
    $gm = new Gmagick();
    try { $gm->getimagehistogram(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_29() {
    $gm = new Gmagick();
    try { $gm->getimageinterlacescheme(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_30() {
    $gm = new Gmagick();
    try { $gm->getimageiterations(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_31() {
    $gm = new Gmagick();
    try { $gm->getimagematte(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_32() {
    $gm = new Gmagick();
    try { $gm->getimagemattecolor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_33() {
    $gm = new Gmagick();
    try { $gm->getimageprofile("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_34() {
    $gm = new Gmagick();
    try { $gm->getimageredprimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_35() {
    $gm = new Gmagick();
    try { $gm->getimagerenderingintent(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_36() {
    $gm = new Gmagick();
    try { $gm->getimageresolution(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_37() {
    $gm = new Gmagick();
    try { $gm->getimagescene(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_38() {
    $gm = new Gmagick();
    try { $gm->getimagesignature(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_39() {
    $gm = new Gmagick();
    try { $gm->getimagetype(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_40() {
    $gm = new Gmagick();
    try { $gm->getimageunits(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_41() {
    $gm = new Gmagick();
    try { $gm->getimagewhitepoint(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_42() {
    $gm = new Gmagick();
    try { $gm->getsamplingfactors(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_43() {
    $gm = new Gmagick();
    try { $gm->getsize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_44() {
    $gm = new Gmagick();
    try { $gm->getversion(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_45() {
    $gm = new Gmagick();
    try { $gm->implodeimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_46() {
    $gm = new Gmagick();
    try { $gm->labelimage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_47() {
    $gm = new Gmagick();
    try { $gm->levelimage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_48() {
    $gm = new Gmagick();
    try { $gm->magnifyimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_49() {
    $gm = new Gmagick();
    try { $gm->mapimage($gm, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_50() {
    $gm = new Gmagick();
    try { $gm->medianfilterimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_51() {
    $gm = new Gmagick();
    try { $gm->minifyimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_52() {
    $gm = new Gmagick();
    try { $gm->motionblurimage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_53() {
    $gm = new Gmagick();
    try { $gm->normalizeimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_54() {
    $gm = new Gmagick();
    try { $gm->profileimage("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_55() {
    $gm = new Gmagick();
    try { $gm->quantizeimage(1, 1, 1, false, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_56() {
    $gm = new Gmagick();
    try { $gm->quantizeimages(1, 1, 1, false, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_57() {
    $gm = new Gmagick();
    $gmdraw = new GmagickDraw();
    try { $gm->queryfontmetrics($gmdraw, "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_58() {
    $gm = new Gmagick();
    try { $gm->queryfonts(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_59() {
    $gm = new Gmagick();
    try { $gm->radialblurimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_60() {
    $gm = new Gmagick();
    try { $gm->raiseimage(1, 1, 1, 1, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_61() {
    $gm = new Gmagick();
    try { $gm->read("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_62() {
    $gm = new Gmagick();
    try { $gm->readimagefile(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_63() {
    $gm = new Gmagick();
    try { $gm->reducenoiseimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_64() {
    $gm = new Gmagick();
    try { $gm->removeimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_65() {
    $gm = new Gmagick();
    try { $gm->removeimageprofile("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_66() {
    $gm = new Gmagick();
    try { $gm->resampleimage(1.0, 1.0, 1, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_67() {
    $gm = new Gmagick();
    try { $gm->rollimage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_68() {
    $gm = new Gmagick();
    try { $gm->separateimagechannel(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_69() {
    $gm = new Gmagick();
    try { $gm->setfilename("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_70() {
    $gm = new Gmagick();
    try { $gm->setimageblueprimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_71() {
    $gm = new Gmagick();
    $gmpx = new GmagickPixel();
    try { $gm->setimagebordercolor($gmpx); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_72() {
    $gm = new Gmagick();
    try { $gm->setimagechanneldepth(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_73() {
    $gm = new Gmagick();
    try { $gm->setimagecolorspace(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_74() {
    $gm = new Gmagick();
    try { $gm->setimagecompose(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_75() {
    $gm = new Gmagick();
    try { $gm->setimagedelay(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_76() {
    $gm = new Gmagick();
    try { $gm->setimagedepth(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_77() {
    $gm = new Gmagick();
    try { $gm->setimagedispose(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_78() {
    $gm = new Gmagick();
    try { $gm->setimagefilename("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_79() {
    $gm = new Gmagick();
    try { $gm->setimagegamma(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_80() {
    $gm = new Gmagick();
    try { $gm->setimagegreenprimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_81() {
    $gm = new Gmagick();
    try { $gm->setimageinterlacescheme(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_82() {
    $gm = new Gmagick();
    try { $gm->setimageiterations(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_83() {
    $gm = new Gmagick();
    try { $gm->setimageprofile("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_84() {
    $gm = new Gmagick();
    try { $gm->setimageredprimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_85() {
    $gm = new Gmagick();
    try { $gm->setimagerenderingintent(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_86() {
    $gm = new Gmagick();
    try { $gm->setimageresolution(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_87() {
    $gm = new Gmagick();
    try { $gm->setimagescene(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_88() {
    $gm = new Gmagick();
    try { $gm->setimagetype(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_89() {
    $gm = new Gmagick();
    try { $gm->setimageunits(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_90() {
    $gm = new Gmagick();
    try { $gm->setimagewhitepoint(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_91() {
    $gm = new Gmagick();
    try { $gm->setsamplingfactors([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_92() {
    $gm = new Gmagick();
    try { $gm->setsize(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_93() {
    $gm = new Gmagick();
    try { $gm->shearimage(null, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_94() {
    $gm = new Gmagick();
    try { $gm->solarizeimage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_95() {
    $gm = new Gmagick();
    try { $gm->spreadimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_96() {
    $gm = new Gmagick();
    try { $gm->stripimage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_97() {
    $gm = new Gmagick();
    try { $gm->trimimage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- GmagickDraw ---
function _cov_98() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->arc(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_99() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->bezier([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_100() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfillcolor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_101() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfillopacity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_102() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfont(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_103() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfontsize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_104() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfontstyle(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_105() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getfontweight(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_106() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getstrokecolor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_107() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getstrokeopacity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_108() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->getstrokewidth(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_109() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->gettextdecoration(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_110() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->gettextencoding(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_111() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->polyline([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_112() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->rotate(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_113() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->roundrectangle(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_114() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->scale(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_115() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setfillopacity(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_116() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setfont("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_117() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setfontsize(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_118() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setfontstyle(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_119() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setfontweight(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_120() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->setstrokeopacity(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_121() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->settextdecoration(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_122() {
    $gmdraw = new GmagickDraw();
    try { $gmdraw->settextencoding("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- GmagickPixel ---
function _cov_123() {
    $gmpx = new GmagickPixel();
    try { $gmpx->getcolorcount(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
$n = 0;
$n += _cov_0();
$n += _cov_1();
$n += _cov_2();
$n += _cov_3();
$n += _cov_4();
$n += _cov_5();
$n += _cov_6();
$n += _cov_7();
$n += _cov_8();
$n += _cov_9();
$n += _cov_10();
$n += _cov_11();
$n += _cov_12();
$n += _cov_13();
$n += _cov_14();
$n += _cov_15();
$n += _cov_16();
$n += _cov_17();
$n += _cov_18();
$n += _cov_19();
$n += _cov_20();
$n += _cov_21();
$n += _cov_22();
$n += _cov_23();
$n += _cov_24();
$n += _cov_25();
$n += _cov_26();
$n += _cov_27();
$n += _cov_28();
$n += _cov_29();
$n += _cov_30();
$n += _cov_31();
$n += _cov_32();
$n += _cov_33();
$n += _cov_34();
$n += _cov_35();
$n += _cov_36();
$n += _cov_37();
$n += _cov_38();
$n += _cov_39();
$n += _cov_40();
$n += _cov_41();
$n += _cov_42();
$n += _cov_43();
$n += _cov_44();
$n += _cov_45();
$n += _cov_46();
$n += _cov_47();
$n += _cov_48();
$n += _cov_49();
$n += _cov_50();
$n += _cov_51();
$n += _cov_52();
$n += _cov_53();
$n += _cov_54();
$n += _cov_55();
$n += _cov_56();
$n += _cov_57();
$n += _cov_58();
$n += _cov_59();
$n += _cov_60();
$n += _cov_61();
$n += _cov_62();
$n += _cov_63();
$n += _cov_64();
$n += _cov_65();
$n += _cov_66();
$n += _cov_67();
$n += _cov_68();
$n += _cov_69();
$n += _cov_70();
$n += _cov_71();
$n += _cov_72();
$n += _cov_73();
$n += _cov_74();
$n += _cov_75();
$n += _cov_76();
$n += _cov_77();
$n += _cov_78();
$n += _cov_79();
$n += _cov_80();
$n += _cov_81();
$n += _cov_82();
$n += _cov_83();
$n += _cov_84();
$n += _cov_85();
$n += _cov_86();
$n += _cov_87();
$n += _cov_88();
$n += _cov_89();
$n += _cov_90();
$n += _cov_91();
$n += _cov_92();
$n += _cov_93();
$n += _cov_94();
$n += _cov_95();
$n += _cov_96();
$n += _cov_97();
$n += _cov_98();
$n += _cov_99();
$n += _cov_100();
$n += _cov_101();
$n += _cov_102();
$n += _cov_103();
$n += _cov_104();
$n += _cov_105();
$n += _cov_106();
$n += _cov_107();
$n += _cov_108();
$n += _cov_109();
$n += _cov_110();
$n += _cov_111();
$n += _cov_112();
$n += _cov_113();
$n += _cov_114();
$n += _cov_115();
$n += _cov_116();
$n += _cov_117();
$n += _cov_118();
$n += _cov_119();
$n += _cov_120();
$n += _cov_121();
$n += _cov_122();
$n += _cov_123();
echo $n . "/" . 124;
"##,
    );
    assert_eq!(out, "124/124");
}
