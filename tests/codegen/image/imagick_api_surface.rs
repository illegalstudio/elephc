//! Purpose:
//! Coverage test for the Imagick-family API-surface throwing stubs.
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

/// Calls every Imagick-family throwing stub and asserts each throws its
/// `*Exception("... not supported in elephc")`.
#[test]
fn test_imagick_api_surface_all_stubs_throw() {
    let out = compile_and_run(
        r##"<?php
// --- Imagick ---
function _cov_0() {
    $im = new Imagick();
    try { $im->adaptiveBlurImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_1() {
    $im = new Imagick();
    try { $im->adaptiveResizeImage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_2() {
    $im = new Imagick();
    try { $im->adaptiveSharpenImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_3() {
    $im = new Imagick();
    try { $im->adaptiveThresholdImage(1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_4() {
    $im = new Imagick();
    try { $im->addNoiseImage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_5() {
    $im = new Imagick();
    $draw = new ImagickDraw();
    try { $im->affineTransformImage($draw); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_6() {
    $im = new Imagick();
    try { $im->animateImages("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_7() {
    $im = new Imagick();
    try { $im->appendImages(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_8() {
    $im = new Imagick();
    try { $im->autoLevelImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_9() {
    $im = new Imagick();
    try { $im->averageImages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_10() {
    $im = new Imagick();
    try { $im->blackThresholdImage(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_11() {
    $im = new Imagick();
    try { $im->blueShiftImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_12() {
    $im = new Imagick();
    try { $im->borderImage(null, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_13() {
    $im = new Imagick();
    try { $im->brightnessContrastImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_14() {
    $im = new Imagick();
    try { $im->charcoalImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_15() {
    $im = new Imagick();
    try { $im->chopImage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_16() {
    $im = new Imagick();
    try { $im->clampImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_17() {
    $im = new Imagick();
    try { $im->clipImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_18() {
    $im = new Imagick();
    try { $im->clipImagePath("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_19() {
    $im = new Imagick();
    try { $im->clipPathImage("x", false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_20() {
    $im = new Imagick();
    try { $im->clone(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_21() {
    $im = new Imagick();
    try { $im->clutImage($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_22() {
    $im = new Imagick();
    try { $im->coalesceImages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_23() {
    $im = new Imagick();
    try { $im->colorFloodfillImage(null, 1.0, null, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_24() {
    $im = new Imagick();
    try { $im->colorizeImage(null, null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_25() {
    $im = new Imagick();
    try { $im->colorMatrixImage([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_26() {
    $im = new Imagick();
    try { $im->combineImages(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_27() {
    $im = new Imagick();
    try { $im->commentImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_28() {
    $im = new Imagick();
    try { $im->compareImageChannels($im, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_29() {
    $im = new Imagick();
    try { $im->compareImageLayers(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_30() {
    $im = new Imagick();
    try { $im->compareImages($im, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_31() {
    $im = new Imagick();
    try { $im->contrastImage(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_32() {
    $im = new Imagick();
    try { $im->contrastStretchImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_33() {
    $im = new Imagick();
    try { $im->cropThumbnailImage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_34() {
    $im = new Imagick();
    try { $im->cycleColormapImage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_35() {
    $im = new Imagick();
    try { $im->decipherImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_36() {
    $im = new Imagick();
    try { $im->deconstructImages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_37() {
    $im = new Imagick();
    try { $im->deleteImageArtifact("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_38() {
    $im = new Imagick();
    try { $im->deleteImageProperty("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_39() {
    $im = new Imagick();
    try { $im->deskewImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_40() {
    $im = new Imagick();
    try { $im->despeckleImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_41() {
    $im = new Imagick();
    try { $im->displayImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_42() {
    $im = new Imagick();
    try { $im->displayImages("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_43() {
    $im = new Imagick();
    try { $im->edgeImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_44() {
    $im = new Imagick();
    try { $im->embossImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_45() {
    $im = new Imagick();
    try { $im->encipherImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_46() {
    $im = new Imagick();
    try { $im->enhanceImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_47() {
    $im = new Imagick();
    try { $im->equalizeImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_48() {
    $im = new Imagick();
    try { $im->evaluateImage(1, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_49() {
    $im = new Imagick();
    try { $im->exportImagePixels(1, 1, 1, 1, "x", 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_50() {
    $im = new Imagick();
    try { $im->extentImage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_51() {
    $im = new Imagick();
    $kern = new ImagickKernel();
    try { $im->filter($kern); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_52() {
    $im = new Imagick();
    try { $im->flattenImages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_53() {
    $im = new Imagick();
    try { $im->floodFillPaintImage(null, 1.0, null, 1, 1, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_54() {
    $im = new Imagick();
    try { $im->forwardFourierTransformimage(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_55() {
    $im = new Imagick();
    try { $im->frameImage(null, 1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_56() {
    $im = new Imagick();
    try { $im->functionImage(1, []); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_57() {
    $im = new Imagick();
    try { $im->gammaImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_58() {
    $im = new Imagick();
    try { $im->getColorspace(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_59() {
    $im = new Imagick();
    try { $im->getCompression(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_60() {
    $im = new Imagick();
    try { $im->getCompressionQuality(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_61() {
    try { Imagick::getCopyright(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_62() {
    $im = new Imagick();
    try { $im->getFilename(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_63() {
    $im = new Imagick();
    try { $im->getFont(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_64() {
    $im = new Imagick();
    try { $im->getGravity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_65() {
    try { Imagick::getHomeURL(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_66() {
    $im = new Imagick();
    try { $im->getImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_67() {
    $im = new Imagick();
    try { $im->getImageAlphaChannel(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_68() {
    $im = new Imagick();
    try { $im->getImageArtifact("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_69() {
    $im = new Imagick();
    try { $im->getImageAttribute("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_70() {
    $im = new Imagick();
    try { $im->getImageBackgroundColor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_71() {
    $im = new Imagick();
    try { $im->getImageBluePrimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_72() {
    $im = new Imagick();
    try { $im->getImageBorderColor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_73() {
    $im = new Imagick();
    try { $im->getImageChannelDepth(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_74() {
    $im = new Imagick();
    try { $im->getImageChannelDistortion($im, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_75() {
    $im = new Imagick();
    try { $im->getImageChannelDistortions($im, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_76() {
    $im = new Imagick();
    try { $im->getImageChannelExtrema(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_77() {
    $im = new Imagick();
    try { $im->getImageChannelKurtosis(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_78() {
    $im = new Imagick();
    try { $im->getImageChannelMean(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_79() {
    $im = new Imagick();
    try { $im->getImageChannelRange(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_80() {
    $im = new Imagick();
    try { $im->getImageChannelStatistics(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_81() {
    $im = new Imagick();
    try { $im->getImageClipMask(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_82() {
    $im = new Imagick();
    try { $im->getImageColormapColor(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_83() {
    $im = new Imagick();
    try { $im->getImageColors(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_84() {
    $im = new Imagick();
    try { $im->getImageColorspace(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_85() {
    $im = new Imagick();
    try { $im->getImageCompose(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_86() {
    $im = new Imagick();
    try { $im->getImageCompression(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_87() {
    $im = new Imagick();
    try { $im->getImageDelay(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_88() {
    $im = new Imagick();
    try { $im->getImageDepth(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_89() {
    $im = new Imagick();
    try { $im->getImageDispose(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_90() {
    $im = new Imagick();
    try { $im->getImageDistortion($im, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_91() {
    $im = new Imagick();
    try { $im->getImageExtrema(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_92() {
    $im = new Imagick();
    try { $im->getImageFilename(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_93() {
    $im = new Imagick();
    try { $im->getImageGamma(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_94() {
    $im = new Imagick();
    try { $im->getImageGravity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_95() {
    $im = new Imagick();
    try { $im->getImageGreenPrimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_96() {
    $im = new Imagick();
    try { $im->getImageHistogram(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_97() {
    $im = new Imagick();
    try { $im->getImageInterlaceScheme(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_98() {
    $im = new Imagick();
    try { $im->getImageInterpolateMethod(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_99() {
    $im = new Imagick();
    try { $im->getImageIterations(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_100() {
    $im = new Imagick();
    try { $im->getImageLength(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_101() {
    $im = new Imagick();
    try { $im->getImageMatte(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_102() {
    $im = new Imagick();
    try { $im->getImageMatteColor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_103() {
    $im = new Imagick();
    try { $im->getImageMimeType(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_104() {
    $im = new Imagick();
    try { $im->getImageOrientation(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_105() {
    $im = new Imagick();
    try { $im->getImagePage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_106() {
    $im = new Imagick();
    try { $im->getImageProfile("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_107() {
    $im = new Imagick();
    try { $im->getImageProfiles(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_108() {
    $im = new Imagick();
    try { $im->getImageProperties(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_109() {
    $im = new Imagick();
    try { $im->getImageProperty("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_110() {
    $im = new Imagick();
    try { $im->getImageRedPrimary(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_111() {
    $im = new Imagick();
    try { $im->getImageRegion(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_112() {
    $im = new Imagick();
    try { $im->getImageRenderingIntent(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_113() {
    $im = new Imagick();
    try { $im->getImageResolution(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_114() {
    $im = new Imagick();
    try { $im->getImageScene(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_115() {
    $im = new Imagick();
    try { $im->getImageSignature(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_116() {
    $im = new Imagick();
    try { $im->getImageSize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_117() {
    $im = new Imagick();
    try { $im->getImageTicksPerSecond(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_118() {
    $im = new Imagick();
    try { $im->getImageTotalInkDensity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_119() {
    $im = new Imagick();
    try { $im->getImageType(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_120() {
    $im = new Imagick();
    try { $im->getImageUnits(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_121() {
    $im = new Imagick();
    try { $im->getImageVirtualPixelMethod(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_122() {
    $im = new Imagick();
    try { $im->getImageWhitePoint(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_123() {
    $im = new Imagick();
    try { $im->getInterlaceScheme(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_124() {
    $im = new Imagick();
    try { $im->getOption("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_125() {
    try { Imagick::getPackageName(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_126() {
    $im = new Imagick();
    try { $im->getPage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_127() {
    $im = new Imagick();
    try { $im->getPixelRegionIterator(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_128() {
    $im = new Imagick();
    try { $im->getPointSize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_129() {
    try { Imagick::getQuantum(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_130() {
    try { Imagick::getQuantumDepth(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_131() {
    try { Imagick::getQuantumRange(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_132() {
    try { Imagick::getRegistry("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_133() {
    try { Imagick::getReleaseDate(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_134() {
    try { Imagick::getResource(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_135() {
    try { Imagick::getResourceLimit(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_136() {
    $im = new Imagick();
    try { $im->getSamplingFactors(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_137() {
    $im = new Imagick();
    try { $im->getSize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_138() {
    $im = new Imagick();
    try { $im->getSizeOffset(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_139() {
    try { Imagick::getVersion(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_140() {
    $im = new Imagick();
    try { $im->haldClutImage($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_141() {
    $im = new Imagick();
    try { $im->hasNextImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_142() {
    $im = new Imagick();
    try { $im->hasPreviousImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_143() {
    $im = new Imagick();
    try { $im->identifyFormat("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_144() {
    $im = new Imagick();
    try { $im->identifyImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_145() {
    $im = new Imagick();
    try { $im->implodeImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_146() {
    $im = new Imagick();
    try { $im->importImagePixels(1, 1, 1, 1, "x", 1, []); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_147() {
    $im = new Imagick();
    try { $im->inverseFourierTransformImage($im, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_148() {
    $im = new Imagick();
    try { $im->labelImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_149() {
    $im = new Imagick();
    try { $im->levelImage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_150() {
    $im = new Imagick();
    try { $im->linearStretchImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_151() {
    try { Imagick::listRegistry(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_152() {
    $im = new Imagick();
    try { $im->magnifyImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_153() {
    $im = new Imagick();
    try { $im->mapImage($im, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_154() {
    $im = new Imagick();
    try { $im->matteFloodfillImage(1.0, 1.0, null, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_155() {
    $im = new Imagick();
    try { $im->medianFilterImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_156() {
    $im = new Imagick();
    try { $im->mergeImageLayers(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_157() {
    $im = new Imagick();
    try { $im->minifyImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_158() {
    $im = new Imagick();
    $draw = new ImagickDraw();
    try { $im->montageImage($draw, "x", "x", 1, "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_159() {
    $im = new Imagick();
    try { $im->morphImages(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_160() {
    $im = new Imagick();
    $kern = new ImagickKernel();
    try { $im->morphology(1, 1, $kern); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_161() {
    $im = new Imagick();
    try { $im->mosaicImages(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_162() {
    $im = new Imagick();
    try { $im->motionBlurImage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_163() {
    $im = new Imagick();
    try { $im->newPseudoImage(1, 1, "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_164() {
    $im = new Imagick();
    try { $im->normalizeImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_165() {
    $im = new Imagick();
    try { $im->oilPaintImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_166() {
    $im = new Imagick();
    try { $im->opaquePaintImage(null, null, 1.0, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_167() {
    $im = new Imagick();
    try { $im->optimizeImageLayers(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_168() {
    $im = new Imagick();
    try { $im->orderedPosterizeImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_169() {
    $im = new Imagick();
    try { $im->paintFloodfillImage(null, 1.0, null, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_170() {
    $im = new Imagick();
    try { $im->paintOpaqueImage(null, null, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_171() {
    $im = new Imagick();
    try { $im->paintTransparentImage(null, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_172() {
    $im = new Imagick();
    try { $im->pingImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_173() {
    $im = new Imagick();
    try { $im->pingImageBlob("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_174() {
    $im = new Imagick();
    try { $im->pingImageFile(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_175() {
    $im = new Imagick();
    $draw = new ImagickDraw();
    try { $im->polaroidImage($draw, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_176() {
    $im = new Imagick();
    try { $im->posterizeImage(1, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_177() {
    $im = new Imagick();
    try { $im->previewImages(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_178() {
    $im = new Imagick();
    try { $im->profileImage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_179() {
    $im = new Imagick();
    try { $im->quantizeImage(1, 1, 1, false, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_180() {
    $im = new Imagick();
    try { $im->quantizeImages(1, 1, 1, false, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_181() {
    $im = new Imagick();
    $draw = new ImagickDraw();
    try { $im->queryFontMetrics($draw, "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_182() {
    try { Imagick::queryFonts(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_183() {
    $im = new Imagick();
    try { $im->radialBlurImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_184() {
    $im = new Imagick();
    try { $im->raiseImage(1, 1, 1, 1, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_185() {
    $im = new Imagick();
    try { $im->randomThresholdImage(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_186() {
    $im = new Imagick();
    try { $im->readImageFile(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_187() {
    $im = new Imagick();
    try { $im->readImages([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_188() {
    $im = new Imagick();
    try { $im->recolorImage([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_189() {
    $im = new Imagick();
    try { $im->reduceNoiseImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_190() {
    $im = new Imagick();
    try { $im->remapImage($im, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_191() {
    $im = new Imagick();
    try { $im->removeImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_192() {
    $im = new Imagick();
    try { $im->removeImageProfile("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_193() {
    $im = new Imagick();
    try { $im->render(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_194() {
    $im = new Imagick();
    try { $im->resampleImage(1.0, 1.0, 1, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_195() {
    $im = new Imagick();
    try { $im->resetImagePage("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_196() {
    $im = new Imagick();
    try { $im->rollImage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_197() {
    $im = new Imagick();
    try { $im->rotationalBlurImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_198() {
    $im = new Imagick();
    try { $im->roundCorners(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_199() {
    $im = new Imagick();
    try { $im->sampleImage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_200() {
    $im = new Imagick();
    try { $im->segmentImage(1, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_201() {
    $im = new Imagick();
    try { $im->selectiveBlurImage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_202() {
    $im = new Imagick();
    try { $im->separateImageChannel(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_203() {
    $im = new Imagick();
    try { $im->sepiaToneImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_204() {
    $im = new Imagick();
    try { $im->setBackgroundColor(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_205() {
    $im = new Imagick();
    try { $im->setColorspace(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_206() {
    $im = new Imagick();
    try { $im->setCompression(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_207() {
    $im = new Imagick();
    try { $im->setFilename("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_208() {
    $im = new Imagick();
    try { $im->setFont("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_209() {
    $im = new Imagick();
    try { $im->setGravity(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_210() {
    $im = new Imagick();
    try { $im->setImage($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_211() {
    $im = new Imagick();
    try { $im->setImageAlphaChannel(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_212() {
    $im = new Imagick();
    try { $im->setImageArtifact("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_213() {
    $im = new Imagick();
    try { $im->setImageAttribute("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_214() {
    $im = new Imagick();
    try { $im->setImageBias(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_215() {
    $im = new Imagick();
    try { $im->setImageBiasQuantum(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_216() {
    $im = new Imagick();
    try { $im->setImageBluePrimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_217() {
    $im = new Imagick();
    try { $im->setImageBorderColor(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_218() {
    $im = new Imagick();
    try { $im->setImageChannelDepth(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_219() {
    $im = new Imagick();
    try { $im->setImageClipMask($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_220() {
    $im = new Imagick();
    $px = new ImagickPixel();
    try { $im->setImageColormapColor(1, $px); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_221() {
    $im = new Imagick();
    try { $im->setImageColorspace(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_222() {
    $im = new Imagick();
    try { $im->setImageCompose(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_223() {
    $im = new Imagick();
    try { $im->setImageCompression(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_224() {
    $im = new Imagick();
    try { $im->setImageDelay(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_225() {
    $im = new Imagick();
    try { $im->setImageDepth(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_226() {
    $im = new Imagick();
    try { $im->setImageDispose(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_227() {
    $im = new Imagick();
    try { $im->setImageExtent(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_228() {
    $im = new Imagick();
    try { $im->setImageFilename("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_229() {
    $im = new Imagick();
    try { $im->setImageGamma(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_230() {
    $im = new Imagick();
    try { $im->setImageGravity(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_231() {
    $im = new Imagick();
    try { $im->setImageGreenPrimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_232() {
    $im = new Imagick();
    try { $im->setImageInterlaceScheme(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_233() {
    $im = new Imagick();
    try { $im->setImageInterpolateMethod(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_234() {
    $im = new Imagick();
    try { $im->setImageIterations(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_235() {
    $im = new Imagick();
    try { $im->setImageMatte(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_236() {
    $im = new Imagick();
    try { $im->setImageMatteColor(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_237() {
    $im = new Imagick();
    try { $im->setImageOpacity(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_238() {
    $im = new Imagick();
    try { $im->setImageOrientation(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_239() {
    $im = new Imagick();
    try { $im->setImagePage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_240() {
    $im = new Imagick();
    try { $im->setImageProfile("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_241() {
    $im = new Imagick();
    try { $im->setImageProperty("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_242() {
    $im = new Imagick();
    try { $im->setImageRedPrimary(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_243() {
    $im = new Imagick();
    try { $im->setImageRenderingIntent(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_244() {
    $im = new Imagick();
    try { $im->setImageResolution(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_245() {
    $im = new Imagick();
    try { $im->setImageScene(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_246() {
    $im = new Imagick();
    try { $im->setImageTicksPerSecond(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_247() {
    $im = new Imagick();
    try { $im->setImageType(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_248() {
    $im = new Imagick();
    try { $im->setImageUnits(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_249() {
    $im = new Imagick();
    try { $im->setImageVirtualPixelMethod(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_250() {
    $im = new Imagick();
    try { $im->setImageWhitePoint(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_251() {
    $im = new Imagick();
    try { $im->setInterlaceScheme(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_252() {
    $im = new Imagick();
    try { $im->setOption("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_253() {
    $im = new Imagick();
    try { $im->setPage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_254() {
    $im = new Imagick();
    try { $im->setPointSize(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_255() {
    $im = new Imagick();
    try { $im->setProgressMonitor(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_256() {
    try { Imagick::setRegistry("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_257() {
    $im = new Imagick();
    try { $im->setResolution(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_258() {
    try { Imagick::setResourceLimit(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_259() {
    $im = new Imagick();
    try { $im->setSamplingFactors([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_260() {
    $im = new Imagick();
    try { $im->setSize(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_261() {
    $im = new Imagick();
    try { $im->setSizeOffset(1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_262() {
    $im = new Imagick();
    try { $im->setType(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_263() {
    $im = new Imagick();
    try { $im->shadeImage(false, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_264() {
    $im = new Imagick();
    try { $im->shadowImage(1.0, 1.0, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_265() {
    $im = new Imagick();
    try { $im->shaveImage(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_266() {
    $im = new Imagick();
    try { $im->shearImage(null, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_267() {
    $im = new Imagick();
    try { $im->sigmoidalContrastImage(false, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_268() {
    $im = new Imagick();
    try { $im->sketchImage(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_269() {
    $im = new Imagick();
    try { $im->smushImages(false, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_270() {
    $im = new Imagick();
    try { $im->solarizeImage(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_271() {
    $im = new Imagick();
    try { $im->sparseColorImage(1, []); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_272() {
    $im = new Imagick();
    try { $im->spliceImage(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_273() {
    $im = new Imagick();
    try { $im->spreadImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_274() {
    $im = new Imagick();
    try { $im->statisticImage(1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_275() {
    $im = new Imagick();
    try { $im->steganoImage($im, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_276() {
    $im = new Imagick();
    try { $im->stereoImage($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_277() {
    $im = new Imagick();
    try { $im->stripImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_278() {
    $im = new Imagick();
    $refa = [];
    $refn = 0.0;
    try { $im->subImageMatch($im, $refa, $refn); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_279() {
    $im = new Imagick();
    try { $im->thresholdImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_280() {
    $im = new Imagick();
    try { $im->tintImage(null, null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_281() {
    $im = new Imagick();
    try { $im->__toString(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_282() {
    $im = new Imagick();
    try { $im->transformImage("x", "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_283() {
    $im = new Imagick();
    try { $im->transformImageColorspace(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_284() {
    $im = new Imagick();
    try { $im->transparentPaintImage(null, 1.0, 1.0, false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_285() {
    $im = new Imagick();
    try { $im->transposeImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_286() {
    $im = new Imagick();
    try { $im->transverseImage(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_287() {
    $im = new Imagick();
    try { $im->trimImage(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_288() {
    $im = new Imagick();
    try { $im->uniqueImageColors(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_289() {
    $im = new Imagick();
    try { $im->unsharpMaskImage(1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_290() {
    $im = new Imagick();
    try { $im->vignetteImage(1.0, 1.0, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_291() {
    $im = new Imagick();
    try { $im->whiteThresholdImage(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_292() {
    $im = new Imagick();
    try { $im->writeImageFile(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_293() {
    $im = new Imagick();
    try { $im->writeImagesFile(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- ImagickDraw ---
function _cov_294() {
    $draw = new ImagickDraw();
    try { $draw->affine([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_295() {
    $draw = new ImagickDraw();
    try { $draw->annotation(1.0, 1.0, "x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_296() {
    $draw = new ImagickDraw();
    try { $draw->arc(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_297() {
    $draw = new ImagickDraw();
    try { $draw->bezier([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_298() {
    $draw = new ImagickDraw();
    try { $draw->clone(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_299() {
    $draw = new ImagickDraw();
    try { $draw->color(1.0, 1.0, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_300() {
    $draw = new ImagickDraw();
    try { $draw->comment("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_301() {
    $im = new Imagick();
    $draw = new ImagickDraw();
    try { $draw->composite(1, 1.0, 1.0, 1.0, 1.0, $im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_302() {
    $draw = new ImagickDraw();
    try { $draw->getClipPath(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_303() {
    $draw = new ImagickDraw();
    try { $draw->getClipRule(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_304() {
    $draw = new ImagickDraw();
    try { $draw->getClipUnits(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_305() {
    $draw = new ImagickDraw();
    try { $draw->getFillOpacity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_306() {
    $draw = new ImagickDraw();
    try { $draw->getFillRule(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_307() {
    $draw = new ImagickDraw();
    try { $draw->getFont(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_308() {
    $draw = new ImagickDraw();
    try { $draw->getFontFamily(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_309() {
    $draw = new ImagickDraw();
    try { $draw->getFontSize(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_310() {
    $draw = new ImagickDraw();
    try { $draw->getFontStretch(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_311() {
    $draw = new ImagickDraw();
    try { $draw->getFontStyle(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_312() {
    $draw = new ImagickDraw();
    try { $draw->getFontWeight(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_313() {
    $draw = new ImagickDraw();
    try { $draw->getGravity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_314() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeAntialias(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_315() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeColor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_316() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeDashArray(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_317() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeDashOffset(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_318() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeLineCap(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_319() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeLineJoin(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_320() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeMiterLimit(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_321() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeOpacity(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_322() {
    $draw = new ImagickDraw();
    try { $draw->getStrokeWidth(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_323() {
    $draw = new ImagickDraw();
    try { $draw->getTextAlignment(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_324() {
    $draw = new ImagickDraw();
    try { $draw->getTextAntialias(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_325() {
    $draw = new ImagickDraw();
    try { $draw->getTextDecoration(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_326() {
    $draw = new ImagickDraw();
    try { $draw->getTextEncoding(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_327() {
    $draw = new ImagickDraw();
    try { $draw->getTextInterlineSpacing(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_328() {
    $draw = new ImagickDraw();
    try { $draw->getTextInterwordSpacing(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_329() {
    $draw = new ImagickDraw();
    try { $draw->getTextKerning(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_330() {
    $draw = new ImagickDraw();
    try { $draw->getTextUnderColor(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_331() {
    $draw = new ImagickDraw();
    try { $draw->getVectorGraphics(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_332() {
    $draw = new ImagickDraw();
    try { $draw->matte(1.0, 1.0, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_333() {
    $draw = new ImagickDraw();
    try { $draw->pathClose(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_334() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToAbsolute(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_335() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToQuadraticBezierAbsolute(1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_336() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToQuadraticBezierRelative(1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_337() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToQuadraticBezierSmoothAbsolute(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_338() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToQuadraticBezierSmoothRelative(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_339() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToRelative(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_340() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToSmoothAbsolute(1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_341() {
    $draw = new ImagickDraw();
    try { $draw->pathCurveToSmoothRelative(1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_342() {
    $draw = new ImagickDraw();
    try { $draw->pathEllipticArcAbsolute(1.0, 1.0, 1.0, false, false, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_343() {
    $draw = new ImagickDraw();
    try { $draw->pathEllipticArcRelative(1.0, 1.0, 1.0, false, false, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_344() {
    $draw = new ImagickDraw();
    try { $draw->pathFinish(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_345() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToAbsolute(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_346() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToHorizontalAbsolute(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_347() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToHorizontalRelative(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_348() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToRelative(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_349() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToVerticalAbsolute(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_350() {
    $draw = new ImagickDraw();
    try { $draw->pathLineToVerticalRelative(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_351() {
    $draw = new ImagickDraw();
    try { $draw->pathMoveToAbsolute(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_352() {
    $draw = new ImagickDraw();
    try { $draw->pathMoveToRelative(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_353() {
    $draw = new ImagickDraw();
    try { $draw->pathStart(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_354() {
    $draw = new ImagickDraw();
    try { $draw->polyline([]); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_355() {
    $draw = new ImagickDraw();
    try { $draw->pop(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_356() {
    $draw = new ImagickDraw();
    try { $draw->popClipPath(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_357() {
    $draw = new ImagickDraw();
    try { $draw->popDefs(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_358() {
    $draw = new ImagickDraw();
    try { $draw->popPattern(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_359() {
    $draw = new ImagickDraw();
    try { $draw->push(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_360() {
    $draw = new ImagickDraw();
    try { $draw->pushClipPath("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_361() {
    $draw = new ImagickDraw();
    try { $draw->pushDefs(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_362() {
    $draw = new ImagickDraw();
    try { $draw->pushPattern("x", 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_363() {
    $draw = new ImagickDraw();
    try { $draw->render(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_364() {
    $draw = new ImagickDraw();
    try { $draw->resetVectorGraphics(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_365() {
    $draw = new ImagickDraw();
    try { $draw->rotate(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_366() {
    $draw = new ImagickDraw();
    try { $draw->roundRectangle(1.0, 1.0, 1.0, 1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_367() {
    $draw = new ImagickDraw();
    try { $draw->scale(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_368() {
    $draw = new ImagickDraw();
    try { $draw->setClipPath("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_369() {
    $draw = new ImagickDraw();
    try { $draw->setClipRule(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_370() {
    $draw = new ImagickDraw();
    try { $draw->setClipUnits(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_371() {
    $draw = new ImagickDraw();
    try { $draw->setFillAlpha(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_372() {
    $draw = new ImagickDraw();
    try { $draw->setFillOpacity(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_373() {
    $draw = new ImagickDraw();
    try { $draw->setFillPatternURL("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_374() {
    $draw = new ImagickDraw();
    try { $draw->setFillRule(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_375() {
    $draw = new ImagickDraw();
    try { $draw->setFont("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_376() {
    $draw = new ImagickDraw();
    try { $draw->setFontFamily("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_377() {
    $draw = new ImagickDraw();
    try { $draw->setFontSize(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_378() {
    $draw = new ImagickDraw();
    try { $draw->setFontStretch(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_379() {
    $draw = new ImagickDraw();
    try { $draw->setFontStyle(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_380() {
    $draw = new ImagickDraw();
    try { $draw->setFontWeight(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_381() {
    $draw = new ImagickDraw();
    try { $draw->setGravity(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_382() {
    $draw = new ImagickDraw();
    try { $draw->setResolution(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_383() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeAlpha(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_384() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeAntialias(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_385() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeDashArray(null); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_386() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeDashOffset(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_387() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeLineCap(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_388() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeLineJoin(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_389() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeMiterLimit(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_390() {
    $draw = new ImagickDraw();
    try { $draw->setStrokeOpacity(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_391() {
    $draw = new ImagickDraw();
    try { $draw->setStrokePatternURL("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_392() {
    $draw = new ImagickDraw();
    try { $draw->setTextAlignment(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_393() {
    $draw = new ImagickDraw();
    try { $draw->setTextAntialias(false); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_394() {
    $draw = new ImagickDraw();
    try { $draw->setTextDecoration(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_395() {
    $draw = new ImagickDraw();
    try { $draw->setTextEncoding("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_396() {
    $draw = new ImagickDraw();
    try { $draw->setTextInterlineSpacing(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_397() {
    $draw = new ImagickDraw();
    try { $draw->setTextInterwordSpacing(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_398() {
    $draw = new ImagickDraw();
    try { $draw->setTextKerning(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_399() {
    $draw = new ImagickDraw();
    $px = new ImagickPixel();
    try { $draw->setTextUnderColor($px); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_400() {
    $draw = new ImagickDraw();
    try { $draw->setVectorGraphics("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_401() {
    $draw = new ImagickDraw();
    try { $draw->setViewbox(1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_402() {
    $draw = new ImagickDraw();
    try { $draw->skewX(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_403() {
    $draw = new ImagickDraw();
    try { $draw->skewY(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_404() {
    $draw = new ImagickDraw();
    try { $draw->translate(1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- ImagickPixel ---
function _cov_405() {
    $px = new ImagickPixel();
    try { $px->getColorCount(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_406() {
    $px = new ImagickPixel();
    try { $px->getColorQuantum(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_407() {
    $px = new ImagickPixel();
    try { $px->getColorValueQuantum(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_408() {
    $px = new ImagickPixel();
    try { $px->getHSL(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_409() {
    $px = new ImagickPixel();
    try { $px->getIndex(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_410() {
    $px = new ImagickPixel();
    try { $px->isPixelSimilarQuantum("x"); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_411() {
    $px = new ImagickPixel();
    try { $px->setcolorcount(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_412() {
    $px = new ImagickPixel();
    try { $px->setColorValue(1, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_413() {
    $px = new ImagickPixel();
    try { $px->setColorValueQuantum(1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_414() {
    $px = new ImagickPixel();
    try { $px->setHSL(1.0, 1.0, 1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_415() {
    $px = new ImagickPixel();
    try { $px->setIndex(1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- ImagickPixelIterator ---
function _cov_416() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->getIteratorRow(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_417() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->getPreviousIteratorRow(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_418() {
    $im = new Imagick();
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->newPixelIterator($im); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_419() {
    $im = new Imagick();
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->newPixelRegionIterator($im, 1, 1, 1, 1); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_420() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->resetIterator(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_421() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->setIteratorFirstRow(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_422() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->setIteratorLastRow(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_423() {
    $pi = new ImagickPixelIterator(new Imagick());
    try { $pi->syncIterator(); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
// --- ImagickKernel ---
function _cov_424() {
    $kern = new ImagickKernel();
    try { $kern->addKernel($kern); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_425() {
    $kern = new ImagickKernel();
    try { $kern->addUnityKernel(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_426() {
    $kern = new ImagickKernel();
    try { $kern->scale(1.0); } catch (\Exception $e) {
        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }
    }
    return 0;
}
function _cov_427() {
    $kern = new ImagickKernel();
    try { $kern->separate(); } catch (\Exception $e) {
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
$n += _cov_124();
$n += _cov_125();
$n += _cov_126();
$n += _cov_127();
$n += _cov_128();
$n += _cov_129();
$n += _cov_130();
$n += _cov_131();
$n += _cov_132();
$n += _cov_133();
$n += _cov_134();
$n += _cov_135();
$n += _cov_136();
$n += _cov_137();
$n += _cov_138();
$n += _cov_139();
$n += _cov_140();
$n += _cov_141();
$n += _cov_142();
$n += _cov_143();
$n += _cov_144();
$n += _cov_145();
$n += _cov_146();
$n += _cov_147();
$n += _cov_148();
$n += _cov_149();
$n += _cov_150();
$n += _cov_151();
$n += _cov_152();
$n += _cov_153();
$n += _cov_154();
$n += _cov_155();
$n += _cov_156();
$n += _cov_157();
$n += _cov_158();
$n += _cov_159();
$n += _cov_160();
$n += _cov_161();
$n += _cov_162();
$n += _cov_163();
$n += _cov_164();
$n += _cov_165();
$n += _cov_166();
$n += _cov_167();
$n += _cov_168();
$n += _cov_169();
$n += _cov_170();
$n += _cov_171();
$n += _cov_172();
$n += _cov_173();
$n += _cov_174();
$n += _cov_175();
$n += _cov_176();
$n += _cov_177();
$n += _cov_178();
$n += _cov_179();
$n += _cov_180();
$n += _cov_181();
$n += _cov_182();
$n += _cov_183();
$n += _cov_184();
$n += _cov_185();
$n += _cov_186();
$n += _cov_187();
$n += _cov_188();
$n += _cov_189();
$n += _cov_190();
$n += _cov_191();
$n += _cov_192();
$n += _cov_193();
$n += _cov_194();
$n += _cov_195();
$n += _cov_196();
$n += _cov_197();
$n += _cov_198();
$n += _cov_199();
$n += _cov_200();
$n += _cov_201();
$n += _cov_202();
$n += _cov_203();
$n += _cov_204();
$n += _cov_205();
$n += _cov_206();
$n += _cov_207();
$n += _cov_208();
$n += _cov_209();
$n += _cov_210();
$n += _cov_211();
$n += _cov_212();
$n += _cov_213();
$n += _cov_214();
$n += _cov_215();
$n += _cov_216();
$n += _cov_217();
$n += _cov_218();
$n += _cov_219();
$n += _cov_220();
$n += _cov_221();
$n += _cov_222();
$n += _cov_223();
$n += _cov_224();
$n += _cov_225();
$n += _cov_226();
$n += _cov_227();
$n += _cov_228();
$n += _cov_229();
$n += _cov_230();
$n += _cov_231();
$n += _cov_232();
$n += _cov_233();
$n += _cov_234();
$n += _cov_235();
$n += _cov_236();
$n += _cov_237();
$n += _cov_238();
$n += _cov_239();
$n += _cov_240();
$n += _cov_241();
$n += _cov_242();
$n += _cov_243();
$n += _cov_244();
$n += _cov_245();
$n += _cov_246();
$n += _cov_247();
$n += _cov_248();
$n += _cov_249();
$n += _cov_250();
$n += _cov_251();
$n += _cov_252();
$n += _cov_253();
$n += _cov_254();
$n += _cov_255();
$n += _cov_256();
$n += _cov_257();
$n += _cov_258();
$n += _cov_259();
$n += _cov_260();
$n += _cov_261();
$n += _cov_262();
$n += _cov_263();
$n += _cov_264();
$n += _cov_265();
$n += _cov_266();
$n += _cov_267();
$n += _cov_268();
$n += _cov_269();
$n += _cov_270();
$n += _cov_271();
$n += _cov_272();
$n += _cov_273();
$n += _cov_274();
$n += _cov_275();
$n += _cov_276();
$n += _cov_277();
$n += _cov_278();
$n += _cov_279();
$n += _cov_280();
$n += _cov_281();
$n += _cov_282();
$n += _cov_283();
$n += _cov_284();
$n += _cov_285();
$n += _cov_286();
$n += _cov_287();
$n += _cov_288();
$n += _cov_289();
$n += _cov_290();
$n += _cov_291();
$n += _cov_292();
$n += _cov_293();
$n += _cov_294();
$n += _cov_295();
$n += _cov_296();
$n += _cov_297();
$n += _cov_298();
$n += _cov_299();
$n += _cov_300();
$n += _cov_301();
$n += _cov_302();
$n += _cov_303();
$n += _cov_304();
$n += _cov_305();
$n += _cov_306();
$n += _cov_307();
$n += _cov_308();
$n += _cov_309();
$n += _cov_310();
$n += _cov_311();
$n += _cov_312();
$n += _cov_313();
$n += _cov_314();
$n += _cov_315();
$n += _cov_316();
$n += _cov_317();
$n += _cov_318();
$n += _cov_319();
$n += _cov_320();
$n += _cov_321();
$n += _cov_322();
$n += _cov_323();
$n += _cov_324();
$n += _cov_325();
$n += _cov_326();
$n += _cov_327();
$n += _cov_328();
$n += _cov_329();
$n += _cov_330();
$n += _cov_331();
$n += _cov_332();
$n += _cov_333();
$n += _cov_334();
$n += _cov_335();
$n += _cov_336();
$n += _cov_337();
$n += _cov_338();
$n += _cov_339();
$n += _cov_340();
$n += _cov_341();
$n += _cov_342();
$n += _cov_343();
$n += _cov_344();
$n += _cov_345();
$n += _cov_346();
$n += _cov_347();
$n += _cov_348();
$n += _cov_349();
$n += _cov_350();
$n += _cov_351();
$n += _cov_352();
$n += _cov_353();
$n += _cov_354();
$n += _cov_355();
$n += _cov_356();
$n += _cov_357();
$n += _cov_358();
$n += _cov_359();
$n += _cov_360();
$n += _cov_361();
$n += _cov_362();
$n += _cov_363();
$n += _cov_364();
$n += _cov_365();
$n += _cov_366();
$n += _cov_367();
$n += _cov_368();
$n += _cov_369();
$n += _cov_370();
$n += _cov_371();
$n += _cov_372();
$n += _cov_373();
$n += _cov_374();
$n += _cov_375();
$n += _cov_376();
$n += _cov_377();
$n += _cov_378();
$n += _cov_379();
$n += _cov_380();
$n += _cov_381();
$n += _cov_382();
$n += _cov_383();
$n += _cov_384();
$n += _cov_385();
$n += _cov_386();
$n += _cov_387();
$n += _cov_388();
$n += _cov_389();
$n += _cov_390();
$n += _cov_391();
$n += _cov_392();
$n += _cov_393();
$n += _cov_394();
$n += _cov_395();
$n += _cov_396();
$n += _cov_397();
$n += _cov_398();
$n += _cov_399();
$n += _cov_400();
$n += _cov_401();
$n += _cov_402();
$n += _cov_403();
$n += _cov_404();
$n += _cov_405();
$n += _cov_406();
$n += _cov_407();
$n += _cov_408();
$n += _cov_409();
$n += _cov_410();
$n += _cov_411();
$n += _cov_412();
$n += _cov_413();
$n += _cov_414();
$n += _cov_415();
$n += _cov_416();
$n += _cov_417();
$n += _cov_418();
$n += _cov_419();
$n += _cov_420();
$n += _cov_421();
$n += _cov_422();
$n += _cov_423();
$n += _cov_424();
$n += _cov_425();
$n += _cov_426();
$n += _cov_427();
echo $n . "/" . 428;
"##,
    );
    assert_eq!(out, "428/428");
}
