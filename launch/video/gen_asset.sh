#!/bin/zsh
# 用 Codex 内置 image_gen 生成一张素材：gen_asset.sh <名字> <提示词文件> [transparent]
# 生成结果复制为 assets/<名字>.png；日志写到 assets/logs/<名字>.log
set -e
here=${0:A:h}
name=$1; promptfile=$2; mode=${3:-opaque}
mkdir -p "$here/assets/logs"
bg="an opaque background"
[[ $mode == transparent ]] && bg="a genuinely transparent background (preserve the alpha channel)"
instr="Use the built-in image_gen tool (imagegen skill, built-in mode, not the CLI fallback) to generate exactly ONE image with $bg, then copy the generated PNG into the current directory as $name.png. Do nothing else and do not edit any other file.

Image prompt:
$(cat "$promptfile")"
cd "$here/assets"
codex exec --skip-git-repo-check --sandbox workspace-write -C "$here/assets" "$instr" > "$here/assets/logs/$name.log" 2>&1
ls -la "$here/assets/$name.png"
