#!/bin/sh
export VERSION=$GITHUB_REF_NAME
./linuxdeploy-x86_64.AppImage -dwayvr.desktop -iwayvr.png --appdir=${APPDIR} --output appimage --exclude-library '*libpipewire*'
mv WayVR-$VERSION-x86_64.AppImage WayVR-x86_64.AppImage
