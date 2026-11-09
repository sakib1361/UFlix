//! mpv's OpenGL renderer: draws the video into the embedder's GL context, so the UI can be drawn on top in the same
//! frame (translucent overlays). Create, use and drop it only on the thread that owns the GL context.

use crate::ffi::{self, api};
use crate::player::Player;
use std::ffi::{CStr, c_char, c_int, c_void};

/// The native display the GL context lives on; mpv needs it for zero-copy VA-API interop.
#[derive(Clone, Copy)]
pub enum NativeDisplay {
    Wayland(*mut c_void),
    X11(*mut c_void),
}

type Callback = Box<dyn Fn() + Send + Sync>;

/// Runs on an mpv thread. A panic must not unwind into libmpv: it would abort the process.
unsafe extern "C" fn call(ctx: *mut c_void) {
    let _ = std::panic::catch_unwind(|| unsafe { (*(ctx as *const Callback))() });
}

pub struct RenderContext {
    ctx: *mut c_void,
    update: Option<Box<Callback>>,
    get_integerv: unsafe extern "C" fn(u32, *mut c_int),
}

impl RenderContext {
    pub fn new(player: &Player, get_proc_address: &dyn Fn(&CStr) -> *const c_void, display: Option<NativeDisplay>) -> Result<Self, String> {
        let a = api()?;
        // called by libmpv: a panic in the embedder's loader becomes "function not found" instead of an abort
        unsafe extern "C" fn trampoline(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                let f = &*(ctx as *const &dyn Fn(&CStr) -> *const c_void);
                f(CStr::from_ptr(name)) as *mut c_void
            }))
            .unwrap_or(std::ptr::null_mut())
        }
        // mpv resolves every GL function inside render_context_create, so the loader only has to live for this call
        let loader: &dyn Fn(&CStr) -> *const c_void = get_proc_address;
        let mut init = ffi::OpenGlInitParams { get_proc_address: trampoline, ctx: &loader as *const _ as *mut c_void };
        let (dty, dptr) = match display {
            Some(NativeDisplay::Wayland(p)) => (ffi::RENDER_PARAM_WL_DISPLAY, p),
            Some(NativeDisplay::X11(p)) => (ffi::RENDER_PARAM_X11_DISPLAY, p),
            None => (ffi::RENDER_PARAM_INVALID, std::ptr::null_mut()),
        };
        let mut params = [
            ffi::RenderParam { ty: ffi::RENDER_PARAM_API_TYPE, data: c"opengl".as_ptr() as *mut c_void },
            ffi::RenderParam { ty: ffi::RENDER_PARAM_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
            ffi::RenderParam { ty: dty, data: dptr },
            ffi::RenderParam { ty: ffi::RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let mut ctx = std::ptr::null_mut();
        let r = unsafe { (a.render_context_create)(&mut ctx, player.raw(), params.as_mut_ptr()) };
        if r < 0 {
            return Err(format!("mpv_render_context_create: {}", ffi::error_string(r)));
        }
        let gi = get_proc_address(c"glGetIntegerv");
        if gi.is_null() {
            unsafe { (a.render_context_free)(ctx) };
            return Err("glGetIntegerv not found".into());
        }
        let get_integerv = unsafe { std::mem::transmute::<*const c_void, unsafe extern "C" fn(u32, *mut c_int)>(gi) };
        Ok(Self { ctx, update: None, get_integerv })
    }

    /// Called from an mpv thread whenever a new frame should be drawn.
    pub fn set_update_callback(&mut self, f: impl Fn() + Send + Sync + 'static) {
        let Ok(a) = api() else { return };
        let boxed: Box<Callback> = Box::new(Box::new(f));
        unsafe { (a.render_context_set_update_callback)(self.ctx, Some(call), &*boxed as *const Callback as *mut c_void) };
        self.update = Some(boxed);
    }

    /// Draws the current frame into the framebuffer bound now, sized `width`×`height` pixels.
    pub fn render(&self, width: i32, height: i32) {
        let Ok(a) = api() else { return };
        const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
        let mut fbo_id: c_int = 0;
        unsafe {
            (self.get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut fbo_id);
            (a.render_context_update)(self.ctx);
        }
        let mut fbo = ffi::OpenGlFbo { fbo: fbo_id, w: width, h: height, internal_format: 0 };
        let mut flip: c_int = 1;
        let mut params = [
            ffi::RenderParam { ty: ffi::RENDER_PARAM_OPENGL_FBO, data: &mut fbo as *mut _ as *mut c_void },
            ffi::RenderParam { ty: ffi::RENDER_PARAM_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
            ffi::RenderParam { ty: ffi::RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        unsafe { (a.render_context_render)(self.ctx, params.as_mut_ptr()) };
    }

    pub fn report_swap(&self) {
        if let Ok(a) = api() {
            unsafe { (a.render_context_report_swap)(self.ctx) };
        }
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe {
                (a.render_context_set_update_callback)(self.ctx, None, std::ptr::null_mut());
                (a.render_context_free)(self.ctx);
            }
        }
    }
}
