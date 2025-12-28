#!/bin/sh
WAYVR_DASHBOARD_PATH="/tmp/wayvr-dashboard"

MAIN_DIR=$(realpath $(pwd))

# built wayvr-dashboard binary executable path
DASH_PATH="${WAYVR_DASHBOARD_PATH}/temp/wayvr-dashboard"

git clone --depth=1 https://github.com/olekolek1000/wayvr-dashboard.git ${WAYVR_DASHBOARD_PATH}

cd ${WAYVR_DASHBOARD_PATH}
.github/workflows/build.sh

# See https://github.com/olekolek1000/wayvr-dashboard/blob/master/.github/workflows/appimage.sh
cd ${MAIN_DIR}
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

cd ${MAIN_DIR}

chmod +x ${DASH_PATH}

# Put resulting executable into wlx AppDir
cp ${DASH_PATH} ${APPDIR}/usr/bin/wayvr-dashboard