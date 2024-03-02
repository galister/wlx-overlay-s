# WlxOverlay-S

A lightweight OpenXR/OpenVR overlay for Wayland and X11 desktops, inspired by XSOverlay.

WlxOverlay-S allows you to access your desktop screens while in VR.

Compared to similar software, WlxOverlay-S aims to run alongside your other VR games or experiences and have as little performance impact as possible. The UI looks and rendering methods are kept simple and efficient as much as possible.

This is the coming-together of two of my previous projects:
- [WlxOverlay](https://github.com/galister/WlxOverlay) (SteamVR overlay written in C#)
- [WlxOverlay-X](https://github.com/galister/wlx-overlay-x) (OpenXR overlay using StereoKit, written in Rust)

# Join the Linux VR Community

We are available on either:
- Discord: https://discord.gg/gHwJ2vwSWV
- Matrix Space: `#linux-vr-adventures:matrix.org`

Questions/issues specific to WlxOverlay-S will be handled in the `wlxoverlay` chat room.

# Setup

1. Grab the latest AppImage from [Releases](https://github.com/galister/wlx-overlay-s/releases).
1. `chmod +x WlxOverlay-S-*.AppImage`
1. Start Monado or SteamVR.
1. Run the overlay

AUR package is [wlx-overlay-s-git](https://aur.archlinux.org/packages/wlx-overlay-s-git).

You may also want to [build from source](https://github.com/galister/wlx-overlay-s/wiki/Building-from-Source).

# First Start

**If you get a screen share pop-up, check the terminal and select the screens in the order it tells you to.**

SteamVR users: WlxOverlay-S will register itself for auto-start, so you will not need to start it every time.

**Please continue reading the guide below.**

# Getting Started

### Pointer Modes AKA Laser Colors

Much of the functionality in WlxOverlay-S depends on what color of laser you are using to interact with a UI element. \
Using the default settings, there are 3 modes:
- Regular Mode: Blue laser
- Right-click Mode: Orange laser
- Middle-click Mode: Purple laser

Please see the bindings section below on how to activate these modes.

The guide here uses the colors for ease of getting started.

### The Watch

Check your left wrist for the watch. The watch is your primary tool for controlling the app.

![Watch usage guide](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-watch.png)

### The Screens

Hovering a pointer over a screen will move the mouse. If there are more than one pointers hovering a screen, the pointer that was last used to click will take precedence.

The click depends on the laser color:
- Blue laser: Left click
- Orange laser: Right click
- Purple laser: Middle click
- Stick up/down: Scroll wheel

See the bindings section on how to grab, move and resize screens.

### The keyboard

The keyboard is fully customizable via the [keyboard.yaml](https://raw.githubusercontent.com/galister/wlx-overlay-s/main/src/res/keyboard.yaml) file. \
Download it into your `~/.config/wlxoverlay/` folder and edit it to your liking.

Typing
- Use the BLUE laser when typing regularly.
- While using ORANGE laser, all keystrokes will have SHIFT applied.
- Purple laser has no effect as of now.

**Modifier Keys** are sticky. They will remain pressed until you press a non-modifier key, or toggle them off.

### Default Bindings

![Index Controller Bindings](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-index.png)

![Touch Controller Bindings](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-oculus.png)

If your bindings are not supported, please reach out. \
We would like to work with you and include additional bindings.

# Known Issues

## SteamVR: laser pointers not visible

This seems to be a rare issue with SteamVR startup overlays. Restarting the overlay will fix the issue.

## Scroll wheel doesn't work

This seems to be an issue specific to Electron apps (Discord, Element, Slack, Spotify) on Wayland. Scrolling will work when using these in your web browser.

## WiVRn support

While WiVRn technically supports EXTX_overlay, I do not recommend using this software with WiVRn at this time, due to WiVRn not being optimized for overlay apps. You will likely get ghosting or stuttering while rotating your head.

## X11 limitations

DPI scaling and upright screens are not supported on X11. These might display incorrectly or mess up your mouse position.
