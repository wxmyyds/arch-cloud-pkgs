#!/usr/bin/env bash
# Build the current upstream main source in GitHub Actions; never use its release assets.
set -euo pipefail

repo="${1:?usage: prepare-upstream-source.sh OWNER/REPO PKGBASE PACKAGE_DIRECTORY}"
pkgbase="${2:?usage: prepare-upstream-source.sh OWNER/REPO PKGBASE PACKAGE_DIRECTORY}"
dest="${3:?usage: prepare-upstream-source.sh OWNER/REPO PKGBASE PACKAGE_DIRECTORY}"
case "$repo:$pkgbase" in
    StatIndet/quickshell:clavis-shell|StatIndet/key-cli:key-cli) ;;
    *) printf 'Unsupported upstream package mapping: %s:%s\n' "$repo" "$pkgbase" >&2; exit 2 ;;
esac

build_date="${BUILD_DATE:-$(TZ=Asia/Shanghai date +%F)}"
run_number="${GITHUB_RUN_NUMBER:-1}"
[[ "$build_date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || { echo 'Invalid BUILD_DATE' >&2; exit 2; }
[[ "$run_number" =~ ^[1-9][0-9]*$ ]] || { echo 'Invalid GITHUB_RUN_NUMBER' >&2; exit 2; }

work_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/upstream-source.XXXXXX")"
trap 'rm -rf -- "$work_root"' EXIT
source_dir="$work_root/source"
output_dir="$work_root/package"
mkdir -p "$output_dir"

git clone --depth=1 --branch main "https://github.com/${repo}.git" "$source_dir"
git -C "$source_dir" fetch --tags --force
source_commit="$(git -C "$source_dir" rev-parse --short HEAD)"
chown -R builder:builder "$work_root"

release_tool="$source_dir/scripts/release.py"
version="$(sudo -u builder python3 "$release_tool" next-version --date "$build_date")"
sudo -u builder python3 "$release_tool" stamp "$version"
archive="$(sudo -u builder python3 "$release_tool" source --working-tree --output "$output_dir")"
sudo -u builder python3 "$release_tool" render \
    --output "$output_dir" --archive "$archive" --pkgrel "$run_number"

[[ -s "$output_dir/PKGBUILD" && -s "$archive" ]] || { echo 'Upstream packaging did not produce PKGBUILD/source archive' >&2; exit 1; }
mkdir -p /opt/cache/sources
install -m644 "$archive" "/opt/cache/sources/$(basename "$archive")"
cp -f "$output_dir/PKGBUILD" "$dest/PKGBUILD"
for helper in "$output_dir"/*.install "$output_dir"/*.hook; do
    [[ -f "$helper" ]] || continue
    cp -f "$helper" "$dest/"
done
chown -R builder:builder "$dest" /opt/cache/sources
printf 'Prepared %s from main commit %s as %s-%s (pkgrel %s)\n' \
    "$pkgbase" "$source_commit" "$pkgbase" "$version" "$run_number"
