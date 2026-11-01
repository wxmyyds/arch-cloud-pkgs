#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"
: "${GITHUB_ACTOR:?GITHUB_ACTOR is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${PACKAGE_NAME:?PACKAGE_NAME is required}"
: "${PACKAGE_FILE:?PACKAGE_FILE is required}"

for tool in curl jq bsdtar sha256sum awk; do
    command -v "$tool" >/dev/null || { echo "::error::Missing publisher tool: $tool"; exit 1; }
done

[[ "$PACKAGE_NAME" =~ ^[a-z0-9][a-z0-9_-]*$ ]] || { echo "::error::Invalid package name"; exit 1; }
[[ -f "$PACKAGE_FILE" ]] || { echo "::error::Package file not found: $PACKAGE_FILE"; exit 1; }

workdir="$(mktemp -d)"
trap 'rm -rf -- "$workdir"' EXIT
bsdtar -xOf "$PACKAGE_FILE" .PKGINFO > "$workdir/PKGINFO"
pkgname="$(awk -F ' = ' '$1 == "pkgname" { print $2; exit }' "$workdir/PKGINFO")"
pkgver="$(awk -F ' = ' '$1 == "pkgver" { print $2; exit }' "$workdir/PKGINFO")"
arch="$(awk -F ' = ' '$1 == "arch" { print $2; exit }' "$workdir/PKGINFO")"
pkgdesc="$(awk -F ' = ' '$1 == "pkgdesc" { print substr($0, index($0, " = ") + 3); exit }' "$workdir/PKGINFO")"
[[ "$pkgname" == "$PACKAGE_NAME" && -n "$pkgver" && -n "$arch" ]] || {
    echo "::error::.PKGINFO identity/version/arch validation failed"
    exit 1
}

# Encode every byte outside OCI's conservative version-tag alphabet as _HH.
tag="v"
LC_ALL=C
for ((i = 0; i < ${#pkgver}; i++)); do
    c="${pkgver:i:1}"
    case "$c" in
        [a-zA-Z0-9.-]) tag+="$c" ;;
        *) printf -v escaped '_%02X' "'${c}"; tag+="$escaped" ;;
    esac
done
[[ ${#tag} -le 128 ]] || { echo "::error::Encoded version exceeds OCI tag length"; exit 1; }

repository="${GITHUB_REPOSITORY%%/*}/${PACKAGE_NAME}"
registry="https://ghcr.io"
accept='application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json'
token="$(curl -fsS -u "$GITHUB_ACTOR:$GITHUB_TOKEN" \
    "$registry/token?service=ghcr.io&scope=repository:$repository:pull,push" | jq -er '.token')"
auth="Authorization: Bearer $token"

package_digest="sha256:$(sha256sum "$PACKAGE_FILE" | awk '{print $1}')"
package_size="$(wc -c < "$PACKAGE_FILE" | tr -d '[:space:]')"
config="$workdir/empty.json"
printf '{}' > "$config"
config_digest="sha256:$(sha256sum "$config" | awk '{print $1}')"

# Never mutate an already published version tag. Identical retries are idempotent.
status="$(curl -sS -o "$workdir/existing.json" -w '%{http_code}' \
    -H "$auth" -H "Accept: $accept" \
    "$registry/v2/$repository/manifests/$tag")"
case "$status" in
    200)
        existing_digest="$(jq -er '.layers[0].digest' "$workdir/existing.json")"
        if [[ "$existing_digest" == "$package_digest" ]]; then
            echo "OCI tag $repository:$tag already has this package digest; skipping push."
            exit 0
        fi
        echo "::warning::Existing tag $repository:$tag is immutable and has another digest; bump pkgrel to publish this rebuild."
        exit 0
        ;;
    404) ;;
    *)
        echo "::error::Could not check existing OCI tag (HTTP $status)"
        cat "$workdir/existing.json"
        exit 1
        ;;
esac

upload_blob() {
    local file="$1" digest="$2" headers location separator
    local head_status
    head_status="$(curl -sS -o /dev/null -w '%{http_code}' -I -H "$auth" \
        "$registry/v2/$repository/blobs/$digest")"
    [[ "$head_status" == 200 ]] && return 0
    [[ "$head_status" == 404 ]] || { echo "::error::Blob existence check failed (HTTP $head_status)"; return 1; }

    headers="$workdir/upload.headers"
    local post_status
    post_status="$(curl -sS -D "$headers" -o "$workdir/upload.json" -w '%{http_code}' \
        -X POST -H "$auth" "$registry/v2/$repository/blobs/uploads/")"
    [[ "$post_status" == 202 ]] || { echo "::error::Blob upload session failed (HTTP $post_status)"; cat "$workdir/upload.json"; return 1; }
    location="$(awk 'BEGIN { IGNORECASE=1 } /^Location:/ { sub(/^[^:]*:[[:space:]]*/, ""); sub(/\r$/, ""); value=$0 } END { print value }' "$headers")"
    [[ -n "$location" ]] || { echo "::error::Registry omitted upload Location"; return 1; }
    safe_location="${location%%\?*}"
    echo "Registry upload Location: $safe_location"
    if [[ "$location" == https://* ]]; then
        [[ "$location" == "$registry/"* ]] || {
            echo "::error::Registry returned an upload URL outside GHCR: $safe_location"
            return 1
        }
        location_path="${location#https://ghcr.io}"
        [[ "$location_path" == "/v2/$repository/blobs/uploads/"* ]] || {
            echo "::error::Registry returned an upload URL outside the expected repository: $safe_location"
            return 1
        }
    elif [[ "$location" == /v2/$repository/blobs/uploads/* ]]; then
        location="$registry$location"
    else
        echo "::error::Registry returned an unexpected upload Location: $safe_location"
        return 1
    fi
    separator='?'
    [[ "$location" == *\?* ]] && separator='&'
    curl -fsS -o /dev/null -X PUT -H "$auth" -H 'Content-Type: application/octet-stream' \
        --data-binary "@$file" "$location${separator}digest=$digest"
}

upload_blob "$config" "$config_digest"
upload_blob "$PACKAGE_FILE" "$package_digest"

jq -n \
    --arg artifact_type 'application/vnd.wxmyyds.cpkg.v1' \
    --arg config_digest "$config_digest" \
    --arg package_digest "$package_digest" \
    --argjson package_size "$package_size" \
    --arg pkgname "$pkgname" \
    --arg pkgver "$pkgver" \
    --arg arch "$arch" \
    --arg description "$pkgdesc" \
    --arg source "https://github.com/$GITHUB_REPOSITORY" \
    --arg pkginfo "$(<"$workdir/PKGINFO")" \
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
    }' > "$workdir/manifest.json"

curl -fsS -o "$workdir/push.json" -w 'Manifest push HTTP %{http_code}\n' \
    -X PUT -H "$auth" -H 'Content-Type: application/vnd.oci.image.manifest.v1+json' \
    --data-binary "@$workdir/manifest.json" \
    "$registry/v2/$repository/manifests/$tag"

echo "Published $pkgname $pkgver as ghcr.io/$repository:$tag ($package_size bytes, $package_digest)"
