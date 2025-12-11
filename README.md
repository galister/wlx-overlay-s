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

### Installation

There are multiple ways to install WlxOverlay-S:
1. AppImage: Download from [Releases](https://github.com/galister/wlx-overlay-s/releases)
1. AUR package: [wlx-overlay-s-git](https://aur.archlinux.org/packages/wlx-overlay-s-git)
1. Homebrew:
  1. Add AtomicXR tap: `brew tap matrixfurry.com/atomicxr https://tangled.sh/@matrixfurry.com/homebrew-atomicxr`
  1. Install WlxOverlay-S: `brew install wlx-overlay-s`
1. [Building from source](https://github.com/galister/wlx-overlay-s/wiki/Building-from-Source).

### General Setup

1. Start Monado, WiVRn or SteamVR.
1. Run the overlay

**Note:** If you are using Monado or WiVRn, no additional setup steps are required for Flatpak Steam compatibility—most people use WlxOverlay-S seamlessly with Monado/WiVRn.

### SteamVR via Steam Flatpak

For users specifically running **SteamVR via Steam Flatpak**, follow these steps:

1. Grab the latest AppImage from [Releases](https://github.com/galister/wlx-overlay-s/releases).
1. `WlxOverlay-S-*.AppImage --appimage-extract`
1. `chmod +x squashfs-root/AppRun`
1. Move the newly created `squashfs-root` folder to a location accessible by the Steam Flatpak.
1. `flatpak override com.valvesoftware.Steam --user --filesystem=xdg-run/pipewire-0/:rw`
1. Restart Steam.
1. Start SteamVR.
1. `flatpak run --command='/path/to/squashfs-root/AppRun' com.valvesoftware.Steam`


## First Start

**When the screen share pop-up appears, check your notifications or the terminal and select the screens in the order it requests.**

In case screens were selected in the wrong order:

- `rm ~/.config/wlxoverlay/conf.d/pw_tokens.yaml` then restart


**WiVRn users**: Select WlxOverlay-S from the `Application` drop-down. If there's no such entry, select `Custom` and browse to your WlxOverlay-S executable or AppImage.

**Envision users**: Go to the Plugins menu and select the WlxOverlay-S plugin. This will download and run the AppImage version of the overlay.
In order to run a standalone installation (for instance from the AUR), create a bash script containing `wlx-overlay-s --openxr --show` and then set this bash script as a custom Envision plugin.

This will show a home environment with headset passthrough by default or a [customizable background](https://github.com/galister/wlx-overlay-s/wiki/OpenXR-Skybox)!

**SteamVR users**: WlxOverlay-S will register itself for auto-start, so there is no need to start it every time. Disclaimer: SteamVR will sometimes disregard this and not start Wlx anyway.

**Please continue reading the guide below.**

## Getting Started

### Working Set

The working set consists of all currently selected overlays; screens, mirrors, keyboard, etc.

The working set appears in front of the headset when shown, and can be re-centered by hiding and showing again.

Show and hide the working set using:
- Non-vive controller: double-tap B or Y on the left controller.
- Vive controller: double-tap the menu button on the left controller (for SteamVR, the `showhide` binding must be bound)

See the [bindings](#default-bindings) section on how to grab, move and resize overlay windows.

### Pointer Modes AKA Laser Colors

Much of the functionality in WlxOverlay-S depends on what color of laser is used to interact with a UI element. \
Using the default settings, there are 3 modes:

- Regular Mode: Blue laser
- Right-click Mode: Orange laser
- Middle-click Mode: Purple laser

Please see the bindings section below on how to activate these modes.

The guide here uses the colors for ease of getting started.

### The watch

Check your left wrist for the watch. The watch is the primary tool for controlling the app.

The top of the watch shows device batteries, and the bottom shows your overlay controls.

Enter edit mode (leftmost button on bottom) to edit your overlay sets.

While in edit mode, the watch can also be grabbed, and passed between your hands.

After grabbing, the watch will automatically attach to the hand that's opposite from the one that held it.

In edit mode, try hovering other overlays to see their advanced options!

![Watch usage guide](https://github.com/galister/wlx-overlay-s/blob/guide/wlx-watch.png)

### The screens

Hovering a pointer over a screen will move the mouse. If there are more than one pointers hovering a screen, the pointer that was last used to click will take precedence.

The click type depends on the laser color:

- Blue laser: Left click
- Orange laser: Right click
- Purple laser: Middle click
- Stick up/down: Scroll wheel

### The keyboard

The keyboard is fully customizable via the [keyboard.yaml](https://raw.githubusercontent.com/galister/wlx-overlay-s/main/src/res/keyboard.yaml) file. \
Download it into the `~/.config/wlxoverlay/` folder and edit it to your liking.

Typing

- Use the BLUE laser when typing regularly.
- While using ORANGE laser, all keystrokes will have SHIFT applied.
- Purple laser is customizable via the `keyboard.yaml`'s `alt_modifier` settings.

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

X11 users:
- Might be dealing with a [Phantom Monitor](https://wiki.archlinux.org/title/Xrandr#Disabling_phantom_monitor).
- DPI scaling is not supported and will mess with the mouse.
- Upright screens are not supported and will mess with the mouse.

Other desktops: The screens may have been selected in the wrong order, see [First Start](#first-start).

### Crashes, blank screens

There are some driver-desktop combinations that don't play nice with DMA-buf capture. 

Disabling DMA-buf capture is a good first step to try when encountering an app crash or gpu driver reset.

```bash
echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml
```

Without DMA-buf capture, capturing screens takes CPU power, so let's try and not show too many screens at the same time.

### Modifiers get stuck, mouse clicks stop working on KDE Plasma

We are not sure what causes this, but it only happens on KDE Plasma. Restarting the overlay fixes this.

### X11 limitations

- X11 capture can generally seem slow. This is because zero-copy GPU capture is not supported on the general X11 desktop. Consider trying Wayland.
- DPI scaling is not supported and may cause the mouse to not follow the laser properly.
- Upright screens are not supported and can cause the mouse to not follow the laser properly.
- Screen changes (connecting / disconnecting a display, resolution changes, etc) are not handled at runtime. Restart the overlay for these to take effect.
