#!/bin/sh
cargo build --release
chmod +x ../target/release/wlx-overlay-s
cp ../target/release/wlx-overlay-s ${APPDIR}/usr/bin
