#!/usr/bin/env bash
set -euo pipefail

# The single publisher for repository builds and external producers of finished packages.
# Usage: PACKAGE_NAME=... GITHUB_REPOSITORY=owner/source PUBLISH=false bash "$0" package.pkg.tar.zst [...]
: "${PACKAGE_NAME:?PACKAGE_NAME is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
PUBLISH="${PUBLISH-true}"
[[ "$PUBLISH" == true || "$PUBLISH" == false ]] || { echo '::error::PUBLISH must be true or false'; exit 1; }
[[ "$#" -gt 0 ]] || { echo '::error::Pass one or more .pkg.tar.zst package paths'; exit 1; }
[[ "$PACKAGE_NAME" =~ ^[a-z0-9][a-z0-9_-]*$ ]] || { echo '::error::Invalid package name'; exit 1; }
[[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$ ]] || { echo '::error::Invalid source repository'; exit 1; }
for tool in jq bsdtar zstd sha256sum awk od wc tr sort; do
    command -v "$tool" >/dev/null || { echo "::error::Missing publisher tool: $tool"; exit 1; }
done

workdir="$(mktemp -d)"
trap 'rm -rf -- "$workdir"' EXIT
export LC_ALL=C
manifest_type='application/vnd.oci.image.manifest.v1+json'
index_type='application/vnd.oci.image.index.v1+json'
owner="${GITHUB_REPOSITORY%%/*}"
repository="${owner,,}/$PACKAGE_NAME"
registry='https://ghcr.io'
config="$workdir/empty.json"
printf '{}' > "$config"
file_digest() { sha256sum < "$1" | awk '{print "sha256:" $1}'; }
file_size() { wc -c < "$1" | tr -d '[:space:]'; }
config_digest="$(file_digest "$config")"
pkginfo_field() {
    awk -F ' = ' -v field="$2" '$1 == field { count++; value=substr($0, index($0, " = ") + 3) }
        END { if (count != 1 || value == "") exit 1; print value }' "$1" || {
        echo "::error::.PKGINFO requires exactly one nonempty $2 field" >&2
        return 1
    }
}

declare -A package_files package_digests package_sizes
pkgver=''
for file in "$@"; do
    [[ "$file" == *.pkg.tar.zst && -f "$file" ]] || { echo "::error::Expected a .pkg.tar.zst file: $file"; exit 1; }
    # The suffix alone is not evidence of compression (Tauri may emit other tar formats).
    [[ "$(od -An -tx1 -N4 -- "$file" | tr -d '[:space:]')" == 28b52ffd ]] || { echo "::error::Not a zstd frame: $file"; exit 1; }
    zstd -t -- "$file"
    info="$workdir/PKGINFO"
    bsdtar -xOf "$file" .PKGINFO > "$info"
    pkgname="$(pkginfo_field "$info" pkgname)"
    version="$(pkginfo_field "$info" pkgver)"
    arch="$(pkginfo_field "$info" arch)"
    description="$(pkginfo_field "$info" pkgdesc)"
    [[ "$pkgname" == "$PACKAGE_NAME" && ! "$version" =~ [[:cntrl:][:space:]/] ]] || { echo '::error::.PKGINFO identity/version validation failed'; exit 1; }
    [[ -z "$pkgver" || "$version" == "$pkgver" ]] || { echo '::error::All packages must have the same pkgver (including pkgrel/epoch)'; exit 1; }
    pkgver="$version"
    case "$arch" in
        x86_64) oci_arch=amd64 ;;
        aarch64) oci_arch=arm64 ;;
        any)
            [[ "$#" == 1 ]] || { echo '::error::An index requires native architectures; any is single-file only'; exit 1; }
            oci_arch=any
            ;;
        *) echo "::error::Unsupported package architecture: $arch"; exit 1 ;;
    esac
    [[ -z "${package_files[$arch]+present}" ]] || { echo "::error::Duplicate package architecture: $arch"; exit 1; }
    package_files[$arch]="$file"
    package_digests[$arch]="$(file_digest "$file")"
    package_sizes[$arch]="$(file_size "$file")"
    [[ "${package_sizes[$arch]}" -gt 0 ]] || { echo '::error::Empty package'; exit 1; }

    # Keep the one-file manifest byte layout and annotations used by existing cpkg releases.
    jq -n \
        --arg artifact_type 'application/vnd.wxmyyds.cpkg.v1' \
        --arg config_digest "$config_digest" \
        --arg package_digest "${package_digests[$arch]}" \
        --argjson package_size "${package_sizes[$arch]}" \
        --arg pkgname "$pkgname" --arg pkgver "$pkgver" --arg arch "$arch" \
        --arg description "$description" --arg source "https://github.com/$GITHUB_REPOSITORY" \
        --arg pkginfo "$(<"$info")" \
        '{
          schemaVersion: 2,
          mediaType: "application/vnd.oci.image.manifest.v1+json",
          artifactType: $artifact_type,
          config: {
            mediaType: "application/vnd.oci.empty.v1+json",
            digest: $config_digest,
            size: 2
          },
          layers: [{
            mediaType: "application/vnd.wxmyyds.cpkg.pkg.v1",
            digest: $package_digest,
            size: $package_size
          }],
          annotations: {
            "org.opencontainers.image.source": $source,
            "org.opencontainers.image.title": $pkgname,
            "org.opencontainers.image.version": $pkgver,
            "org.opencontainers.image.description": $description,
            "org.wxmyyds.cpkg.arch": $arch,
            "org.wxmyyds.cpkg.pkginfo": $pkginfo
          }
        }' > "$workdir/manifest-$arch.json"
    jq -n --arg type "$manifest_type" --arg arch "$oci_arch" \
        --arg digest "$(file_digest "$workdir/manifest-$arch.json")" \
        --argjson size "$(file_size "$workdir/manifest-$arch.json")" \
        '{mediaType: $type, digest: $digest, size: $size} +
         (if $arch == "any" then {} else {platform: {os: "linux", architecture: $arch}} end)' \
        > "$workdir/descriptor-$arch.json"
done

# Encode every byte outside OCI's conservative version-tag alphabet as _HH.
tag='v'
for ((i = 0; i < ${#pkgver}; i++)); do
    c="${pkgver:i:1}"
    case "$c" in
        [a-zA-Z0-9.-]) tag+="$c" ;;
        *) printf -v escaped '_%02X' "'${c}"; tag+="$escaped" ;;
    esac
done
[[ ${#tag} -le 128 ]] || { echo '::error::Encoded version exceeds OCI tag length'; exit 1; }
mapfile -t arches < <(printf '%s\n' "${!package_files[@]}" | sort)
root="$workdir/root.json"
root_type="$manifest_type"
if [[ "${#arches[@]}" == 1 ]]; then
    cp -- "$workdir/manifest-${arches[0]}.json" "$root"
else
    root_type="$index_type"
    jq -s --arg type "$index_type" --arg name "$PACKAGE_NAME" --arg version "$pkgver" \
        --arg source "https://github.com/$GITHUB_REPOSITORY" \
        '{schemaVersion: 2, mediaType: $type, manifests: ., annotations: {
          "org.opencontainers.image.source": $source,
          "org.opencontainers.image.title": $name,
          "org.opencontainers.image.version": $version
        }}' "$workdir"/descriptor-*.json > "$root"
fi
root_digest="$(file_digest "$root")"
for arch in "${arches[@]}"; do
    jq -cn --arg file "${package_files[$arch]}" --arg arch "$arch" \
        --arg digest "${package_digests[$arch]}" --argjson size "${package_sizes[$arch]}" \
        --slurpfile manifest "$workdir/descriptor-$arch.json" \
        '{file: $file, arch: $arch, digest: $digest, size: $size, manifest: $manifest[0]}'
done > "$workdir/report.jsonl"
jq -cn --arg package "$PACKAGE_NAME" --arg version "$pkgver" --arg reference "ghcr.io/$repository:$tag" \
    --arg mediaType "$root_type" --arg digest "$root_digest" --argjson size "$(file_size "$root")" \
    --argjson publish "$PUBLISH" --slurpfile packages "$workdir/report.jsonl" \
    '{validated: true, publish: $publish, package: $package, version: $version, reference: $reference,
      mediaType: $mediaType, digest: $digest, size: $size, packages: $packages}'

# Validation performs no network requests, including token requests, and needs no credentials.
[[ "$PUBLISH" == true ]] || exit 0
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required when PUBLISH=true}"
: "${GITHUB_ACTOR:?GITHUB_ACTOR is required when PUBLISH=true}"
command -v curl >/dev/null || { echo '::error::Missing publisher tool: curl'; exit 1; }
token="$(curl -fsS -u "$GITHUB_ACTOR:$GITHUB_TOKEN" \
    "$registry/token?service=ghcr.io&scope=repository:$repository:pull,push" | jq -er '.token | select(type == "string" and length > 0)')"
auth="Authorization: Bearer $token"

# Never mutate a published version, including changing its architecture set or metadata.
version_exists=false
check_version_tag() {
    local status
    status="$(curl -sS -o "$workdir/existing.json" -w '%{http_code}' \
        -H "$auth" -H "Accept: $manifest_type, $index_type" \
        "$registry/v2/$repository/manifests/$tag")"
    case "$status" in
        200)
            [[ "$(file_digest "$workdir/existing.json")" == "$root_digest" ]] || {
                echo "::error::Immutable version $repository:$tag has a different manifest digest; bump pkgrel"
                exit 1
            }
            version_exists=true
            ;;
        404) version_exists=false ;;
        *) echo "::error::Could not check existing OCI tag (HTTP $status)"; exit 1 ;;
    esac
}
check_version_tag
if [[ "$version_exists" == true ]]; then
    echo "OCI tag $repository:$tag already has digest $root_digest; skipping push."
    exit 0
fi

response_digest() {
    awk 'tolower($1) == "docker-content-digest:" { sub(/\r$/, "", $2); value=$2 } END { print value }' "$1"
}
upload_blob() {
    local file="$1" digest="$2" headers="$workdir/upload.headers" status location safe_location location_path separator
    [[ "$(file_digest "$file")" == "$digest" ]] || { echo "::error::Blob changed after validation: $file"; exit 1; }
    status="$(curl -sS -D "$headers" -o /dev/null -w '%{http_code}' -I -H "$auth" \
        "$registry/v2/$repository/blobs/$digest")"
    if [[ "$status" == 200 ]]; then
        [[ "$(response_digest "$headers")" == "$digest" ]] || { echo '::error::Registry blob digest mismatch'; exit 1; }
        return 0
    fi
    [[ "$status" == 404 ]] || { echo "::error::Blob existence check failed (HTTP $status)"; exit 1; }
    status="$(curl -sS -D "$headers" -o "$workdir/upload.json" -w '%{http_code}' \
        -X POST -H "$auth" "$registry/v2/$repository/blobs/uploads/")"
    [[ "$status" == 202 ]] || { echo "::error::Blob upload session failed (HTTP $status)"; exit 1; }
    location="$(awk 'tolower($1) == "location:" { sub(/^[^:]*:[[:space:]]*/, ""); sub(/\r$/, ""); value=$0 } END { print value }' "$headers")"
    [[ -n "$location" && ! "$location" =~ [[:cntrl:][:space:]] && "$location" != *'#'* ]] || { echo '::error::Invalid upload Location'; exit 1; }
    safe_location="${location%%\?*}"
    if [[ "$location" == "$registry/"* ]]; then
        location_path="${location#https://ghcr.io}"
    elif [[ "$location" == /v2/* ]]; then
        location_path="$location"
        location="$registry$location"
    else
        echo "::error::Registry returned an upload URL outside GHCR: $safe_location"
        exit 1
    fi
    [[ "$location_path" == "/v2/$repository/blobs/uploads/"* || "$location_path" == "/v2/$repository/blobs/upload/"* ]] || {
        echo "::error::Upload Location is outside the expected repository: $safe_location"; exit 1;
    }
    location_path="${location_path%%\?*}"
    [[ "$location_path/" != *'/../'* && "$location_path/" != *'/./'* && "$location_path" != *'%'* && "$location_path" != *'\'* && "$location_path" != *'//'* ]] || {
        echo '::error::Unsafe upload Location path'; exit 1;
    }
    separator='?'
    [[ "$location" == *\?* ]] && separator='&'
    status="$(curl -sS -D "$headers" -o "$workdir/upload.json" -w '%{http_code}' \
        -X PUT -H "$auth" -H 'Content-Type: application/octet-stream' \
        --data-binary "@$file" "$location${separator}digest=$digest")"
    [[ "$status" == 201 && "$(response_digest "$headers")" == "$digest" ]] || { echo "::error::Blob upload/digest validation failed (HTTP $status)"; exit 1; }
    [[ "$(file_digest "$file")" == "$digest" ]] || { echo "::error::Blob changed during upload: $file"; exit 1; }
}
push_manifest() {
    local file="$1" reference="$2" type="$3" status digest
    digest="$(file_digest "$file")"
    status="$(curl -sS -D "$workdir/push.headers" -o "$workdir/push.json" -w '%{http_code}' \
        -X PUT -H "$auth" -H "Content-Type: $type" --data-binary "@$file" \
        "$registry/v2/$repository/manifests/$reference")"
    [[ "$status" == 201 && "$(response_digest "$workdir/push.headers")" == "$digest" ]] || { echo "::error::Manifest push/digest validation failed (HTTP $status)"; exit 1; }
}

upload_blob "$config" "$config_digest"
for arch in "${arches[@]}"; do
    upload_blob "${package_files[$arch]}" "${package_digests[$arch]}"
    if [[ "$root_type" == "$index_type" ]]; then
        leaf="$workdir/manifest-$arch.json"
        push_manifest "$leaf" "$(file_digest "$leaf")" "$manifest_type"
    fi
done
# Recheck after uploads so a concurrent differing publication fails before assigning the tag.
check_version_tag
if [[ "$version_exists" != true ]]; then
    push_manifest "$root" "$tag" "$root_type"
fi
echo "Published $PACKAGE_NAME $pkgver as ghcr.io/$repository:$tag ($root_digest)"
