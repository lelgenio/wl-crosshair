use std::{fs::File, io::Write, os::unix::prelude::AsRawFd};

use image::{GenericImageView, Pixel};
use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_region::WlRegion, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, Proxy, QueueHandle,
};

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer},
    zwlr_layer_surface_v1::{self, Anchor},
};

use wayland_protocols::xdg::shell::client::xdg_wm_base;

struct State {
    running: bool,

    cursor_width: u32,
    cursor_height: u32,
    image_path: String,

    compositor: Option<wl_compositor::WlCompositor>,
    base_surface: Option<wl_surface::WlSurface>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    buffer: Option<wl_buffer::WlBuffer>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
}

fn get_cursor_image_path() -> String {
    if let Some(p) = std::env::args().skip(1).next() {
        return p;
    }

    if let Ok(p) = std::env::var("WL_CROSSHAIR_IMAGE_PATH") {
        return p;
    }

    [
        std::option_env!("WL_CROSSHAIR_IMAGE_PATH").map(String::from),
        Some("cursors/inverse-v.png".to_string()),
    ]
      .into_iter()
      .flatten()
      .filter(|p|
          std::fs::metadata(p)
              .map(|m| m.is_file())
              .unwrap_or(false)
      )
      .next()
      .expect("Could not find a crosshair image, pass it as a cli argument or set WL_CROSSHAIR_IMAGE_PATH environment variable")
}

fn main() {
    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        running: true,
        cursor_width: 10,
        cursor_height: 10,
        image_path: get_cursor_image_path(),
        compositor: None,
        base_surface: None,
        layer_shell: None,
        layer_surface: None,
        buffer: None,
        wm_base: None,
    };

    event_queue.blocking_dispatch(&mut state).unwrap();

    if state.layer_shell.is_some() && state.wm_base.is_some() {
        state.init_layer_surface(&qhandle);
    }

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        eprintln!("WlRegistry event {event:#?}");
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == zwlr_layer_shell_v1::ZwlrLayerShellV1::interface().name {
                let wl_layer = registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                    name,
                    version,
                    qh,
                    (),
                );
                state.layer_shell = Some(wl_layer);
            } else if interface == wl_compositor::WlCompositor::interface().name {
                let compositor =
                    registry.bind::<wl_compositor::WlCompositor, _, _>(name, version, qh, ());
                let surface = compositor.create_surface(qh, ());
                state.base_surface = Some(surface);
                state.compositor = Some(compositor);
            } else if interface == wl_shm::WlShm::interface().name {
                let shm = registry.bind::<wl_shm::WlShm, _, _>(name, version, qh, ());

                let mut file = tempfile::tempfile().unwrap();
                state.draw(&mut file);

                let (init_w, init_h) = (state.cursor_width, state.cursor_height);

                let pool = shm.create_pool(file.as_raw_fd(), (init_w * init_h * 4) as i32, qh, ());
                let buffer = pool.create_buffer(
                    0,
                    init_w as i32,
                    init_h as i32,
                    (init_w * 4) as i32,
                    wl_shm::Format::Argb8888,
                    qh,
                    (),
                );
                state.buffer = Some(buffer);
            } else if interface == xdg_wm_base::XdgWmBase::interface().name {
                let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                state.wm_base = Some(wm_base);
            }
        }
    }
}

impl Dispatch<WlRegion, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlRegion,
        _: <WlRegion as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl State {
    fn init_layer_surface(&mut self, qh: &QueueHandle<State>) {
        let layer = self.layer_shell.as_ref().unwrap().get_layer_surface(
            self.base_surface.as_ref().unwrap(),
            None,
            Layer::Overlay,
            "crosshair".to_string(),
            qh,
            (),
        );
        // Center the window
        layer.set_anchor(Anchor::Top | Anchor::Right | Anchor::Bottom | Anchor::Left);
        layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        layer.set_size(self.cursor_width, self.cursor_height);
        // A negative value means we will be centered on the screen
        // independently of any other xdg_layer_shell
        layer.set_exclusive_zone(-1);
        // Set empty input region to allow clicking through the window.
        if let Some(compositor) = &self.compositor {
            let region = compositor.create_region(qh, ());
            self.base_surface
                .as_ref()
                .unwrap()
                .set_input_region(Some(&region));
        }
        self.base_surface.as_ref().unwrap().commit();

        self.layer_surface = Some(layer);
    }

    fn draw(&mut self, tmp: &mut File) {
        let mut buf = std::io::BufWriter::new(tmp);

        let i = image::open(&self.image_path).unwrap();

        self.cursor_width = i.width();
        self.cursor_height = i.height();

        for y in 0..self.cursor_height {
            for x in 0..self.cursor_width {
                let px = i.get_pixel(x, y).to_rgba();

                let [r, g, b, a] = px.channels().try_into().unwrap();

                let color = u32::from_be_bytes([a, r, g, b]);

                buf.write_all(&color.to_le_bytes()).unwrap();
            }
        }
        buf.flush().unwrap();
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        eprintln!("ZwlrLayerSurfaceV1 event {event:#?}");
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            surface.ack_configure(serial);
            if let (Some(surface), Some(buffer)) = (&state.base_surface, &state.buffer) {
                surface.attach(Some(buffer), 0, 0);
                surface.commit();
            }
        }
    }
}

// ignored

macro_rules! impl_dispatch_log {
    ($DispatchStruct: path) => {
        impl Dispatch<$DispatchStruct, ()> for State {
            fn event(
                _: &mut Self,
                _: &$DispatchStruct,
                event: <$DispatchStruct as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                eprintln!("{} event {:#?}", stringify!($DispatchStruct), event);
            }
        }
    };
}

impl_dispatch_log!(wl_buffer::WlBuffer);
impl_dispatch_log!(wl_compositor::WlCompositor);
impl_dispatch_log!(wl_keyboard::WlKeyboard);
impl_dispatch_log!(wl_seat::WlSeat);
impl_dispatch_log!(wl_shm_pool::WlShmPool);
impl_dispatch_log!(wl_shm::WlShm);
impl_dispatch_log!(wl_surface::WlSurface);
impl_dispatch_log!(xdg_wm_base::XdgWmBase);
impl_dispatch_log!(zwlr_layer_shell_v1::ZwlrLayerShellV1);
