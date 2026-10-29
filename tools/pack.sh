#!/usr/bin/env bash
# 打包成宿主可导入的模块 zip。
#
# 形状要求（docs/plugins.md「第三方模块」节）：zip 根目录**平铺** module.json 与 entry
# 指向的可执行文件——不要多套一层目录。
set -euo pipefail

TARGET="${1:-x86_64-pc-windows-gnu}"
OUT_DIR="${2:-dist}"
EXE="target/${TARGET}/release/game-buddy.exe"

if [[ ! -f "$EXE" ]]; then
  echo "缺少产物：$EXE" >&2
  echo "先跑：cargo build --release --target $TARGET" >&2
  exit 1
fi

VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ -z "$VERSION" ]]; then
  echo "无法从 Cargo.toml 读出版本号" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
ZIP="${OUT_DIR}/game-buddy-${VERSION}.zip"
rm -f "$ZIP"

# -j 摊平路径：module.json 与 exe 必须待在 zip 根
if command -v zip >/dev/null 2>&1; then
  zip -q -j "$ZIP" module.json "$EXE"
else
  # 没有 zip 命令时用 Python 的 zipfile（结果等价：两个文件都在 zip 根）
  python3 - "$ZIP" module.json "$EXE" <<'PY'
import sys, zipfile
zip_path, *files = sys.argv[1:]
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
    for path in files:
        import os
        zf.write(path, arcname=os.path.basename(path))
PY
fi

echo "已产出 ${ZIP}"
python3 - "$ZIP" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as zf:
    for info in zf.infolist():
        print(f"  {info.file_size:>10}  {info.filename}")
PY
