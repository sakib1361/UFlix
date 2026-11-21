// libmpv (MPVKit) contains its OpenGL ES hwdec interop (hwdec_ios_gl.m), which references EAGLContext. The app never
// uses it (video goes through mpv's software renderer, decoding through VideoToolbox copy-back), and the runner's iOS
// SDK no longer resolves EAGLContext from OpenGLES, so this empty class satisfies the linker. Never called.
#import <Foundation/Foundation.h>

@interface EAGLContext : NSObject
@end

@implementation EAGLContext
@end
