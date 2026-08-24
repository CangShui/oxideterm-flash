// OxideTerm modification: falls back to X11 when Wayland startup requirements are unavailable.

mod dispatcher;
mod headless;
mod keyboard;
mod platform;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod text_system;
#[cfg(feature = "wayland")]
mod wayland;
#[cfg(feature = "x11")]
mod x11;

#[cfg(any(feature = "wayland", feature = "x11"))]
mod xdg_desktop_portal;

pub use dispatcher::*;
pub(crate) use headless::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) use text_system::*;
#[cfg(feature = "wayland")]
pub(crate) use wayland::*;
#[cfg(feature = "x11")]
pub(crate) use x11::*;

use std::rc::Rc;

#[cfg(feature = "wayland")]
fn wayland_platform() -> anyhow::Result<Rc<dyn gpui::Platform>> {
    Ok(Rc::new(LinuxPlatform {
        inner: WaylandClient::new()?,
    }))
}

#[cfg(feature = "x11")]
fn x11_platform() -> anyhow::Result<Rc<dyn gpui::Platform>> {
    Ok(Rc::new(LinuxPlatform {
        inner: X11Client::new()?,
    }))
}

/// Returns the default platform implementation for the current OS.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    if headless {
        return Rc::new(LinuxPlatform {
            inner: HeadlessClient::new(),
        });
    }

    match gpui::guess_compositor() {
        #[cfg(feature = "wayland")]
        "Wayland" => match wayland_platform() {
            Ok(platform) => platform,
            Err(wayland_error) => {
                #[cfg(feature = "x11")]
                {
                    log::warn!(
                        "Wayland initialization failed; falling back to X11: {wayland_error:#}"
                    );
                    x11_platform().unwrap_or_else(|x11_error| {
                        panic!(
                            "Failed to initialize either Linux display backend. Wayland: \
                             {wayland_error:#}. X11: {x11_error:#}"
                        )
                    })
                }

                #[cfg(not(feature = "x11"))]
                panic!(
                    "Failed to initialize Wayland and X11 support is not compiled in: \
                     {wayland_error:#}"
                )
            }
        },

        #[cfg(feature = "x11")]
        "X11" => x11_platform()
            .unwrap_or_else(|error| panic!("Failed to initialize X11 client: {error:#}")),

        "Headless" => Rc::new(LinuxPlatform {
            inner: HeadlessClient::new(),
        }),
        _ => unreachable!(),
    }
}
