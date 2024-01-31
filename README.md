# WlxOverlay-S

A lightweight OpenXR/OpenVR overlay for Wayland desktops, inspired by XSOverlay.

This is the coming-together of two of my previous projects:
- [WlxOverlay](https://github.com/galister/WlxOverlay) (SteamVR overlay written in C#)
- [WlxOverlay-X](https://github.com/galister/wlx-overlay-x) (OpenXR overlay using StereoKit, written in Rust)

# What does WlxOverlay-S do?

Simply put, this program allows you to access your desktop screens while in VR.

Compared to similar software, WlxOverlay-S aims to run alongside your other VR games or experiences and have as little performance impact as possible. The UI looks and rendering methods are kept simple and efficient as much as possible.

# Under Development

This project is in a highly experimental state. If you would like to give this a go, you might want to talk to us first.

We are available under the `wlxoverlay` chat room on either:
- Discord: https://discord.gg/gHwJ2vwSWV
- Matrix Space: `#linux-vr-adventures:matrix.org`

# Usage

Recommend grabbing [rustup](https://rustup.rs/) if you don't have it yet.

Start Monado or SteamVR.

```sh
cargo run --release
```

**If you get a screen share pop-up, check the terminal and select the screens in the order it tells you to.**

You'll see a screen and keyboard. You can turn these on and off using the watch on your left wrist.

Right click: Touch (do not push down) the B/Y button on your controller to get a YELLOW laser, and pull the trigger.

Middle click: Same as right click, but A/X to get a PURPLE laser.

Move screen: Point your laser on the screen and then grab using grip. Adjust distance using stick up/down while gripping.

Resize screen: While grabbing, touch B/Y to get a YELLOW laser and use stick up/down (this is wonky, I'll come up with a better one!)

# Known Issues

While WiVRn technically supports EXTX_overlay, I do not recommend using this software with WiVRn at this time, due to WiVRn not being optimized for overlay apps. You will likely get ghosting or stuttering while rotating your head.
