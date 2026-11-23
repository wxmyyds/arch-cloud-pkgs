#!/usr/bin/env bash
set -euo pipefail

package="${1:?usage: build-aur-package.sh PACKAGE DESTINATION}"
destination="${2:?usage: build-aur-package.sh PACKAGE DESTINATION}"
case "$package" in
    libcava|qt6-m3shapes-git) ;;
    *) echo "Unsupported AUR package: $package" >&2; exit 1 ;;
esac

workdir="$(mktemp -d "${RUNNER_TEMP:-/tmp}/aur-${package}.XXXXXX")"
trap 'rm -rf -- "$workdir"' EXIT
chown builder:builder "$workdir"
sudo -u builder git clone --depth=1 "https://aur.archlinux.org/${package}.git" "$workdir/source"
(
    cd "$workdir/source"
    # Optional per-package pkgrel override: pkgs/<package>/pkgrel holds the pkgrel
    # the caller wants baked into the clone. AUR keeps pkgrel=1 forever, so a
    # rebuild that must land on GHCR under a fresh immutable OCI tag (e.g. a
    # runtime soname change such as sndio 7 -> 9) needs a bumped pkgrel here.
    pkgrel="${AUR_PKGREL_OVERRIDE:-}"
    if [[ -n "$pkgrel" ]]; then
        sed -i -E "s/^pkgrel=.*/pkgrel=${pkgrel}/" PKGBUILD
        chown builder:builder PKGBUILD
    fi
    sudo -u builder makepkg -sf --noconfirm
)

shopt -s nullglob
outputs=()
for file in "$workdir/source"/*.pkg.tar.zst; do
    name="$(bsdtar -xOf "$file" .PKGINFO | awk -F ' = ' '$1 == "pkgname" { print $2; exit }')"
    [[ "$name" == "$package" ]] && outputs+=("$file")
done
if (( ${#outputs[@]} != 1 )); then
    printf 'Expected one %s package, found %d\n' "$package" "${#outputs[@]}" >&2
    exit 1
fi
mkdir -p "$destination"
cp -- "${outputs[0]}" "$destination/"
