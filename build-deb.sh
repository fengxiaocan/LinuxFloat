#!/usr/bin/env bash
set -euo pipefail

# 切换到脚本所在目录（项目根目录）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

APP_NAME="linux-monitor"
VERSION="$(grep -m1 '^version = ' Cargo.toml | cut -d '"' -f2)"
if [ -z "$VERSION" ]; then
    echo "错误: 无法从 Cargo.toml 读取版本号" >&2
    exit 1
fi

ARCH="$(dpkg --print-architecture 2>/dev/null || echo "amd64")"
DIST_DIR="dist"
OUTPUT_DEB="${DIST_DIR}/${APP_NAME}_${VERSION}_${ARCH}.deb"
BUILD_ROOT="target/debian/${APP_NAME}_${VERSION}_${ARCH}"

echo "====================================================="
echo " 打包 Debian 软件包 (.deb): ${APP_NAME} v${VERSION} (${ARCH})"
echo "====================================================="

# 1. 编译 Release 二进制文件
echo "[1/6] 正在编译 Release 版本 (cargo build --release)..."
cargo build --release

BIN_SRC="target/release/${APP_NAME}"
if [ ! -f "$BIN_SRC" ]; then
    echo "错误: 未找到编译产物 $BIN_SRC" >&2
    exit 1
fi

# 2. 准备打包目录树
echo "[2/6] 准备打包工作目录: ${BUILD_ROOT}"
rm -rf "$BUILD_ROOT"
mkdir -p "$BUILD_ROOT/DEBIAN"
mkdir -p "$BUILD_ROOT/usr/bin"
mkdir -p "$BUILD_ROOT/usr/share/applications"
mkdir -p "$BUILD_ROOT/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$BUILD_ROOT/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$BUILD_ROOT/usr/share/pixmaps"
mkdir -p "$BUILD_ROOT/usr/share/man/man1"
mkdir -p "$BUILD_ROOT/usr/share/doc/${APP_NAME}"
mkdir -p "$DIST_DIR"

# 3. 复制并安装各文件
echo "[3/6] 安装可执行文件及资源..."
# 二进制程序并裁剪调试符号
install -m 755 "$BIN_SRC" "$BUILD_ROOT/usr/bin/${APP_NAME}"
strip --strip-unneeded "$BUILD_ROOT/usr/bin/${APP_NAME}" || true

# Desktop 桌面快捷方式
install -m 644 "assets/linux-monitor.desktop" "$BUILD_ROOT/usr/share/applications/${APP_NAME}.desktop"

# 图标
if [ -f "assets/icons/linux-monitor.svg" ]; then
    install -m 644 "assets/icons/linux-monitor.svg" "$BUILD_ROOT/usr/share/icons/hicolor/scalable/apps/${APP_NAME}.svg"
    install -m 644 "assets/icons/linux-monitor.svg" "$BUILD_ROOT/usr/share/pixmaps/${APP_NAME}.svg"
fi

if [ -f "assets/icons/linux-monitor.png" ]; then
    install -m 644 "assets/icons/linux-monitor.png" "$BUILD_ROOT/usr/share/icons/hicolor/256x256/apps/${APP_NAME}.png"
    install -m 644 "assets/icons/linux-monitor.png" "$BUILD_ROOT/usr/share/pixmaps/${APP_NAME}.png"
fi

# Man 手册
if [ -f "assets/linux-monitor.1" ]; then
    gzip -9 -n -c "assets/linux-monitor.1" > "$BUILD_ROOT/usr/share/man/man1/${APP_NAME}.1.gz"
    chmod 644 "$BUILD_ROOT/usr/share/man/man1/${APP_NAME}.1.gz"
fi

# 文档与版权
if [ -f "README.md" ]; then
    install -m 644 "README.md" "$BUILD_ROOT/usr/share/doc/${APP_NAME}/README.md"
fi

# 格式规范的 copyright 文件
cat > "$BUILD_ROOT/usr/share/doc/${APP_NAME}/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: linux-monitor
Source: https://github.com/fengxiaocan/LinuxFloat

Files: *
Copyright: 2026 XiaoCan <fengxiaocan@ayaneo.com>
License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction, including without limitation the rights
 to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 copies of the Software, and to permit persons to whom the Software is
 furnished to do so, subject to the following conditions:
 .
 The above copyright notice and this permission notice shall be included in all
 copies or substantial portions of the Software.
 .
 THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 SOFTWARE.
EOF
chmod 644 "$BUILD_ROOT/usr/share/doc/${APP_NAME}/copyright"

# Debian changelog
cat > "$BUILD_ROOT/usr/share/doc/${APP_NAME}/changelog" <<EOF
${APP_NAME} (${VERSION}) stable; urgency=medium

  * Release version ${VERSION}.

 -- XiaoCan <fengxiaocan@ayaneo.com>  $(date -R)
EOF
gzip -9 -n -f "$BUILD_ROOT/usr/share/doc/${APP_NAME}/changelog"
chmod 644 "$BUILD_ROOT/usr/share/doc/${APP_NAME}/changelog.gz"

# 4. 生成 DEBIAN 元数据与钩子
echo "[4/6] 生成 DEBIAN/control 及安装钩子..."
INSTALLED_SIZE=$(du -sk "$BUILD_ROOT" | cut -f1)

cat > "$BUILD_ROOT/DEBIAN/control" <<EOF
Package: ${APP_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: XiaoCan <fengxiaocan@ayaneo.com>
Installed-Size: ${INSTALLED_SIZE}
Depends: libc6 (>= 2.34), libgcc-s1 (>= 3.0)
Recommends: fonts-dejavu-core | fonts-liberation2 | fonts-noto-core, hicolor-icon-theme
Homepage: https://github.com/fengxiaocan/LinuxFloat
Description: Lightweight Linux desktop performance monitor overlay
 Linux native lightweight performance monitor overlay that directly reads
 /proc, /sys and NVIDIA NVML without spawning shell commands or persistent
 monitoring helper daemons.
EOF
chmod 644 "$BUILD_ROOT/DEBIAN/control"

cat > "$BUILD_ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e

if [ "$1" = "configure" ]; then
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
    fi
fi

exit 0
EOF
chmod 755 "$BUILD_ROOT/DEBIAN/postinst"

cat > "$BUILD_ROOT/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
    fi
fi

exit 0
EOF
chmod 755 "$BUILD_ROOT/DEBIAN/postrm"

# 统一规范目录权限为 0755
find "$BUILD_ROOT" -type d -exec chmod 755 {} +

# 5. 构建 deb 安装包
echo "[5/6] 运行 dpkg-deb 打包..."
dpkg-deb --build --root-owner-group "$BUILD_ROOT" "$OUTPUT_DEB"

# 6. 验证与检查
echo "[6/6] 检查生成的 deb 软件包..."
echo ""
echo "================ 打包完成 ================"
echo "软件包位置: ${OUTPUT_DEB}"
echo "文件大小: $(ls -lh "${OUTPUT_DEB}" | awk '{print $5}')"
echo "=========================================="

echo ""
echo ">>> 控制信息 (dpkg-deb --info):"
dpkg-deb --info "$OUTPUT_DEB"

echo ""
echo ">>> 文件清单 (dpkg-deb --contents):"
dpkg-deb --contents "$OUTPUT_DEB"

if command -v lintian >/dev/null 2>&1; then
    echo ""
    echo ">>> Lintian 规范检查:"
    lintian --tag-display-limit 0 "$OUTPUT_DEB" || true
fi
