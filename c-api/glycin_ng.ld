/* Version script controlling which symbols libglycin_ng.so exports.
 *
 * Only the documented C ABI (everything prefixed `glycin_ng_`) is
 * visible to dynamic consumers. Every other Rust symbol becomes
 * local so the linker can dead-code-eliminate code reachable only
 * through it, matching the size profile the old statically-linked
 * shim used to get from whole-program LTO. */

{
    global:
        glycin_ng_*;
    local:
        *;
};
