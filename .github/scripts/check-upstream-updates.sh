#!/usr/bin/env bash
# =====================================================================
# 二期：上游发版检测 — 由 .github/workflows/check-updates.yml 调用
# 运行环境: archlinux:base-devel 容器（nvchecker 在官方 extra 仓库）
# 用法:     check-upstream-updates.sh
#
# 职责：跑 nvchecker，把 .github/nvchecker.toml 里"版本发生变化"的条目映射成
#       pkgs/ 下的包名，写到 $GITHUB_OUTPUT 供工作流触发构建。只做检测与信号
#       输出，不解析最终 pkgver、不计算校验值——那些仍由各 PKGBUILD 在构建时
#       自行解析，因此这里的取值形态与 pkgver 不一致也无害。
#
# 输出（写入 $GITHUB_OUTPUT，同时打印到日志）：
#   changed     空格分隔的包名列表（仅包含 pkgs/ 下真实存在的目录）
#   json        changed 的 JSON 数组形式
#   has_updates true/false
#   missing     空格分隔的"本次未取到版本"的条目名（上游源异常/结构变化）
# =====================================================================
set -euo pipefail

config='.github/nvchecker.toml'
oldver='.github/nvchecker/oldver.json'
newver='.github/nvchecker/newver.json'

emit() {
  printf '%s: %s\n' "$1" "$2"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    printf '%s=%s\n' "$1" "$2" >> "$GITHUB_OUTPUT"
  fi
}

[ -f "$config" ] || { echo "::error::缺少 $config"; exit 1; }
[ -f "$oldver" ] || { echo "::error::缺少基线 $oldver"; exit 1; }

# --- 1. 准备依赖（官方仓库包，无需 pip / 编译）---
# jq 用于读写 verfile，python3 用于解析 TOML 表名；nvchecker 自身依赖 python，
# 这里仍逐个检查，避免容器镜像预装状态变化时踩空。
need_pacman=false
for c in nvchecker jq python3; do
  command -v "$c" >/dev/null 2>&1 || need_pacman=true
done
if $need_pacman; then
  pacman -Sy --noconfirm archlinux-keyring >/dev/null
  pacman -S --needed --noconfirm nvchecker jq python >/dev/null
fi

# --- 2. GitHub 令牌：提高 API 配额（匿名 60 次/小时够用，但反复调试容易耗尽）。
# 写到仓库外的临时文件避免误提交；没有令牌也能跑，只是配额低。
keyfile_args=()
if [ -n "${GITHUB_TOKEN:-}" ]; then
  keyfile="$(mktemp "${RUNNER_TEMP:-/tmp}/nvchecker-keyfile.XXXXXX.toml")"
  chmod 600 "$keyfile"
  trap 'rm -f -- "$keyfile"' EXIT
  # TOML 基本字符串里的反斜杠与双引号需转义；令牌实际只含 [A-Za-z0-9_]
  printf '[keys]\ngithub = "%s"\n' "${GITHUB_TOKEN//[\"\\]/}" > "$keyfile"
  keyfile_args=(-k "$keyfile")
fi

# --- 3. 跑检测 ---
# 不给 --failures：单个源临时抽风不该让整轮检测作废，取不到版本的条目在下面
# 单独统计进 missing。nvchecker 只写 newver.json，oldver.json 保持为基线。
rm -f "$newver"
nvchecker "${keyfile_args[@]}" -c "$config"

[ -s "$newver" ] || { echo "::error::nvchecker 未产出 $newver"; exit 1; }

# --- 4. 汇总：配置里的全部条目 vs 本次取到版本的条目 ---
# verfile 结构：{"version":2,"data":{"<name>":{"version":"<ver>"}}}
# 源报错时 nvchecker 不写入该条目，故 newver 只含成功项。
mapfile -t conf_names < <(python3 - "$config" <<'PY'
import sys, tomllib
with open(sys.argv[1], 'rb') as f:
    cfg = tomllib.load(f)
for k in cfg:
    if k != '__config__':
        print(k)
PY
)
mapfile -t got_names < <(jq -r '(.data // {}) | keys[]' "$newver")

missing=()
for n in ${conf_names[@]+"${conf_names[@]}"}; do
  found=false
  for g in ${got_names[@]+"${got_names[@]}"}; do
    if [[ "$g" == "$n" ]]; then found=true; break; fi
  done
  $found || missing+=("$n")
done

echo "::group::上游版本对比"
jq -r --slurpfile old "$oldver" '
  (.data // {}) | to_entries[] | .key as $k
  | (.value.version // "") as $nv
  | (($old[0].data[$k].version) // "（无基线）") as $ov
  | "\($k): \($ov) -> \($nv)"
' "$newver" | sort
for n in ${missing[@]+"${missing[@]}"}; do
  echo "$n: （未取到版本）"
done
echo "::endgroup::"

# --- 5. 变化项 → 包名，并剔除 pkgs/ 下已不存在的目录 ---
changed_all="$(
  jq -r --slurpfile old "$oldver" '
    (.data // {}) | to_entries[]
    | .key as $k
    | (.value.version // "") as $nv
    | (($old[0].data[$k].version) // "") as $ov
    | select($nv != "" and $nv != $ov)
    | $k
  ' "$newver"
)"

changed=()
for name in $changed_all; do
  if [ -d "pkgs/$name" ]; then
    changed+=("$name")
  else
    echo "::warning::nvchecker 条目 $name 有更新，但 pkgs/$name 不存在，已忽略"
  fi
done

# 全军覆没通常意味着网络不可达或上游批量改版，而不是"上游没更新"。
# 这种情况必须报错，否则工作流会显示成功、实际什么都没检测到。
if [ ${#missing[@]} -eq ${#conf_names[@]} ]; then
  echo "::error::全部 ${#conf_names[@]} 个条目都未取到版本，疑似网络不可达或上游批量改版"
  exit 1
fi

# --- 6. 输出 ---
if [ ${#missing[@]} -gt 0 ]; then
  echo "::warning::以下条目本次未取到版本（上游源异常或结构变化）：${missing[*]}"
fi
emit missing "${missing[*]-}"

if [ ${#changed[@]} -eq 0 ]; then
  emit changed ''
  emit json '[]'
  emit has_updates false
  echo "上游无新版本"
else
  emit changed "${changed[*]}"
  emit json "$(printf '%s\n' "${changed[@]}" | jq -R . | jq -sc .)"
  emit has_updates true
  echo "上游有更新：${changed[*]}"
fi
