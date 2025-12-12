#!/bin/sh
export VERSION=$GITHUB_REF_NAME
./linuxdeploy-x86_64.AppImage -dwlx-overlay-s.desktop -iwlx-overlay-s.png --appdir=${APPDIR} --output appimage --exclude-library '*libpipewire*'
mv WlxOverlay-S-$VERSION-x86_64.AppImage WlxOverlay-S-x86_64.AppImage