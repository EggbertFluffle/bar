use std::{convert::TryInto, num::NonZeroU32};

use fontdue::{Font, FontSettings, Metrics};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};

use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
    Connection, QueueHandle,
};

fn main() {
    // All Wayland apps start by connecting the server.
    let conn = Connection::connect_to_env()
        .unwrap();

    // Enumerate the list of globals to get the protocols the server implements.
    let (globals, mut event_queue) = registry_queue_init(&conn)
        .unwrap();
    let qh = event_queue.handle();

    // The compositor (not to be confused with the server which is commonly called the compositor) allows
    // configuring surfaces to be presented.
    let compositor = CompositorState::bind(&globals, &qh)
        .expect("wl_compositor is not available");

    // This app uses the wlr layer shell, which may not be available with every compositor.
    let layer_shell = LayerShell::bind(&globals, &qh)
        .expect("Wlr layer shell is not available");

    // Since we are not using the GPU in this example, we use wl_shm to allow software rendering to a buffer
    // we share with the compositor process.
    let shm = Shm::bind(&globals, &qh)
        .expect("wl_shm is not available");

    // A layer surface is created from a surface.
    let surface = compositor.create_surface(&qh);

    // And then we create the layer shell.
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("bar"), None);

    // Configure the layer surface, providing things like the anchor on screen, desired size and the keyboard
    // interactivity
    layer.set_anchor(Anchor::BOTTOM);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(1920, 60);

    // In order for the layer surface to be mapped, we need to perform an initial commit with no attached\
    // buffer. For more info, see WaylandSurface::commit
    //
    // The compositor will respond with an initial configure that we can then use to present to the layer
    // surface with the correct options.
    layer.commit();

    // Pool for wl_shm
    let pool = SlotPool::new(1920 * 60 * 4, &shm)
        .expect("Failed to create pool");

    let glyph_cache = GlyphCache::new("./Iosevka.ttc", 52.0).unwrap();

    let mut simple_layer = SimpleLayer {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,

        exit: false,
        first_configure: true,
        glyph_cache: glyph_cache,
        pool,
        width: 256,
        height: 256,
        layer
    };

    // We don't draw immediately, the configure will notify us when to first draw.
    loop {
        event_queue.blocking_dispatch(&mut simple_layer).unwrap();

        if simple_layer.exit {
            println!("exiting example");
            break;
        }
    }
}

struct SimpleLayer {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,

    exit: bool,
    first_configure: bool,
    pool: SlotPool,
    glyph_cache: GlyphCache,
    width: u32,
    height: u32,
    layer: LayerSurface
}

impl CompositorHandler for SimpleLayer {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) { }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) { }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) { }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) { }
}

impl OutputHandler for SimpleLayer {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        println!("New output has arrived")
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) { }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) { }
}

impl LayerShellHandler for SimpleLayer {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(256, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(256, NonZeroU32::get);

        // Initiate the first draw.
        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

impl ShmHandler for SimpleLayer {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SimpleLayer {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;
        let stride = self.width * 4;

        // Don't create a new buffer each time, only when output is resized
        let (buffer, canvas) = self.pool
            .create_buffer(width as i32, height as i32, stride as i32, wl_shm::Format::Argb8888)
            .expect("Create buffer");

        self.layer.set_exclusive_zone(height as i32);

        {
            canvas.chunks_exact_mut(4).enumerate().for_each(|(_, chunk)| {
                let a = 0xFF;
                let r = 0;
                let g = 0;
                let b = 0;
                let color: u32 = (a << 24) + (r << 16) + (g << 8) + b;

                let array: &mut [u8; 4] = chunk.try_into().unwrap();
                *array = color.to_le_bytes();
            });
        }

        let kearning = 10;
        let mut pen_x = 0;
        for c in 'a'..='z' {
            let (metrics, bitmap) = self.glyph_cache.get_or_rasterize(c);
            
            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let v = bitmap[y * metrics.width + x];
                    if v == 0 { continue; }

                    let px = y * stride as usize + ((x + pen_x) * 4);
                    canvas[px..px + 4].copy_from_slice(&[v, v, v, 0xFF]);
                }
            }

            pen_x += metrics.width + kearning;
        }


        // We shouldn't damage the entire window
        // Damage the entire window
        self.layer.wl_surface().damage_buffer(0, 0, width as i32, height as i32);

        // Request our next frame
        self.layer.wl_surface().frame(qh, FrameCallbackData(self.layer.wl_surface().clone()));

        // Attach and commit to present.
        buffer.attach_to(self.layer.wl_surface()).expect("buffer attach");
        self.layer.commit();

        // TODO save and reuse buffer when the window size is unchanged.  This is especially
        // useful if you do damage tracking, since you don't need to redraw the undamaged parts
        // of the canvas.
    }
}

struct GlyphCache {
    map: std::collections::HashMap<char, (Metrics, Vec<u8>)>,
    px: f32,
    font: Font
}

impl GlyphCache {
    fn new(font_path: &str, px: f32) -> Result<Self, &'static str> {
        let file = std::fs::read(font_path)
            .map_err(|_| { "Unable to read file" })?;

        Ok(GlyphCache { 
            map: std::collections::HashMap::new(),
            px: px,
            font: Font::from_bytes(file, FontSettings::default())?
        })   
    }

    fn get_or_rasterize(&mut self, character: char) -> &(Metrics, Vec<u8>) {
        self.map.entry(character)
            .or_insert(self.font.rasterize(character, self.px))
    }
}

delegate_registry!(SimpleLayer);

impl ProvidesRegistryState for SimpleLayer {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(SimpleLayer);
