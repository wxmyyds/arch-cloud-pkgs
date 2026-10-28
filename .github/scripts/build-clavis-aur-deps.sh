#!/usr/bin/env bash
# Build the Clavis AUR runtime dependencies in the ephemeral GitHub Arch runner.
set -euo pipefail

dest="${1:?usage: build-clavis-aur-deps.sh PACKAGE_DIRECTORY}"
mkdir -p "$dest"
work_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/clavis-aur.XXXXXX")"
chown builder:builder "$work_root"
trap 'rm -rf -- "$work_root"' EXIT

for package in libcava qt6-m3shapes-git; do
    work_dir="$work_root/$package"
    git clone --depth=1 "https://aur.archlinux.org/${package}.git" "$work_dir"
    chown -R builder:builder "$work_dir"
    (
        cd "$work_dir"
        sudo -u builder makepkg -sf --noconfirm
    )
    shopt -s nullglob
    outputs=()
    for candidate in "$work_dir"/*.pkg.tar.zst; do
        [[ -e "$candidate" ]] || continue
        built_name="$(bsdtar -xOf "$candidate" .PKGINFO | awk -F ' = ' '$1 == "pkgname" { print $2; exit }')"
        if [[ "$built_name" == "$package" ]]; then
            outputs+=("$candidate")
        fi
    done
    if (( ${#outputs[@]} != 1 )); then
        printf 'Expected exactly one %s package, found %d\n' "$package" "${#outputs[@]}" >&2
        exit 1
    fi
    pacman -U --noconfirm "${outputs[0]}"
    cp -f -- "${outputs[0]}" "$dest/"
    printf 'Built and installed CI dependency: %s\n' "${outputs[0]}"
done
