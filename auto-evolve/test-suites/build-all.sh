#!/usr/bin/env bash
# 一键构建全部测试套件（697 个测试用例）
# 用法: ./auto-evolve/test-suites/build-all.sh [ARCH]
# 需要: musl 交叉编译工具链 + autoconf/automake/libtool

set -e
ARCH="${1:-riscv64}"
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
GCC="${ARCH}-linux-musl-gcc"

echo "════════════════════════════════════════════"
echo "  构建全部测试套件 (arch=$ARCH)"
echo "════════════════════════════════════════════"

if ! command -v "$GCC" &>/dev/null; then
    echo "错误: 找不到 $GCC"
    echo "请安装 musl 交叉编译工具链并加入 PATH"
    exit 1
fi

# ── L1: 自编测试 ──
echo ""
echo "── L1: 自编测试 ──"
"$ROOT/auto-evolve/test-harness.py" build --arch "$ARCH"

# ── L2: oscomp basic ──
echo ""
echo "── L2: oscomp basic ──"
ARCH=$ARCH python3 "$DIR/oscomp-basic/gen-basic-tests.py"

# ── L3: libc-test ──
echo ""
echo "── L3: libc-test ──"
LIBC_DIR="/tmp/libc-test-build-$$"
if [ ! -d "$LIBC_DIR" ]; then
    git clone --depth 1 git://repo.or.cz/libc-test.git "$LIBC_DIR"
fi
cd "$LIBC_DIR"
cat > config.mak << EOF
CROSS_COMPILE=${ARCH}-linux-musl-
CC=\$(CROSS_COMPILE)gcc
CFLAGS+=-static
LDFLAGS+=-static
EOF
make CC="${GCC}" CROSS_COMPILE="${ARCH}-linux-musl-" CFLAGS="-static -Isrc/common" LDFLAGS="-static" -j$(nproc) 2>/dev/null || true

LIBC_OUT="$DIR/libc-test/$ARCH"
mkdir -p "$LIBC_OUT"
for d in functional regression; do
    for exe in src/$d/*.exe; do
        [ -f "$exe" ] && cp "$exe" "$LIBC_OUT/"
    done
done
cp src/common/runtest.exe "$LIBC_OUT/" 2>/dev/null || true
echo "libc-test: $(ls "$LIBC_OUT"/*.exe 2>/dev/null | wc -l) 个二进制"
cd "$ROOT"

# ── L4: LTP ──
echo ""
echo "── L4: LTP (syscalls) ──"
LTP_DIR="/tmp/ltp-build-$$"
if [ ! -d "$LTP_DIR" ]; then
    git clone --depth 1 --branch 20240930 https://github.com/linux-test-project/ltp.git "$LTP_DIR"
fi
cd "$LTP_DIR"
if [ ! -f configure ]; then
    make autotools 2>/dev/null
    ./configure --host="${ARCH}-linux-musl" CC="$GCC" \
        --prefix=/tmp/ltp-install --without-tirpc --disable-metadata 2>/dev/null
fi
make -C testcases/kernel/syscalls -j$(nproc) 2>/dev/null || true

LTP_OUT="$DIR/ltp/$ARCH"
mkdir -p "$LTP_OUT"
find testcases/kernel/syscalls -type f -executable ! -name "*.sh" ! -name "*.py" ! -name "Makefile" -exec cp {} "$LTP_OUT/" \;
echo "LTP: $(ls "$LTP_OUT" | wc -l) 个二进制"
cd "$ROOT"

# ── 总结 ──
echo ""
echo "════════════════════════════════════════════"
echo "  构建完成"
echo "  L1 自编:      $(ls "$ROOT/auto-evolve/test-build/$ARCH/l1"/test_* 2>/dev/null | wc -l) 个"
echo "  L2 basic:     $(ls "$DIR/oscomp-basic/$ARCH"/test_* 2>/dev/null | wc -l) 个"
echo "  L3 libc-test: $(ls "$DIR/libc-test/$ARCH"/*.exe 2>/dev/null | wc -l) 个"
echo "  L4 LTP:       $(ls "$DIR/ltp/$ARCH"/ 2>/dev/null | wc -l) 个"
echo "════════════════════════════════════════════"
