# WlxOverlay-S

A lightweight OpenXR/OpenVR overlay for Wayland and X11 desktops, inspired by XSOverlay.

WlxOverlay-S lets you to access your desktop screens while in VR.

In comparison to similar overlays, WlxOverlay-S aims to run alongside VR games and experiences while having as little performance impact as possible. The UI appearance and rendering techniques are kept as simple and efficient as possible, while still allowing a high degree of customizability.

![Screenshot of WlxOverlay-S being used as an OpenXR home environment](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-s.png?raw=true)

## Join the Linux VR Community

We are available on either:

- Discord: <https://discord.gg/gHwJ2vwSWV>
- Matrix Space: `#linux-vr-adventures:matrix.org`

Questions/issues specific to WlxOverlay-S will be handled in the `wlxoverlay` chat room.

## Setup

1. Grab the latest AppImage from [Releases](https://github.com/galister/wlx-overlay-s/releases).
1. `chmod +x WlxOverlay-S-*.AppImage`
1. Start Monado, WiVRn or SteamVR.
1. Run the overlay

AUR package is [wlx-overlay-s-git](https://aur.archlinux.org/packages/wlx-overlay-s-git).

You may also want to [build from source](https://github.com/galister/wlx-overlay-s/wiki/Building-from-Source).

## First Start

**When the screen share pop-up appears, check the terminal and select the screens in the order it requests.**

In case screens were selected in the wrong order:

- `rm ~/.config/wlxoverlay/conf.d/pw_tokens.yaml` then restart

**SteamVR users**: WlxOverlay-S will register itself for auto-start, so there is no need to start it every time.

**Envision users**: Set `wlx-overlay-s --openxr --show` as the _Autostart Command_ on your Envision profile! This will show a home environment with a [customizable background](https://github.com/galister/wlx-overlay-s/wiki/OpenXR-Skybox)!

**Please continue reading the guide below.**

## Getting Started

### Working Set

The working set consists of all currently selected overlays; screens, mirrors, keyboard, etc.

The working set appears in front of the headset when shown, and can be re-centered by hiding and showing again.

Show and hide the working set using:

- Non-vive controller: double-tap B or Y on the left controller.
- Vive controller: double-tap the menu button on the left controller (for SteamVR, the `showhide` binding must be bound)

### Pointer Modes AKA Laser Colors

Much of the functionality in WlxOverlay-S depends on what color of laser is used to interact with a UI element. \
Using the default settings, there are 3 modes:

- Regular Mode: Blue laser
- Right-click Mode: Orange laser
- Middle-click Mode: Purple laser

Please see the bindings section below on how to activate these modes.

The guide here uses the colors for ease of getting started.

### The Watch

Check your left wrist for the watch. The watch is the primary tool for controlling the app.

![Watch usage guide](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-watch.png)

### The Screens

Hovering a pointer over a screen will move the mouse. If there are more than one pointers hovering a screen, the pointer that was last used to click will take precedence.

The click depends on the laser color:

- Blue laser: Left click
- Orange laser: Right click
- Purple laser: Middle click
- Stick up/down: Scroll wheel

To **curve a screen**, grab it with one hand. Then, using the other hand, hover the laser over the screen and use the scroll action.

See the [bindings](#default-bindings) section on how to grab, move and resize screens.

### The keyboard

The keyboard is fully customizable via the [keyboard.yaml](https://raw.githubusercontent.com/galister/wlx-overlay-s/main/src/res/keyboard.yaml) file. \
Download it into the `~/.config/wlxoverlay/` folder and edit it to your liking.

Typing

- Use the BLUE laser when typing regularly.
- While using ORANGE laser, all keystrokes will have SHIFT applied.
- Purple laser has no effect as of now.

**Modifier Keys** are sticky. They will remain pressed until a non-modifier key is pressed, the modifier gets toggled off, or the keyboard gets hidden.

### Default Bindings

![Index Controller Bindings](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-index.png)

![Touch Controller Bindings](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-oculus.png)

To customize bindings on OpenXR, refer to the [OpenXR Bindings wiki page](https://github.com/galister/wlx-overlay-s/wiki/OpenXR-Bindings).

If your bindings are not supported, please reach out. \
We would like to work with you and include additional bindings.

## Troubleshooting

When an error is detected, we often print tips for fixing into the log file.

Logs will be at `/tmp/wlx.log` for most distros.

Check [here](https://github.com/galister/wlx-overlay-s/wiki/Troubleshooting) for tips.

## Known Issues

### Mouse is not where it should be

Hyprland users: Hyprland v0.41.0 changed their absolute input implementation to one that does not respect existing absolute input standards. Make your voice heard: [Hyprland#6023](https://github.com/hyprwm/Hyprland/issues/6023)・[Hyprland#6889](https://github.com/hyprwm/Hyprland/issues/6889)

Niri users: use on Niri 0.1.7 or later.

X11 users might be dealing with a [Phantom Monitor](https://wiki.archlinux.org/title/Xrandr#Disabling_phantom_monitor).

Other desktops: The screens may have been selected in the wrong order, see [First Start](#first-start).

### Crashes, blank screens

There are some driver-desktop combinations that don't play nice with DMA-buf capture. 

Disabling DMA-buf capture is a good first step to try when encountering an app crash or gpu driver reset.

```bash
echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml
```

Without DMA-buf capture, capturing screens takes CPU power, so let's try and not show too many screens at the same time.

### Space-drag crashes SteamVR

This has been idenfitied as an issue with SteamVR versions 2.5.5 and above (latest tested 2.7.2). One way to avoid the crash is by switching to the `temp-v1.27.5` branch of SteamVR (via beta selection) and selecting [Steam-Play-None](https://github.com/Scrumplex/Steam-Play-None) under the compatibility tab.

### Modifiers get stuck in weird ways

This is a rare issue that can make KDE Plasma not react to click or keys due to what seems to be a race condition with modifiers. Restarting the overlay fixes this.

### X11 limitations

- X11 capture can generally seem slow. This is because zero-copy GPU capture is not supported on the general X11 desktop. Consider trying Wayland or Picom.
- DPI scaling is not supported and may cause the mouse to not follow the laser properly.
- Upright screens are not supported and can cause the mouse to act weirdly.
- Screen changes (connecting / disconnecting a display, resolution changes, etc) are not handled at runtime. Restart the overlay for these to take effect.
