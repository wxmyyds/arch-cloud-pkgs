#!/bin/bash
# rtk-termux 交叉工具链准备 — 由工作流按需调用，无需硬编码 pkg 名
set -e
echo "[prepare.sh] rtk-termux 准备交叉工具链"

# unzip 需在 NDK 解压前就绪
pacman -S --needed --noconfirm unzip 2>&1 || true

# android-ndk 在 AUR，不在官方 DB，需手动装
if ! pacman -Qi android-ndk >/dev/null 2>&1 && [ ! -d "/opt/android-ndk" ]; then
  echo "安装 android-ndk (约 1.2GB，需几分钟)"
  pacman -S --noconfirm android-ndk 2>&1 && echo "pacman 装 android-ndk 成功" || {
    echo "pacman 无 android-ndk，走官方 zip 直装"
    curl -fsSL https://dl.google.com/android/repository/android-ndk-r26d-linux.zip -o /tmp/ndk.zip
    mkdir -p /opt && unzip -q /tmp/ndk.zip -d /opt
    mv /opt/android-ndk-r26d /opt/android-ndk
    ln -sf /opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android* /usr/bin/ 2>/dev/null || true
    rm /tmp/ndk.zip
  }
fi

# builder 和 root 双用户 rustup
pacman -S --needed --noconfirm rustup 2>&1 || true
rustup toolchain install stable --profile minimal 2>&1 || true
rustup default stable 2>&1 || true
sudo -u builder bash -c 'rustup toolchain install stable --profile minimal 2>&1 || true; rustup default stable 2>&1 || true' || true

if ! command -v cargo-ndk >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/cargo-ndk" ] && [ ! -x "/home/builder/.cargo/bin/cargo-ndk" ]; then
  cargo install cargo-ndk --locked 2>&1 || sudo -u builder bash -c 'cargo install cargo-ndk --locked' 2>&1 || true
fi

sudo -u builder rustup target add aarch64-linux-android 2>&1 || rustup target add aarch64-linux-android 2>&1 || true
cargo ndk --version 2>&1 || ~/.cargo/bin/cargo-ndk --version 2>&1 || /home/builder/.cargo/bin/cargo-ndk --version 2>&1 || true

# 授权 NDK 给 builder
chown -R builder:builder /opt/android-ndk 2>/dev/null || true
