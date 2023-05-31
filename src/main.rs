use std::{fs::File, io::Write, os::unix::prelude::AsRawFd};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
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

    cursor_size: u32,

    compositor: Option<wl_compositor::WlCompositor>,
    base_surface: Option<wl_surface::WlSurface>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    buffer: Option<wl_buffer::WlBuffer>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    pointer: Option<wl_pointer::WlPointer>,
}

fn main() {
    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        running: true,
        cursor_size: 10,
        compositor: None,
        base_surface: None,
        layer_shell: None,
        layer_surface: None,
        buffer: None,
        wm_base: None,
        pointer: None,
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

                let (init_w, init_h) = (state.cursor_size, state.cursor_size);

                let mut file = tempfile::tempfile().unwrap();
                draw(&mut file, (init_w, init_h));
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
            } else if interface == wl_seat::WlSeat::interface().name {
                let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ());
                state.pointer = Some(seat.get_pointer(qh, ()));
            } else if interface == xdg_wm_base::XdgWmBase::interface().name {
                let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                state.wm_base = Some(wm_base);
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        eprintln!("WlPointer event {event:#?}");
        match event {
            wl_pointer::Event::Enter { .. } => {
                if let Some(surface) = &state.base_surface {
                    surface.destroy();
                }
                state.base_surface = None;
            }
            wl_pointer::Event::Leave { .. } => {
                let surface = state.compositor.as_ref().unwrap().create_surface(qh, ());
                state.base_surface = Some(surface);

                state.init_layer_surface(qh);
            }
            _ => {}
        }
    }
}

fn draw(tmp: &mut File, (buf_x, buf_y): (u32, u32)) {
    let mut buf = std::io::BufWriter::new(tmp);
    for y in 0..buf_y {
        for x in 0..buf_x {
            let ix = x as i32;
            let iy = y as i32;

            let dist = if x <= (buf_x / 2) {
                ix + iy - (buf_y as i32)
            } else {
                iy - ix
            };

            let a: u32 = match dist.abs() {
                0 => 0xFF,
                1 => 0x88,
                _ => 0x00,
            };

            let c: u32 = match dist.abs() {
                0 => 0xFF,
                1 => 0x88,
                _ => 0x00,
            };

            let color = (a << 24) + (c << 16) + (c << 8) + c;
            buf.write_all(&color.to_ne_bytes()).unwrap();
        }
    }
    buf.flush().unwrap();
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
        layer.set_size(self.cursor_size, self.cursor_size);
        // A negative value means we will be centered on the screen
        // independently of any other xdg_layer_shell
        layer.set_exclusive_zone(-1);
        self.base_surface.as_ref().unwrap().commit();

        self.layer_surface = Some(layer);
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
