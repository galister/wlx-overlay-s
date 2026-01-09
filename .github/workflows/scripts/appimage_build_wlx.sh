#!/bin/sh
cargo build --release
chmod +x ../target/release/wayvr
cp ../target/release/wayvr ${APPDIR}/usr/bin
