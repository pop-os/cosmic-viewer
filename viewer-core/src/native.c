// SPDX-License-Identifier: GPL-3.0-only

#include <avif/avif.h>
#include <libraw/libraw.h>
#include <openjpeg.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define MAX_IMAGE_PIXELS (64u * 1024u * 1024u)
#define MAX_IMAGE_SIDE 32768u
#define MAX_ICC_BYTES (16u * 1024u * 1024u)

static int valid_dimensions(uint32_t width, uint32_t height)
{
    return width && height && width <= MAX_IMAGE_SIDE && height <= MAX_IMAGE_SIDE &&
           (uint64_t)width * height <= MAX_IMAGE_PIXELS;
}

typedef struct {
    uint32_t width, height;
    uint32_t count;
    int repetitions;
    double seconds;
    unsigned int rotation, mirror;
    unsigned int primaries, transfer;
    const uint8_t *icc, *exif;
    size_t icc_len, exif_len;
} ViewerAvifInfo;

avifDecoder *viewer_avif_open(const char *path, const char **error)
{
    avifDecoder *decoder = avifDecoderCreate();
    if (!decoder) {
        *error = "Could not allocate an AVIF decoder";
        return NULL;
    }
    decoder->maxThreads = 2;
    decoder->imageSizeLimit = MAX_IMAGE_PIXELS;
    decoder->imageDimensionLimit = MAX_IMAGE_SIDE;
    decoder->ignoreXMP = AVIF_TRUE;
    avifResult result = avifDecoderSetIOFile(decoder, path);
    if (result == AVIF_RESULT_OK)
        result = avifDecoderParse(decoder);
    if (result != AVIF_RESULT_OK) {
        *error = avifResultToString(result);
        avifDecoderDestroy(decoder);
        return NULL;
    }
    return decoder;
}

int viewer_avif_next(avifDecoder *decoder, ViewerAvifInfo *info, const char **error)
{
    avifResult result = avifDecoderNextImage(decoder);
    if (result == AVIF_RESULT_NO_IMAGES_REMAINING)
        return 0;
    if (result != AVIF_RESULT_OK) {
        *error = avifResultToString(result);
        return -1;
    }

    const avifImage *image = decoder->image;
    avifCropRect crop = {0, 0, image->width, image->height};
    if ((image->transformFlags & AVIF_TRANSFORM_CLAP) &&
        !avifCropRectConvertCleanApertureBox(&crop, &image->clap, image->width,
                                             image->height, image->yuvFormat, &decoder->diag)) {
        *error = "Invalid AVIF clean aperture";
        return -1;
    }

    *info = (ViewerAvifInfo) {
        .width = crop.width, .height = crop.height,
        .count = decoder->imageCount,
        .repetitions = decoder->repetitionCount,
        .seconds = decoder->imageTiming.duration,
        .rotation = (image->transformFlags & AVIF_TRANSFORM_IROT) ? image->irot.angle : 0,
        .mirror = (image->transformFlags & AVIF_TRANSFORM_IMIR) ? image->imir.axis + 1 : 0,
        .primaries = image->colorPrimaries, .transfer = image->transferCharacteristics,
        .icc = image->icc.data, .exif = image->exif.data,
        .icc_len = image->icc.size, .exif_len = image->exif.size,
    };
    return 1;
}

const char *viewer_avif_pixels(avifDecoder *decoder, uint16_t *pixels)
{
    const avifImage *image = decoder->image;
    avifResult result = AVIF_RESULT_OK;
    if (image->transformFlags & AVIF_TRANSFORM_CLAP) {
        avifCropRect crop;
        if (!avifCropRectConvertCleanApertureBox(&crop, &image->clap, image->width,
                                                 image->height, image->yuvFormat, &decoder->diag))
            return "Invalid AVIF clean aperture";

        avifRGBImage full;
        avifRGBImageSetDefaults(&full, image);
        full.depth = 16;
        full.format = AVIF_RGB_FORMAT_RGBA;
        const size_t stride = (size_t)image->width * 8;
        full.pixels = malloc(stride * image->height);
        full.rowBytes = (uint32_t)stride;
        if (!full.pixels)
            return "Could not allocate AVIF pixels";
        result = avifImageYUVToRGB(image, &full);
        if (result == AVIF_RESULT_OK) {
            for (uint32_t y = 0; y < crop.height; ++y)
                memcpy(pixels + (size_t)y * crop.width * 4,
                       full.pixels + (size_t)(y + crop.y) * stride + (size_t)crop.x * 8,
                       (size_t)crop.width * 8);
        }
        free(full.pixels);
        return result == AVIF_RESULT_OK ? NULL : avifResultToString(result);
    }

    avifRGBImage rgb;
    avifRGBImageSetDefaults(&rgb, image);
    rgb.depth = 16;
    rgb.format = AVIF_RGB_FORMAT_RGBA;
    rgb.pixels = (uint8_t *)pixels;
    rgb.rowBytes = image->width * 8;
    result = avifImageYUVToRGB(image, &rgb);
    return result == AVIF_RESULT_OK ? NULL : avifResultToString(result);
}

typedef struct {
    uint32_t width, height;
    uint16_t *rgba;
    uint8_t *icc;
    size_t icc_len;
} ViewerPixels;

void viewer_pixels_clear(ViewerPixels *pixels)
{
    free(pixels->rgba);
    free(pixels->icc);
}

const char *viewer_raw_decode(const char *path, ViewerPixels *pixels)
{
    libraw_data_t *decoder = libraw_init(0);
    if (!decoder)
        return "Could not allocate a camera RAW decoder";

    const char *error = NULL;
    libraw_processed_image_t *bitmap = NULL;
    int status = libraw_open_file(decoder, path);
    if (status == LIBRAW_SUCCESS &&
        !valid_dimensions(decoder->sizes.raw_width, decoder->sizes.raw_height)) {
        error = "RAW dimensions exceed the decode limit";
        goto done;
    }

    decoder->params.use_camera_wb = 1;
    const int srgb_output = 1;
    decoder->params.output_color = srgb_output;
    decoder->params.output_bps = 16;
    if (status == LIBRAW_SUCCESS)
        status = libraw_unpack(decoder);
    if (status == LIBRAW_SUCCESS)
        status = libraw_dcraw_process(decoder);
    if (status == LIBRAW_SUCCESS)
        bitmap = libraw_dcraw_make_mem_image(decoder, &status);
    if (status != LIBRAW_SUCCESS) {
        error = libraw_strerror(status);
        goto done;
    }

    if (!bitmap || bitmap->type != LIBRAW_IMAGE_BITMAP || bitmap->colors != 3 ||
        bitmap->bits != 16 || !valid_dimensions(bitmap->width, bitmap->height)) {
        error = "Unsupported RAW bitmap layout";
        goto done;
    }

    const size_t count = (size_t)bitmap->width * bitmap->height;
    if (bitmap->data_size < count * 3 * sizeof(uint16_t)) {
        error = "Truncated RAW bitmap";
        goto done;
    }
    pixels->rgba = malloc(count * 4 * sizeof(uint16_t));
    if (!pixels->rgba) {
        error = "Could not allocate RAW pixels";
        goto done;
    }
    pixels->width = bitmap->width;
    pixels->height = bitmap->height;
    for (size_t i = 0; i < count; ++i) {
        memcpy(pixels->rgba + i * 4, bitmap->data + i * 6, 6);
        pixels->rgba[i * 4 + 3] = UINT16_MAX;
    }

done:
    if (bitmap)
        libraw_dcraw_clear_mem(bitmap);
    libraw_close(decoder);
    return error;
}

static double component_sample(const opj_image_comp_t *comp, uint32_t x, uint32_t y)
{
    const uint32_t col = x / comp->dx < comp->x0 ? 0 : x / comp->dx - comp->x0;
    const uint32_t row = y / comp->dy < comp->y0 ? 0 : y / comp->dy - comp->y0;
    const uint32_t cx = col < comp->w ? col : comp->w - 1;
    const uint32_t cy = row < comp->h ? row : comp->h - 1;
    const int64_t sample = comp->data[(size_t)cy * comp->w + cx];
    const int64_t bias = comp->sgnd ? (INT64_C(1) << (comp->prec - 1)) : 0;
    return (double)(sample + bias) / (double)((UINT64_C(1) << comp->prec) - 1);
}

static uint16_t sample16(double value)
{
    if (value <= 0.0)
        return 0;
    if (value >= 1.0)
        return UINT16_MAX;
    return (uint16_t)(value * UINT16_MAX + 0.5);
}

const char *viewer_jpeg2000_decode(const char *path, int codestream, ViewerPixels *pixels)
{
    const char *error = "Invalid or unsupported JPEG 2000 image";
    opj_codec_t *decoder = opj_create_decompress(codestream ? OPJ_CODEC_J2K : OPJ_CODEC_JP2);
    if (!decoder)
        return "Could not allocate a JPEG 2000 decoder";
    opj_stream_t *stream = opj_stream_create_default_file_stream(path, OPJ_TRUE);
    opj_image_t *image = NULL;
    opj_dparameters_t params;
    opj_set_default_decoder_parameters(&params);

    if (!stream || !opj_setup_decoder(decoder, &params) ||
        !opj_read_header(stream, decoder, &image))
        goto done;

    if (image->x1 <= image->x0 || image->y1 <= image->y0 ||
        !valid_dimensions(image->x1 - image->x0, image->y1 - image->y0) ||
        image->numcomps < 1 || image->numcomps > 4) {
        error = "JPEG 2000 dimensions or channel count exceed the decode limit";
        goto done;
    }
    if (!opj_decode(decoder, stream, image) || !opj_end_decompress(decoder, stream))
        goto done;

    for (uint32_t i = 0; i < image->numcomps; ++i) {
        const opj_image_comp_t *c = &image->comps[i];
        if (!c->data || !c->w || !c->h || !c->dx || !c->dy || c->prec < 1 || c->prec > 31)
            goto done;
    }

    const int gray = image->numcomps < 3;
    const int alpha = image->numcomps == 2 || image->numcomps == 4;
    const opj_image_comp_t *first = &image->comps[0];
    const opj_image_comp_t *last = &image->comps[image->numcomps - 1];
    const int inferred_alpha = codestream && !last->sgnd &&
        last->dx == first->dx && last->dy == first->dy &&
        last->w == first->w && last->h == first->h &&
        last->x0 == first->x0 && last->y0 == first->y0;
    const int sycc = image->color_space == OPJ_CLRSPC_SYCC ||
        (image->color_space == OPJ_CLRSPC_UNSPECIFIED && !gray &&
         (image->comps[1].dx > image->comps[0].dx || image->comps[1].dy > image->comps[0].dy));
    if (image->color_space == OPJ_CLRSPC_CMYK || image->color_space == OPJ_CLRSPC_EYCC ||
        (alpha && !last->alpha && !inferred_alpha)) {
        error = "Unsupported JPEG 2000 color or channel layout";
        goto done;
    }

    pixels->width = image->x1 - image->x0;
    pixels->height = image->y1 - image->y0;
    pixels->rgba = malloc((size_t)pixels->width * pixels->height * 4 * sizeof(uint16_t));
    if (!pixels->rgba) {
        error = "Could not allocate JPEG 2000 pixels";
        goto done;
    }
    if (image->icc_profile_buf && image->icc_profile_len && image->icc_profile_len <= MAX_ICC_BYTES) {
        pixels->icc = malloc(image->icc_profile_len);
        if (pixels->icc) {
            pixels->icc_len = image->icc_profile_len;
            memcpy(pixels->icc, image->icc_profile_buf, pixels->icc_len);
        }
    }

    for (uint32_t y = 0; y < pixels->height; ++y) {
        for (uint32_t x = 0; x < pixels->width; ++x) {
            const uint32_t sx = x + image->x0, sy = y + image->y0;
            uint16_t *out = pixels->rgba + ((size_t)y * pixels->width + x) * 4;
            double a = component_sample(&image->comps[0], sx, sy);
            double b = gray ? a : component_sample(&image->comps[1], sx, sy);
            double c = gray ? a : component_sample(&image->comps[2], sx, sy);
            if (sycc) {
                out[0] = sample16(a + 1.402 * (c - 0.5));
                out[1] = sample16(a - 0.344136 * (b - 0.5) - 0.714136 * (c - 0.5));
                out[2] = sample16(a + 1.772 * (b - 0.5));
            } else {
                out[0] = sample16(a);
                out[1] = sample16(b);
                out[2] = sample16(c);
            }
            const double opacity = alpha ? component_sample(last, sx, sy) : 1.0;
            out[3] = sample16(opacity);
            if (alpha && last->alpha == 2 && opacity > 0.0) {
                for (int channel = 0; channel < 3; ++channel)
                    out[channel] = sample16(out[channel] / (UINT16_MAX * opacity));
            }
        }
    }
    error = NULL;

done:
    if (image)
        opj_image_destroy(image);
    if (stream)
        opj_stream_destroy(stream);
    opj_destroy_codec(decoder);
    return error;
}
