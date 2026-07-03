#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: build-artifacts.sh [--target TARGET] [--version VERSION] [--bin NAME]
                          [--package PACKAGE] [--out-dir DIR] [--dry-run]

Build or dry-run package a Jikji release artifact and write a SHA-256 checksum.

Options:
  --target TARGET     Rust target triple. Defaults to the host triple.
  --version VERSION   Version string for the archive name. Defaults to Cargo.toml.
  --bin NAME          Binary name to package. Defaults to jikji.
  --package PACKAGE   Cargo package to build. Defaults to jikji-cli.
  --out-dir DIR       Artifact directory. Defaults to target/release-artifacts.
  --dry-run           Create a deterministic placeholder artifact without cargo build.
  -h, --help          Show this help.
USAGE
}

target=""
version=""
bin_name="jikji"
package_name="jikji-cli"
out_dir="target/release-artifacts"
dry_run=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --target)
      [ "$#" -ge 2 ] || { echo "missing value for --target" >&2; exit 2; }
      target="$2"
      shift 2
      ;;
    --version)
      [ "$#" -ge 2 ] || { echo "missing value for --version" >&2; exit 2; }
      version="$2"
      shift 2
      ;;
    --bin)
      [ "$#" -ge 2 ] || { echo "missing value for --bin" >&2; exit 2; }
      bin_name="$2"
      shift 2
      ;;
    --package)
      [ "$#" -ge 2 ] || { echo "missing value for --package" >&2; exit 2; }
      package_name="$2"
      shift 2
      ;;
    --out-dir)
      [ "$#" -ge 2 ] || { echo "missing value for --out-dir" >&2; exit 2; }
      out_dir="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

python_bin="${PYTHON:-python3}"
if ! command -v "$python_bin" >/dev/null 2>&1; then
  python_bin="python"
fi
if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "python3 or python is required for archive creation" >&2
  exit 1
fi

if [ -z "$target" ]; then
  target="$(rustc -vV | sed -n 's/^host: //p')"
fi
if [ -z "$target" ]; then
  echo "could not determine target triple" >&2
  exit 1
fi

if [ -z "$version" ]; then
  version="$("$python_bin" - <<'PY'
from pathlib import Path
for line in Path("Cargo.toml").read_text(encoding="utf-8").splitlines():
    if line.startswith("version = "):
        print(line.split("=", 1)[1].strip().strip('"'))
        raise SystemExit(0)
raise SystemExit("workspace version not found")
PY
)"
fi

case "$version" in
  ""|*/*|*\\*|*..*)
    echo "invalid --version: must be a path-safe version component" >&2
    exit 2
    ;;
esac

case "$target" in
  *windows*)
    binary_name="${bin_name}.exe"
    archive_ext="zip"
    ;;
  *)
    binary_name="$bin_name"
    archive_ext="tar.gz"
    ;;
esac

mkdir -p "$out_dir"
staging="$(mktemp -d "${TMPDIR:-/tmp}/jikji-artifact.XXXXXX")"
cleanup() {
  rm -rf "$staging"
}
trap cleanup EXIT

archive_name="${bin_name}-${version}-${target}.${archive_ext}"
archive_path="${out_dir}/${archive_name}"

if [ "$dry_run" -eq 1 ]; then
  mkdir -p "$staging"
  printf '#!/usr/bin/env sh\nprintf "jikji dry-run artifact %s %s\\n"\n' "$version" "$target" > "$staging/$binary_name"
  chmod +x "$staging/$binary_name"
else
  cargo build --release -p "$package_name" --bin "$bin_name" --target "$target"
  binary_path="target/${target}/release/${binary_name}"
  if [ ! -f "$binary_path" ]; then
    echo "built binary not found: $binary_path" >&2
    exit 1
  fi
  cp "$binary_path" "$staging/$binary_name"
fi

[ -f README.md ] && cp README.md "$staging/README.md"
[ -f LICENSE ] && cp LICENSE "$staging/LICENSE"

"$python_bin" - "$staging" "$archive_path" "$archive_name" "$archive_ext" <<'PY'
from __future__ import annotations

import hashlib
import os
import sys
import tarfile
import zipfile
from pathlib import Path

staging = Path(sys.argv[1])
archive_path = Path(sys.argv[2])
archive_name = sys.argv[3]
archive_ext = sys.argv[4]
root_name = archive_name.removesuffix(".tar.gz").removesuffix(".zip")
archive_path.parent.mkdir(parents=True, exist_ok=True)

files = sorted(path for path in staging.iterdir() if path.is_file())
if archive_ext == "zip":
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in files:
            zf.write(path, f"{root_name}/{path.name}")
else:
    with tarfile.open(archive_path, "w:gz") as tf:
        for path in files:
            info = tf.gettarinfo(path, arcname=f"{root_name}/{path.name}")
            if os.access(path, os.X_OK):
                info.mode = 0o755
            with path.open("rb") as src:
                tf.addfile(info, src)

digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
checksum_path = archive_path.with_name(f"{archive_path.name}.sha256")
checksum_path.write_text(f"{digest}  {archive_path.name}\n", encoding="utf-8")
print(archive_path)
print(checksum_path)
PY
