# Image OOP API spec

This directory holds the maintenance tooling for the Imagick / Gmagick OOP API
surface, kept alongside the `elephc-image` crate it serves:

- `gen_image_api_stubs.py` — the stub/coverage generator (run manually; not part
  of the build).
- `api_spec.json` — its pinned input spec.
- `check_extern_exports.py` — verifies every `extern "elephc_image"` declaration
  in the prelude has a matching `#[no_mangle]` export in the crate.

The CI job `image-api-sync` (`.github/workflows/ci.yml`) re-runs the generator and
fails if the committed stubs/tests drift, then runs `check_extern_exports.py`.
Run both locally before committing changes to the prelude or `api_spec.json`:

```sh
python3 crates/elephc-image/tools/gen_image_api_stubs.py
git diff --exit-code -- src/image_prelude.rs tests/codegen/image/
python3 crates/elephc-image/tools/check_extern_exports.py
```

`api_spec.json` is the pinned input for `gen_image_api_stubs.py`. It
lists, per class (Imagick / ImagickDraw / ImagickPixel / ImagickPixelIterator /
ImagickKernel / Gmagick / GmagickDraw / GmagickPixel), the public method
surface as `name` / `static` / `params` (`type`, `name`, `byref`, `default`) /
`ret`. The generator transcribes each not-yet-implemented method into a
throwing stub (spliced into `src/image_prelude.rs`) and a coverage test under
`tests/codegen/image/`.

## Provenance & licensing

The method signatures were extracted from the PHP manual class synopses for the
Imagick and Gmagick families (`https://www.php.net/manual/en/`). The PHP manual
is licensed under the **Creative Commons Attribution 3.0** license
(<https://creativecommons.org/licenses/by/3.0/legalcode>), © the PHP
Documentation Group.

Only the structured method-signature data is retained here (as
`api_spec.json`); the original rendered HTML pages are not vendored. To extend
the surface when php.net adds methods, edit `api_spec.json` directly — it is the
canonical, human-reviewable spec — then re-run `gen_image_api_stubs.py`.
