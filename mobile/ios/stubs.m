// Link-time stand-ins for code MPVKit's libraries reference but this app never runs. Video goes through mpv's software
// renderer with VideoToolbox copy-back decoding, so neither the OpenGL ES interop nor gpu-next's Dolby Vision path is
// ever used.
#import <Foundation/Foundation.h>
#include <stddef.h>
#include <stdint.h>

// libmpv's GL ES hwdec interop (hwdec_ios_gl.m) references EAGLContext, which the current iOS SDK's OpenGLES no
// longer resolves.
@interface EAGLContext : NSObject
@end

@implementation EAGLContext
@end

// libdovi is a Rust static library that carries its own copy of Rust's std, which clashes with the app's
// (duplicate rust_eh_personality, and it takes over __rust_alloc). Only libplacebo calls it, for Dolby Vision RPU
// metadata; NULL means "no metadata", which libplacebo handles.
void *dovi_parse_unspec62_nalu(const uint8_t *buf, size_t len) { return NULL; }
const void *dovi_rpu_get_header(const void *rpu) { return NULL; }
const void *dovi_rpu_get_vdr_dm_data(const void *rpu) { return NULL; }
void dovi_rpu_free(void *rpu) {}
void dovi_rpu_free_header(const void *header) {}
void dovi_rpu_free_vdr_dm_data(const void *data) {}
