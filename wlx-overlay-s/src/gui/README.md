# WayVR GUI Customization

Place custom XML files under ~/.config/wayvr/gui

## Custom timezones, 12 vs 24-hour clock

These are not done via the GUI system, but via the regular config.

Create `~/.config/wayvr/conf.d/clock.yaml` as such:

```yaml
timezones:
- "Europe/Oslo"
- "America/New_York"

clock_12h: false
```

Once this file is created, the various settings in custom UI that accept the `_timezone` property will use these custom alternate timezones (instead of the default set, which are selected as major ones on different continents from your current actual timezone).

The first timezone is selected with `_timezone="0"`, the second with `_timezone="1"`, and so on.

There is usually no need to specify your own local timezone in here; omitting `_timezone` from a `_source="clock"` Label will display local time.

## Custom UI Elements

### Labels

#### Clock label

Clock labels are driven by the current time. Available display values are: `name` (timezone name), `time`, `date`, `dow`

See the Custom Timezones section for more info on timezones. Skip `_timezone` to use local time.

```xml
<label _source="clock" _display="time" _timezone="0" [...] />
```

#### Fifo label

Fifo label creates a fifo on your system that other programs can pipe output into.

- The label will look for the last line that has a trailing `\n` and display it as its text.
- The pipe is only actively read while the HMD is active.
  - If the producer fills up the pipe buffer before the headset is activated, a SIGPIPE will be sent to the producer, which shall be handled gracefully.
- If the pipe breaks for any reason, re-creation is attempted after 15 seconds.

```xml
<label _source="fifo" _path="$XDG_RUNTIME_DIR/my-test-label" [...] />
```

Example script to test with:
```bash
for i in {0..99}; do echo "i is $i" > $XDG_RUNTIME_DIR/my-test-label; sleep 1; done
```

#### Shell Exec label

This label executes a shell script using the `sh` shell.

- Write lines to the script's stdout to update the label text.
- The label will look for the last line that has a trailing `\n` and display it as its text.
- Long-running scripts are allowed, but the stdout buffer is only read from while the headset is active.
  - As a consequence, the buffer may fill up during very long periods of inactivity, hanging the script due to IO wait until the headset is activated.
- If the script exits successfully (code 0), it will be re-ran on the next frame.
- Control the pacing from inside the script itself. For example, adding a sleep 5 will make the script execute at most once per 5 seconds.

```xml
<label _source="shell" _exec="$HOME/.local/bin/my-test-script.sh" [...] />
```

```bash
#!/usr/bin/bash
echo "This is my script's output!"
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

Buttons consist of a label component and and one or more actions to handle press or release events.

If a shell-type press/release event's script writes to stdout, the last line of stdout will be set as the label text.

Long-running processes are allowed, but a new execution will not be triggered until the previous process has exited.

Note: As of WlxOverlay 25.10, we no longer support events based on laser color, as this was bad practice accessibility-wise.

Supported events:

```xml
<button _press="" _release="" />
```

