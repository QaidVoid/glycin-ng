/* Version script controlling which symbols librsvg-2.so.2 exports.
 *
 * Only the public librsvg C ABI is visible: the 52 rsvg_* functions
 * and the three rsvg_*_version variables. Everything else becomes
 * local so the linker can dead-code-eliminate unreachable code. */

{
    global:
        rsvg_*;
    local:
        *;
};
