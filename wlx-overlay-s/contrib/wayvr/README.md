<p align="center">
  <img src="https://raw.githubusercontent.com/galister/wlx-overlay-s/refs/heads/guide/wayvr/logo.svg" height="120"/>
</p>

# Overview

### Features

- Display Wayland applications without GPU overhead (zero-copy via dma-buf)
- Mouse and keyboard input, with precision scrolling support
- Tested on AMD and Nvidia

### Supported software

- Basically all Qt and GTK applications (they work out of the box)
- Most XWayland applications via `cage`

### Launching external apps inside WayVR

To launch your app externally:

```sh
DISPLAY= WAYLAND_DISPLAY=wayland-$(cat $XDG_RUNTIME_DIR/wayvr.disp) yourapp
```

or (in the most cases):

```
DISPLAY= WAYLAND_DISPLAY=wayland-20 yourapp
```

Setting `DISPLAY` to an empty string forces various apps to use Wayland instead of X11.

# Troubleshooting

### My application doesn't launch but others do!

Even though some applications support Wayland, some still check for the `DISPLAY` environment variable and an available X11 server, throwing an error. This can also be fixed by running `cage` on top of them.

### Floating windows

Context menus are not functional in most cases yet, including drag & drop support.
