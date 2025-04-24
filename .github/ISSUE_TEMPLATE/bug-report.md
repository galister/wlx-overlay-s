---
name: Bug Report
about: Create a report to help us improve
title: ''
labels: ''
assignees: ''

---

## Description
<!-- 
If this is a regression, please mention which version was working previously.
-->

## System Info
**Linux Distribution**: 

<!-- Paste output of `echo $XDG_CURRENT_DESKTOP`, optionally add version -->
**Desktop Environment**: 

<!-- Paste output of `uname -r` -->
**Kernel version**: 

**VR Runtime**:
- [ ] Monado/WiVRn
- [ ] SteamVR/ALVR

<!-- Run `vulkaninfo --summary` and paste the devices section from the bottom. -->
**GPU models and driver versions**: 

## Overlay Logs

<!-- Start the overlay once more with the following environment variables:
  RUST_BACKTRACE=full
  RUST_LOG=debug
If your issue is graphical or crash or freeze, also add:
  VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation

Be sure to go and reproduce the issue once more, after these have been set.

Upload the log file from: /tmp/wlx.log
-->

