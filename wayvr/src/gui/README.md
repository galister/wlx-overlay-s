# Custom UI Elements

Elements on custom panels may be modified at runtime using `wayvrctl`.

For more, refer to: `wayvrctl panel-modify --help`

### Labels

#### Clock label

Clock labels are driven by the current time, adhering to the user's 12/24 hour setting as well as timezone settings.

Available display values are: `name` (timezone name), `time`, `date`, `dow` or a custom [format string](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) like `%H:%M:%S`.

See the Custom Timezones section for more info on timezones. Skip `_timezone` to use local time.

```xml
<label _source="clock" _display="time" _timezone="0" [...] />
```

#### Timer label

Instead of a clock, this label shows the amount of time since program start, (aka time in VR).

Use `_format` to arrange `%h` hours, `%m` minutes, and `%s` seconds.

```xml
<label _source="timer" _format="%h:%m:%s" [...] />
```

#### Battery label

This is a label type that's used internally to display battery states.

```xml
<label _source="battery" _device="0" [...] />
```

#### IPD

Displays IPD value in millimeters. Not parametrizable.

Format: `ipd`

```xml
<label _source="ipd" [...] />
```

### Buttons

Buttons consist of a label component and one or more actions to handle press and/or release events.

Supported events:

```xml
<button _press="..." _release="..." />
```

Laser-color-specific variants are also available
- `_press_left` & `_release_left` for blue laser
- `_press_right` & `_release_right` for orange laser
- `_press_middle` & `_release_middle` for purple laser

Release after short/long press (length controlled by config `long_press_duration`)
- `_short_release` & `_long_release` for any laser
- `_short_release_left` & `_long_release_left` for blue laser
- `_short_release_right` & `_long_release_right` for orange laser
- `_short_release_middle` & `_long_release_middle` for purple laser

#### Supported button actions

##### `::ShellExec <command> [args ..]`

This button action executes a shell script using the `sh` shell.

- Long-running processes are allowed, but a new execution will not be triggered until the previous process has exited.
- If triggered again while the previous process is still running, SIGUSR1 will be sent to that child process.

```xml
<button _press="::ShellExec $HOME/myscript.sh test-argument" [...] />
```

##### `::OscSend <path>` `::OscSend <path> <args ..>`

Send an OSC message. The target port comes from the `osc_out_port` configuration setting.

There are two formats; here is an example for both formats writing a message to the VRChat Chatbox:
```xml
<!-- parameter form - OSC arguments are listed as parameters labelled `_arg<n>` where `n` is 0-indexed -->
<Button _press="::OscSend /chatbox/input" _arg0="Hello World! I am WayVR." _arg1="true" _arg2="true"> </Button>
<!-- will send: ("Hello World! I am WayVR.", True, True) to /chatbox/input -->
```

```xml
<!-- shorthand form - OSC arguments are space-separated in one string. note that single strings cannot contain spaces -->
<Button _press="::OscSend /chatbox/input Hello_World!_I_am_WayVR. true true"> </Button>
<!-- will send: ("Hello_World!_I_am_WayVR.", True, True) to /chatbox/input -->
```

The two can be combined; parameter-form arguments will be appended after shorthand-form arguments:
```xml
<!-- combined-form - rectangle bounds with a name -->
<Button _press="::OscSend /graphthing/rectangle 0i32 0i32 50i32 200i32" _arg0="tall rectangle"> </Button>
<!-- will send: (0, 0, 50, 200, "tall rectangle") to /graphthing/rectangle -->
```

Available argument value types (case insensitive):
- Bool: `true` or `false`
- Nil: `nil`
- Inf: `inf`
- Int: suffix `i32` (`-1i32`, `1i32`, etc)
- Long: suffix `i64` (`-1i64`, `1i64`, etc)
- Float: suffix `f32` (`1f32`, `1.0f32`, etc)
- Double: suffix `f64` (`1f64`, `1.0f64`, etc)
- String: any other value
    - Shorthand form will treat Strings with spaces as multiple arguments. Use parameter form if you need spaces.

##### `::SendKey <VirtualKey> <UP|DOWN>`

Sends a key using the virtual keyboard. If WayVR is focused, the key is sent to the WayVR app.

Supported VirtualKey values are listed [here](https://github.com/galister/wlx-overlay-s/blob/f2bd169c2217d51cd2de862a6429444bf326f471/wlx-overlay-s/src/subsystem/hid/mod.rs#L336).

##### `::PlayspaceReset`

Resets the STAGE space to (0,0,0) with identity rotation.

##### `::PlayspaceRecenter`

Recenters the STAGE space position so that the HMD is in the center. Does not modify floor level.

##### `::PlayspaceFixFloor`

Adjusts the level of floor for STAGE and LOCAL_FLOOR spaces.

The user is asked to place one controller on the floor.

##### `::ContextMenuOpen <name>`

Opens the `<context_menu>` of the given name at the location of the click.

##### `::ContextMenuClose`

Closes any active context menus.

##### `::ElementSetDisplay <id> <none|flex|block|grid>`

Sets the visiblitity of the element with `id`.

##### `::SetToggle <index>`

If the given set is visible, it will be hidden.

Otherwise, it will be set visible.

##### `::SetSwitch <index>`

Switch to the given set. If the set is already visible, nothing happens.

##### `::AddSet`

Add a new set and switch to it. The keyboard will be set to visible.

##### `::DeleteSet`

Hides the current set, then deletes it.

##### `::DashToggle`

Toggle the dashboard

##### `::EditToggle`

Toggle edit mode

##### `::NewMirror`

Opens a new PipeWire mirror (Wayland-only)

##### `::CleanupMirrors`

Destroys all mirrors that are not currently visible (including those that are in a different set).

##### `::Restart`

Restarts WayVR, reloading all settings.

##### `::ShutDown`

Gracefully shuts down WayVR.

##### `::OverlayReset <overlay_name>`

Resets the position of the given overlay and makes it visible.

##### `::OverlayToggle <overlay_name>`

Toggle the visibility of this overlay for the current set

##### `::OverlayDrop <overlay_name>`

Destroys the overlay permanently. Mostly useful for mirrors.

##### `::CustomOverlayReload <overlay_name>`

If this is a custom overlay, reloads its XML from disk.

##### `::WvrOverlayCloseWindow <overlay_name>`

If this is an application, send a close window request to its wl_surface (e.a. X'ing the window)

##### `::WvrOverlayTermProcess <overlay_name>`

If the overlay belongs to an application, sends SIGTERM (graceful exit request) to its process.

##### `::WvrOverlayKillProcess <overlay_name>`

If the overlay belongs to an application, sends SIGKILL (forced exit request) to its process.

Also destroys all overlays belonging to the process.

