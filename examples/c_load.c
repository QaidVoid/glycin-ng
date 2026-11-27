/* Sample C program: decode an image and print its dimensions.
 *
 * Build: cargo build --release -p glycin-ng-c
 *        cc -I include examples/c_load.c -Ltarget/release -lglycin_ng -o c_load
 *
 * Run:   LD_LIBRARY_PATH=target/release ./c_load path/to/image.png
 */
#include <stdio.h>
#include <stdlib.h>

#include "glycin_ng.h"

int main(int argc, char** argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <image>\n", argv[0]);
        return 2;
    }

    GlycinNgLoader* loader = glycin_ng_loader_new_path(argv[1]);
    if (!loader) {
        fprintf(stderr, "loader_new_path failed: %s\n", glycin_ng_last_error());
        return 1;
    }

    GlycinNgImage* image = glycin_ng_loader_load(loader);
    if (!image) {
        fprintf(stderr, "loader_load failed: %s\n", glycin_ng_last_error());
        return 1;
    }

    const char* format = glycin_ng_image_format_name(image);
    printf("format=%s width=%u height=%u frames=%zu\n",
           format ? format : "?",
           glycin_ng_image_width(image),
           glycin_ng_image_height(image),
           glycin_ng_image_frame_count(image));

    const GlycinNgTexture* tex = glycin_ng_image_texture(image, 0);
    if (tex) {
        printf("texture format=%u stride=%u bytes=%zu\n",
               glycin_ng_texture_format(tex),
               glycin_ng_texture_stride(tex),
               glycin_ng_texture_data_len(tex));
    }

    glycin_ng_image_free(image);
    return 0;
}
