extern crate gleam;
extern crate glutin;
extern crate fasternet_common;
extern crate app_units;
extern crate webrender;
extern crate image;
extern crate rayon;

mod app;
mod style;

use gleam::gl;
use glutin::GlContext;
use webrender::api::*;
use std::sync::Arc;
// use webrender::{PROFILER_DBG, RENDER_TARGET_DBG, TEXTURE_CACHE_DBG};

use app::App;

struct Notifier {
    loop_proxy: Arc<glutin::EventsLoopProxy>,
}

impl Notifier {
    fn new(loop_proxy: glutin::EventsLoopProxy)-> Notifier {
        Notifier {
            loop_proxy: Arc::new(loop_proxy),
        }
    }
}

impl RenderNotifier for Notifier {
    fn new_document_ready(&self, _id: DocumentId, _scrolled: bool, _composite_needed: bool) {
        self.wake_up();
    }

    fn wake_up(&self) {
        self.loop_proxy.wakeup().unwrap();
    }

    fn clone(&self) -> Box<RenderNotifier + 'static> {
        Box::new(Notifier{ loop_proxy: self.loop_proxy.clone() })
    }
}

pub fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let window_builder = glutin::WindowBuilder::new()
        .with_multitouch()
        .with_visibility(false)
        .with_title("Quickdown");
    let context = glutin::ContextBuilder::new()
        .with_vsync(true)
        .with_gl(glutin::GlRequest::GlThenGles {
            opengl_version: (3, 2),
            opengles_version: (3, 0)
        });
    let gl_window = glutin::GlWindow::new(window_builder, context, &events_loop).unwrap();

    unsafe { gl_window.make_current().ok() };

    let gl = match gl::GlType::default() {
        gl::GlType::Gl => unsafe { gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) },
        gl::GlType::Gles => unsafe { gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) },
    };

    let (mut width, mut height) = gl_window.get_inner_size().unwrap();

    // TODO hack until https://github.com/tomaka/winit/pull/359 is merged and integrated into glutin master
    width = ((width as f32) * gl_window.hidpi_factor()) as u32;
    height = ((height as f32) * gl_window.hidpi_factor()) as u32;

    let opts = webrender::RendererOptions {
        debug: true,
        precache_shaders: false,
        enable_subpixel_aa: true, // TODO decide
        enable_aa: true,
        device_pixel_ratio: gl_window.hidpi_factor(),
        .. webrender::RendererOptions::default()
    };

    let size = DeviceUintSize::new(width, height);
    let notifier = Box::new(Notifier::new(events_loop.create_proxy()));
    let (mut renderer, sender) = webrender::Renderer::new(gl, notifier, opts).unwrap();
    let api = sender.create_api();
    let document_id = api.add_document(size, 0);

    // renderer.set_render_notifier(notifier);
    let pipeline_id = PipelineId(0, 0);

    let args: Vec<String> = std::env::args().collect();
    let mut app = App::new(&api,pipeline_id, &args[1]);

    let epoch = Epoch(0);
    let root_background_color = app.bg_color();

    let dpi_scale = gl_window.hidpi_factor();
    let layout_size = LayoutSize::new((width as f32) / dpi_scale, (height as f32) / dpi_scale);
    let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);
    let mut resources = ResourceUpdates::new();


    app.render(&api, &mut builder, &mut resources, layout_size, pipeline_id, document_id);
    api.set_display_list(
        document_id,
        epoch,
        Some(root_background_color),
        LayoutSize::new(width as f32, height as f32),
        builder.finalize(),
        true,
        resources
    );
    api.set_root_pipeline(document_id, pipeline_id);
    api.generate_frame(document_id, None);

    // let gl_test = support::load(sgl);
    let mut window_visible = false;

    events_loop.run_forever(|event| {
        // println!("{:?}", event);
        match event {
            glutin::Event::WindowEvent { event, .. } => {
                match event {
                    glutin::WindowEvent::Resized(w, h) => {
                        gl_window.resize(w, h);
                        width = w;
                        height = h;
                        let size = DeviceUintSize::new(width, height);
                        let rect = DeviceUintRect::new(DeviceUintPoint::zero(), size);
                        api.set_window_parameters(document_id, size, rect, gl_window.hidpi_factor());
                    },
                    glutin::WindowEvent::Closed |
                    glutin::WindowEvent::KeyboardInput {
                        input: glutin::KeyboardInput {virtual_keycode: Some(glutin::VirtualKeyCode::Escape), .. }, ..
                    } => return glutin::ControlFlow::Break,
                    glutin::WindowEvent::KeyboardInput {
                        input: glutin::KeyboardInput {
                            virtual_keycode: Some(glutin::VirtualKeyCode::R),
                            state: glutin::ElementState::Pressed, ..
                        }, ..
                    } => {
                        println!("toggling profiler");
                        renderer.toggle_debug_flags(webrender::DebugFlags::PROFILER_DBG | webrender::DebugFlags::GPU_TIME_QUERIES);
                    }
                    _ => (),
                }

                let dpi_scale = gl_window.hidpi_factor();
                let layout_size = LayoutSize::new((width as f32) / dpi_scale, (height as f32) / dpi_scale);
                if app.on_event(event, &api, layout_size, document_id) {
                    let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);
                    let mut resources = ResourceUpdates::new();

                    app.render(&api, &mut builder, &mut resources, layout_size, pipeline_id, document_id);
                    api.set_display_list(
                        document_id,
                        epoch,
                        Some(root_background_color),
                        layout_size,
                        builder.finalize(),
                        true,
                        resources
                    );
                    api.generate_frame(document_id, None);
                }
            },
            _ => (),
        }

        renderer.update();
        renderer.render(DeviceUintSize::new(width, height)).unwrap();
        gl_window.swap_buffers().ok();
        if !window_visible {
            gl_window.show();
            window_visible = true;
        }
        glutin::ControlFlow::Continue
    });

    renderer.deinit();
}
