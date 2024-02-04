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

Right click: Touch (do not push down) the B/Y button on your controller to get a ORANGE laser, and pull the trigger.

Middle click: Same as right click, but A/X to get a PURPLE laser.

Move screen: Point your laser on the screen and then grab using grip. Adjust distance using stick up/down while gripping.

Resize screen: While grabbing, pull trigger to get a RED laser and use stick up/down

Reset size/position: Click the button corresponding to the screen or keyboard on your watch, hold for 3s, then release.

Show/hide: Quickly hide or show your selection of screens by double-tapping B/Y on your left controller.

Lock a screen in place: On your non-watch hand, touch B/Y to get an ORANGE laser and click the screen's button on your watch. You will no longer be able to grab the screen, it will not re-center in front of you when show, nor it will react to the show/hide shortcut.

Make a screen non-clickable: On your non-watch hand, touch A/X to get a PURPLE laser and click the screen's button on your watch. You will no longer get a laser when pointing to that screen. Repeat to toggle back off.

# Known Issues

While WiVRn technically supports EXTX_overlay, I do not recommend using this software with WiVRn at this time, due to WiVRn not being optimized for overlay apps. You will likely get ghosting or stuttering while rotating your head.
