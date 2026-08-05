/* Real-ABI smoke test for glycin-ng's librsvg shim.
 *
 * Compiled against the system librsvg headers and linked against the
 * shim's librsvg-2.so.2, with real GLib/GIO/cairo/gdk-pixbuf from the
 * host. Exercises the paths actual consumers use.
 */
#include <librsvg/rsvg.h>
#include <cairo.h>
#include <gdk-pixbuf/gdk-pixbuf.h>
#include <gio/gio.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, msg)                                                    \
    do {                                                                    \
        if (cond) {                                                         \
            printf("ok   %s\n", msg);                                       \
        } else {                                                            \
            printf("FAIL %s\n", msg);                                       \
            failures++;                                                     \
        }                                                                   \
    } while (0)

static const char TWO_RECTS[] =
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\">"
    "<rect id=\"left\" x=\"0\" y=\"0\" width=\"10\" height=\"10\" fill=\"red\"/>"
    "<rect id=\"right\" x=\"10\" y=\"0\" width=\"10\" height=\"10\" fill=\"blue\"/>"
    "</svg>";

static guint32 surface_pixel(cairo_surface_t *s, int x, int y)
{
    cairo_surface_flush(s);
    const unsigned char *data = cairo_image_surface_get_data(s);
    int stride = cairo_image_surface_get_stride(s);
    return *(const guint32 *)(data + y * stride + x * 4);
}

static void test_versions(void)
{
    CHECK(rsvg_major_version == 2, "rsvg_major_version is 2");
    CHECK(rsvg_minor_version >= 52, "rsvg_minor_version is modern");
}

static void test_gobject_type(void)
{
    RsvgHandle *h = rsvg_handle_new();
    CHECK(h != NULL, "rsvg_handle_new returns a handle");
    CHECK(RSVG_IS_HANDLE(h), "RSVG_IS_HANDLE type check passes");
    CHECK(strcmp(G_OBJECT_TYPE_NAME(h), "RsvgHandle") == 0, "type name is RsvgHandle");

    GTypeQuery q;
    g_type_query(RSVG_TYPE_HANDLE, &q);
    CHECK(q.instance_size == sizeof(RsvgHandle), "instance size matches public struct");
    CHECK(q.class_size == sizeof(RsvgHandleClass), "class size matches public struct");
    g_object_unref(h);
}

static void test_construct_properties(void)
{
    RsvgHandle *h = g_object_new(RSVG_TYPE_HANDLE,
                                 "flags", RSVG_HANDLE_FLAG_UNLIMITED,
                                 "dpi-x", 96.0,
                                 "dpi-y", 96.0,
                                 "base-uri", "file:///tmp/some/dir/x.svg",
                                 NULL);
    RsvgHandleFlags flags = RSVG_HANDLE_FLAGS_NONE;
    gdouble dx = 0, dy = 0;
    gchar *uri = NULL;
    g_object_get(h, "flags", &flags, "dpi-x", &dx, "dpi-y", &dy,
                 "base-uri", &uri, NULL);
    CHECK(flags == RSVG_HANDLE_FLAG_UNLIMITED, "construct-only flags round-trips");
    CHECK(dx == 96.0 && dy == 96.0, "dpi properties round-trip");
    CHECK(uri && strcmp(uri, "file:///tmp/some/dir/x.svg") == 0, "base-uri round-trips");
    g_free(uri);

    gchar *title = (gchar *)0x1;
    g_object_get(h, "title", &title, NULL);
    CHECK(title == NULL, "deprecated title property is NULL");
    g_object_unref(h);
}

static void test_load_and_dimensions(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    CHECK(h != NULL && error == NULL, "new_from_data loads");

    gdouble w = 0, h_px = 0;
    gboolean ok = rsvg_handle_get_intrinsic_size_in_pixels(h, &w, &h_px);
    CHECK(ok && w == 20.0 && h_px == 10.0, "intrinsic size in pixels is 20x10");

    gboolean has_w = FALSE, has_h = FALSE, has_vb = TRUE;
    RsvgLength lw, lh;
    RsvgRectangle vb;
    rsvg_handle_get_intrinsic_dimensions(h, &has_w, &lw, &has_h, &lh, &has_vb, &vb);
    CHECK(has_w && has_h, "intrinsic dimensions always present");
    CHECK(lw.length == 20.0 && lw.unit == RSVG_UNIT_PX, "width is 20px");
    CHECK(!has_vb, "no viewBox reported");

    RsvgDimensionData dim;
    rsvg_handle_get_dimensions(h, &dim);
    CHECK(dim.width == 20 && dim.height == 10 && dim.em == 20.0, "get_dimensions matches");

    gint width_prop = 0;
    gdouble em_prop = 0;
    g_object_get(h, "width", &width_prop, "em", &em_prop, NULL);
    CHECK(width_prop == 20 && em_prop == 20.0, "width/em properties after load");

    CHECK(rsvg_handle_has_sub(h, "#right"), "has_sub finds #right");
    CHECK(!rsvg_handle_has_sub(h, "#missing"), "has_sub rejects unknown id");

    RsvgPositionData pos;
    CHECK(rsvg_handle_get_position_sub(h, &pos, "#right") && pos.x == 10 && pos.y == 0,
          "get_position_sub for #right");
    RsvgDimensionData sub;
    CHECK(rsvg_handle_get_dimensions_sub(h, &sub, "#right") && sub.width == 10,
          "get_dimensions_sub for #right");

    g_object_unref(h);
}

static void test_render_document(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    cairo_surface_t *surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 40, 20);
    cairo_t *cr = cairo_create(surface);
    RsvgRectangle viewport = { 0, 0, 40, 20 };
    gboolean ok = rsvg_handle_render_document(h, cr, &viewport, &error);
    CHECK(ok && error == NULL, "render_document succeeds");
    CHECK(surface_pixel(surface, 5, 5) == 0xFFFF0000u, "left half renders red");
    CHECK(surface_pixel(surface, 35, 5) == 0xFF0000FFu, "right half renders blue");
    cairo_destroy(cr);
    cairo_surface_destroy(surface);

    /* Scaled CTM: the shim must render at device resolution. */
    surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 80, 40);
    cr = cairo_create(surface);
    cairo_scale(cr, 2.0, 2.0);
    ok = rsvg_handle_render_document(h, cr, &viewport, &error);
    CHECK(ok && error == NULL, "render_document under scaled CTM succeeds");
    CHECK(surface_pixel(surface, 78, 20) == 0xFF0000FFu, "scaled render covers device pixels");
    cairo_destroy(cr);
    cairo_surface_destroy(surface);

    /* Layer and element renders. */
    surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 20, 10);
    cr = cairo_create(surface);
    RsvgRectangle vp2 = { 0, 0, 20, 10 };
    ok = rsvg_handle_render_layer(h, cr, "#right", &vp2, &error);
    CHECK(ok && error == NULL, "render_layer succeeds");
    CHECK(surface_pixel(surface, 15, 5) == 0xFF0000FFu, "layer keeps document position");
    CHECK(surface_pixel(surface, 5, 5) == 0x00000000u, "layer leaves the rest empty");
    cairo_destroy(cr);
    cairo_surface_destroy(surface);

    surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 10, 10);
    cr = cairo_create(surface);
    RsvgRectangle vp3 = { 0, 0, 10, 10 };
    ok = rsvg_handle_render_element(h, cr, "#right", &vp3, &error);
    CHECK(ok && error == NULL, "render_element succeeds");
    CHECK(surface_pixel(surface, 5, 5) == 0xFF0000FFu, "element is extracted to origin");
    cairo_destroy(cr);
    cairo_surface_destroy(surface);

    RsvgRectangle ink, logical;
    ok = rsvg_handle_get_geometry_for_layer(h, "#right", &vp2, &ink, &logical, &error);
    CHECK(ok && ink.x == 10.0 && ink.width == 10.0, "geometry_for_layer for #right");
    ok = rsvg_handle_get_geometry_for_element(h, "#right", &ink, &logical, &error);
    CHECK(ok && ink.x == 0.0 && ink.width == 10.0, "geometry_for_element normalized");

    g_object_unref(h);
}

static void test_pixbuf(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    GdkPixbuf *pb = rsvg_handle_get_pixbuf_and_error(h, &error);
    CHECK(pb != NULL && error == NULL, "get_pixbuf_and_error returns a pixbuf");
    CHECK(gdk_pixbuf_get_width(pb) == 20 && gdk_pixbuf_get_height(pb) == 10,
          "pixbuf has the natural size");
    const guchar *px = gdk_pixbuf_get_pixels(pb);
    CHECK(px[0] == 255 && px[1] == 0 && px[2] == 0 && px[3] == 255,
          "pixbuf top-left pixel is opaque red");
    g_object_unref(pb);

    pb = rsvg_handle_get_pixbuf_sub(h, "#right");
    CHECK(pb != NULL, "get_pixbuf_sub returns a pixbuf");
    px = gdk_pixbuf_get_pixels(pb);
    CHECK(px[3] == 0, "sub-pixbuf leaves other areas transparent");
    int rs = gdk_pixbuf_get_rowstride(pb);
    const guchar *right = px + 5 * rs + 15 * 4;
    CHECK(right[2] == 255 && right[3] == 255, "sub-pixbuf renders #right blue");
    g_object_unref(pb);
    g_object_unref(h);
}

static void size_func_4x(gint *w, gint *h, gpointer data)
{
    (void)data;
    *w *= 4;
    *h *= 4;
}

static void test_size_callback(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    rsvg_handle_set_size_callback(h, size_func_4x, NULL, NULL);

    RsvgDimensionData dim;
    rsvg_handle_get_dimensions(h, &dim);
    CHECK(dim.width == 80 && dim.height == 40, "size callback scales get_dimensions");

    GdkPixbuf *pb = rsvg_handle_get_pixbuf(h);
    CHECK(pb && gdk_pixbuf_get_width(pb) == 80, "get_pixbuf honors size callback");
    if (pb) {
        const guchar *px = gdk_pixbuf_get_pixels(pb);
        int rs = gdk_pixbuf_get_rowstride(pb);
        const guchar *mid = px + 20 * rs + 70 * 4;
        CHECK(mid[2] == 255 && mid[3] == 255, "scaled pixbuf is vector-sharp blue");
        g_object_unref(pb);
    }
    g_object_unref(h);
}

static void test_write_close(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new();
    size_t half = strlen(TWO_RECTS) / 2;
    CHECK(rsvg_handle_write(h, (const guint8 *)TWO_RECTS, half, &error), "write first half");
    CHECK(rsvg_handle_write(h, (const guint8 *)TWO_RECTS + half,
                            strlen(TWO_RECTS) - half, &error),
          "write second half");
    CHECK(rsvg_handle_close(h, &error) && error == NULL, "close parses the document");

    cairo_surface_t *surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 20, 10);
    cairo_t *cr = cairo_create(surface);
    CHECK(rsvg_handle_render_cairo(h, cr), "deprecated render_cairo works");
    CHECK(surface_pixel(surface, 15, 5) == 0xFF0000FFu, "render_cairo output correct");
    cairo_destroy(cr);
    cairo_surface_destroy(surface);
    g_object_unref(h);
}

static void test_stylesheet_and_dpi(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    const char css[] = "rect { fill: #00ff00 !important; }";
    CHECK(rsvg_handle_set_stylesheet(h, (const guint8 *)css, strlen(css), &error),
          "set_stylesheet succeeds");
    GdkPixbuf *pb = rsvg_handle_get_pixbuf_and_error(h, &error);
    const guchar *px = pb ? gdk_pixbuf_get_pixels(pb) : NULL;
    CHECK(px && px[1] == 255 && px[0] == 0, "stylesheet recolors to green");
    if (pb) g_object_unref(pb);
    g_object_unref(h);

    static const char INCH[] =
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1in\" height=\"1in\"/>";
    h = rsvg_handle_new_from_data((const guint8 *)INCH, strlen(INCH), &error);
    rsvg_handle_set_dpi(h, 96.0);
    gdouble w = 0, hh = 0;
    CHECK(rsvg_handle_get_intrinsic_size_in_pixels(h, &w, &hh) && w == 96.0,
          "1in resolves to 96px at 96dpi");
    rsvg_handle_set_dpi(h, 192.0);
    RsvgDimensionData dim;
    rsvg_handle_get_dimensions(h, &dim);
    CHECK(dim.width == 192, "dpi change re-resolves physical units");
    g_object_unref(h);
}

static void test_error_paths(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)"<svg garbage", 12, &error);
    CHECK(h == NULL && error != NULL, "garbage input fails with an error");
    CHECK(error && error->domain == rsvg_error_quark(), "error is in the rsvg domain");
    if (error) g_error_free(error);
    error = NULL;

    h = rsvg_handle_new();
    CHECK(!rsvg_handle_close(h, &error) && error != NULL, "close without data errors");
    g_clear_error(&error);
    g_object_unref(h);

    /* Deprecated no-ops must be callable. */
    rsvg_init();
    rsvg_cleanup();
    rsvg_set_default_dpi(90.0);
    rsvg_term();
}

static void test_file_loading(const char *dir)
{
    char path[512];
    int n = snprintf(path, sizeof path, "%s/two_rects.svg", dir);
    if (n < 0 || (size_t)n >= sizeof path) {
        CHECK(0, "fixture path fits in the buffer");
        return;
    }
    FILE *f = fopen(path, "w");
    if (!f) {
        CHECK(0, "fixture file is writable");
        return;
    }
    fwrite(TWO_RECTS, 1, strlen(TWO_RECTS), f);
    fclose(f);

    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_file(path, &error);
    CHECK(h != NULL && error == NULL, "new_from_file loads");
    if (h) {
        const char *uri = rsvg_handle_get_base_uri(h);
        CHECK(uri && strstr(uri, "two_rects.svg") != NULL, "base uri set from file");
        g_object_unref(h);
    }

    GFile *gf = g_file_new_for_path(path);
    h = rsvg_handle_new_from_gfile_sync(gf, RSVG_HANDLE_FLAGS_NONE, NULL, &error);
    CHECK(h != NULL && error == NULL, "new_from_gfile_sync loads");
    if (h) g_object_unref(h);
    g_object_unref(gf);

    GdkPixbuf *pb = rsvg_pixbuf_from_file_at_size(path, 40, 20, &error);
    CHECK(pb && gdk_pixbuf_get_width(pb) == 40, "rsvg_pixbuf_from_file_at_size works");
    if (pb) g_object_unref(pb);

    pb = rsvg_pixbuf_from_file_at_max_size(path, 10, 10, &error);
    CHECK(pb && gdk_pixbuf_get_width(pb) == 10 && gdk_pixbuf_get_height(pb) == 5,
          "rsvg_pixbuf_from_file_at_max_size shrinks uniformly");
    if (pb) g_object_unref(pb);
}

static void test_text_rendering(void)
{
    static const char TEXT[] =
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"128\" height=\"32\">"
        "<rect width=\"128\" height=\"32\" fill=\"white\"/>"
        "<text x=\"10\" y=\"24\" font-family=\"sans-serif\" font-size=\"20\" "
        "fill=\"black\">18:30</text></svg>";
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TEXT, strlen(TEXT), &error);
    CHECK(h != NULL, "text SVG loads");
    GdkPixbuf *pb = rsvg_handle_get_pixbuf_and_error(h, &error);
    int dark = 0;
    if (pb) {
        const guchar *px = gdk_pixbuf_get_pixels(pb);
        int n = gdk_pixbuf_get_height(pb) * gdk_pixbuf_get_rowstride(pb);
        for (int i = 0; i + 3 < n; i += 4)
            if (px[i] < 128 && px[i + 3] > 128) dark++;
        g_object_unref(pb);
    }
    CHECK(dark > 20, "text renders with system fonts");
    g_object_unref(h);
}

static void test_guards(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data((const guint8 *)TWO_RECTS,
                                              strlen(TWO_RECTS), &error);
    RsvgRectangle vp = { 0, 0, 10, 10 };
    CHECK(!rsvg_handle_render_document(h, NULL, &vp, &error) && error != NULL,
          "NULL cairo context fails instead of crashing");
    g_clear_error(&error);
    CHECK(!rsvg_handle_render_layer(h, NULL, "#right", &vp, &error) && error != NULL,
          "render_layer rejects a NULL cairo context");
    g_clear_error(&error);
    CHECK(!rsvg_handle_render_element(h, NULL, "#right", &vp, &error) && error != NULL,
          "render_element rejects a NULL cairo context");
    g_clear_error(&error);
    /* render_element with a NULL id delegates to render_document,
     * which must apply the same guard. */
    CHECK(!rsvg_handle_render_element(h, NULL, NULL, &vp, &error) && error != NULL,
          "render_element delegate rejects a NULL cairo context");
    g_clear_error(&error);

    /* A GObject that is not an RsvgHandle must be rejected, not
     * blindly dereferenced. */
    GObject *foreign = g_object_new(G_TYPE_OBJECT, NULL);
    CHECK(!RSVG_IS_HANDLE(foreign), "foreign object fails the type check");
    CHECK(rsvg_handle_get_base_uri((RsvgHandle *)foreign) == NULL,
          "foreign object rejected by get_base_uri");
    CHECK(!rsvg_handle_has_sub((RsvgHandle *)foreign, "#x"),
          "foreign object rejected by has_sub");
    g_object_unref(foreign);
    g_object_unref(h);
}

/* Gzipped <svg width="64pt" height="32"><rect fill="red" .../></svg> */
static const guint8 SVGZ[] = {
    31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 85, 204, 65, 10, 128, 32, 16, 64, 209,
    171, 200, 28, 192, 17, 139, 22, 161, 94, 38, 77, 5, 43, 209, 161, 233, 248,
    213, 42, 90, 191, 207, 55, 253, 140, 226, 218, 202, 222, 45, 36, 162, 58,
    35, 50, 179, 228, 65, 30, 45, 162, 86, 74, 225, 83, 128, 224, 236, 41, 89,
    152, 198, 74, 32, 82, 200, 49, 145, 133, 65, 131, 51, 45, 44, 244, 241, 15,
    197, 154, 75, 177, 208, 130, 7, 116, 230, 29, 185, 27, 86, 27, 31, 91, 112,
    0, 0, 0,
};

static void test_svgz(void)
{
    GError *error = NULL;
    RsvgHandle *h = rsvg_handle_new_from_data(SVGZ, sizeof SVGZ, &error);
    CHECK(h != NULL && error == NULL, "SVGZ input loads");
    if (!h) return;

    gboolean has_w, has_h, has_vb;
    RsvgLength lw, lh;
    RsvgRectangle vb;
    rsvg_handle_get_intrinsic_dimensions(h, &has_w, &lw, &has_h, &lh, &has_vb, &vb);
    CHECK(lw.length == 64.0 && lw.unit == RSVG_UNIT_PT,
          "SVGZ intrinsic width is 64pt, not a percent fallback");

    gdouble w = 0, hh = 0;
    /* 64pt at the default 90dpi is 80px. */
    CHECK(rsvg_handle_get_intrinsic_size_in_pixels(h, &w, &hh) && w == 80.0,
          "SVGZ intrinsic size resolves to pixels");

    GdkPixbuf *pb = rsvg_handle_get_pixbuf_and_error(h, &error);
    CHECK(pb != NULL, "SVGZ renders");
    if (pb) {
        const guchar *px = gdk_pixbuf_get_pixels(pb);
        CHECK(px[0] == 255 && px[3] == 255, "SVGZ pixels are correct");
        g_object_unref(pb);
    }
    g_object_unref(h);
}

int main(int argc, char **argv)
{
    const char *dir = argc > 1 ? argv[1] : "/tmp";
    test_versions();
    test_gobject_type();
    test_construct_properties();
    test_load_and_dimensions();
    test_render_document();
    test_pixbuf();
    test_size_callback();
    test_write_close();
    test_stylesheet_and_dpi();
    test_error_paths();
    test_file_loading(dir);
    test_text_rendering();
    test_guards();
    test_svgz();

    printf("\n%s: %d failure(s)\n", failures ? "FAILED" : "PASSED", failures);
    return failures ? 1 : 0;
}
