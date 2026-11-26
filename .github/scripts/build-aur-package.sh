#!/usr/bin/env bash
set -euo pipefail

package="${1:?usage: build-aur-package.sh PACKAGE DESTINATION}"
destination="${2:?usage: build-aur-package.sh PACKAGE DESTINATION}"
case "$package" in
    libcava|qt6-m3shapes-git) ;;
    *) echo "Unsupported AUR package: $package" >&2; exit 1 ;;
esac

requested_pkgrel="${AUR_PKGREL_OVERRIDE:-}"
if [[ -n "$requested_pkgrel" && ! "$requested_pkgrel" =~ ^[1-9][0-9]*$ ]]; then
    echo 'AUR pkgrel override must be a positive integer' >&2
    exit 1
fi

workdir="$(mktemp -d "${RUNNER_TEMP:-/tmp}/aur-${package}.XXXXXX")"
trap 'rm -rf -- "$workdir"' EXIT
chown builder:builder "$workdir"
sudo -u builder git clone --depth=1 "https://aur.archlinux.org/${package}.git" "$workdir/source"
(
    cd "$workdir/source"
    # Resolve and verify sources first: a changed VCS pkgver() resets pkgrel to 1.
    # Apply the repository override only after that update, then build this same
    # prepared checkout so the override cannot be discarded by a source refresh.
    sudo -u builder makepkg --nobuild --syncdeps --noconfirm
    if [[ -n "$requested_pkgrel" ]]; then
        [[ "$(grep -Ec '^pkgrel=' PKGBUILD)" == 1 ]] || {
            echo 'Expected exactly one pkgrel assignment in the AUR PKGBUILD' >&2
            exit 1
        }
        sed -i -E "s/^pkgrel=.*/pkgrel=${requested_pkgrel}/" PKGBUILD
        chown builder:builder PKGBUILD
    fi
    sudo -u builder makepkg --force --noextract --holdver --syncdeps --noconfirm
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
if [[ -n "$requested_pkgrel" ]]; then
    version="$(bsdtar -xOf "${outputs[0]}" .PKGINFO | awk -F ' = ' '$1 == "pkgver" { print $2; exit }')"
    [[ "${version##*-}" == "$requested_pkgrel" ]] || {
        printf 'Expected pkgrel %s, got package version %s\n' "$requested_pkgrel" "$version" >&2
        exit 1
    }
fi
mkdir -p "$destination"
cp -- "${outputs[0]}" "$destination/"
