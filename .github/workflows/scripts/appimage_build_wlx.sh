#!/bin/sh
cargo build --release
mv ../target/release/wlx-overlay-s ${APPDIR}/usr/bin
chmod +x ${APPDIR}/usr/bin/wlx-overlay-s
