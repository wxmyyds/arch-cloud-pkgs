#!/usr/bin/env bash
# Delete arch-cloud-pkgs intermediate packages from its current workflow run.
# The caller must wait for every consumer and publisher to succeed.
set -euo pipefail

[[ ${GITHUB_ACTIONS:-} == true ]] || { echo 'Run artifact cleanup through GitHub Actions' >&2; exit 1; }
: "${GITHUB_REPOSITORY:?}" "${GITHUB_RUN_ID:?}" "${GH_TOKEN:?}" "${GITHUB_STEP_SUMMARY:?}"
prefix=arch-packages-
[[ $GITHUB_REPOSITORY == wxmyyds/arch-cloud-pkgs && $GITHUB_RUN_ID =~ ^[0-9]+$ ]] || {
    echo 'Artifact cleanup is restricted to an arch-cloud-pkgs workflow run' >&2
    exit 1
}

# Collect every page before deleting: mutating a paginated result shifts pages.
inventory=$(gh api --paginate --slurp \
    "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/artifacts?per_page=100")
targets=$(jq -c --arg prefix "$prefix" \
    '[.[].artifacts[] | select(.name | startswith($prefix)) | {id, name, size_in_bytes}]' \
    <<< "$inventory")
count=$(jq 'length' <<< "$targets")
bytes=$(jq '[.[].size_in_bytes] | add // 0' <<< "$targets")
ids=$(jq -r '.[].id' <<< "$targets")
while IFS= read -r id; do
    [[ -n $id ]] || continue
    gh api --method DELETE "repos/$GITHUB_REPOSITORY/actions/artifacts/$id"
    printf 'Deleted consumed artifact %s from run %s\n' "$id" "$GITHUB_RUN_ID"
done <<< "$ids"

{
    printf '\n### Intermediate artifact cleanup\n\n'
    printf 'Removed %s consumed `%s*` artifacts (%s bytes) from run `%s`.\n' \
        "$count" "$prefix" "$bytes" "$GITHUB_RUN_ID"
    printf 'Release assets, registry packages, caches, test reports and manifests are unchanged.\n'
} >> "$GITHUB_STEP_SUMMARY"
