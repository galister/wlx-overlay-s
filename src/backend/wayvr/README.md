**WayVR acts as a bridge between Wayland applications and wlx-overlay-s panels, allowing you to display your applications within a VR environment. Internally, WayVR utilizes Smithay to run a Wayland compositor.**

# Features

- Display Wayland applications without GPU overhead (zero-copy via dma-buf)
- Mouse input
- Precision scrolling support
- XWayland "support" via `cage`

# Supported hardware

### Confirmed working GPUs

- Navi 32 family: AMD Radeon RX 7800 XT **\***
- Navi 23 family: AMD Radeon RX 6600 XT
- Navi 21 family: AMD Radeon Pro W6800, AMD Radeon RX 6800 XT
- Nvidia GTX 16 Series
- _Your GPU here? (Let us know!)_

**\*** - With dmabuf modifier mitigation (probably Mesa bug)

# Supported software

- Basically all Qt applications (they work out of the box)
- Most XWayland applications via `cage`

# Known issues

- Context menus are not functional in most cases yet

- Due to unknown circumstances, dma-buf textures may display various graphical glitches due to invalid dma-buf tiling modifier. Please report your GPU model when filing an issue. Alternatively, you can run wlx-overlay-s with `LIBGL_ALWAYS_SOFTWARE=1` to mitigate that (only the Smithay compositor will run in software renderer mode, wlx will still be accelerated).

- Potential data race in the rendering pipeline - A texture could be displayed during the clear-and-blit process in the compositor, causing minor artifacts (no fence sync support yet).

- Even though some applications support Wayland, some still check for the `DISPLAY` environment variable and an available X11 server, throwing an error. This can be fixed by running `cage`.

- GNOME still insists on rendering client-side decorations instead of server-side ones. This results in all GTK applications looking odd due to additional window shadows. [Fix here, "Client-side decorations"](https://wiki.archlinux.org/title/GTK)
