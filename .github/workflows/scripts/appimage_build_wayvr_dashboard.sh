#!/bin/sh
git clone --depth=1 https://github.com/olekolek1000/wayvr-dashboard.git wayvr-dashboard

WLX_DIR=$(realpath $(pwd))

cd wayvr-dashboard
.github/workflows/build.sh

# See https://github.com/olekolek1000/wayvr-dashboard/blob/master/.github/workflows/appimage.sh
cd ..
cd ${APPDIR}

# Fix webkit
echo "Copying webkit runtime executables"

# Copy runtime executables
find -L /usr/lib /usr/libexec -name WebKitNetworkProcess -exec mkdir -p . ';' -exec cp -v --parents '{}' . ';' || true
find -L /usr/lib /usr/libexec -name WebKitWebProcess -exec mkdir -p . ';' -exec cp -v --parents '{}' . ';' || true
find -L /usr/lib /usr/libexec -name libwebkit2gtkinjectedbundle.so -exec mkdir -p . ';' -exec cp --parents '{}' . ';' || true

echo "Patching webkit lib"

# Patch libwebkit .so file: Replace 4 bytes containing "/usr" into "././". Required!
TARGET_WEBKIT_SO="./usr/lib/libwebkit2gtk-4.1.so.0"
cp /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0 ${TARGET_WEBKIT_SO}
sed -i -e "s|/usr|././|g" "${TARGET_WEBKIT_SO}"

cd ${WLX_DIR}

DASH_PATH="${WLX_DIR}/wayvr-dashboard/temp/wayvr-dashboard"
chmod +x ${DASH_PATH}

# Put resulting executable into wlx AppDir
cp ${DASH_PATH} ${APPDIR}/usr/bin/wayvr-dashboard