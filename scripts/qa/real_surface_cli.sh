#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'usage: real_surface_cli.sh --bin PATH --evidence PATH\n' >&2
}

if [ "$#" -ne 4 ]; then
  usage
  exit 2
fi

BIN=""
EVIDENCE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin)
      BIN="$2"
      shift 2
      ;;
    --evidence)
      EVIDENCE="$2"
      shift 2
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [ ! -x "$BIN" ]; then
  printf 'missing executable binary: %s\n' "$BIN" >&2
  exit 2
fi

mkdir -p "$(dirname "$EVIDENCE")"
: >"$EVIDENCE"
TMP_BASE="$(mktemp -d "${TMPDIR:-/tmp}/jikji-f3-real-qa.XXXXXX")"
ROOT="$TMP_BASE/root"
GUI_PID=""

cleanup() {
  local status="$1"
  {
    printf '\n## Cleanup\n'
    if [ -n "$GUI_PID" ]; then
      printf 'kill gui pid: %s\n' "$GUI_PID"
      kill "$GUI_PID" 2>/dev/null || true
      wait "$GUI_PID" 2>/dev/null || true
    fi
    printf 'remove temp root: %s\n' "$TMP_BASE"
  } >>"$EVIDENCE"
  rm -rf "$TMP_BASE"
  exit "$status"
}
trap 'cleanup "$?"' EXIT

mkdir -p "$ROOT/contracts" "$ROOT/media"
printf 'ACME renewal contract alpha marker\n' >"$ROOT/contracts/acme-renewal.md"
printf 'invoice 2026 payment terms for ACME\n' >"$ROOT/contracts/invoice.txt"
printf 'notes about graph route evidence\n' >"$ROOT/notes.txt"

log() {
  printf '\n## %s\n' "$1" >>"$EVIDENCE"
}

run_capture() {
  local name="$1"
  shift
  local stdout_file="$TMP_BASE/$name.stdout"
  local stderr_file="$TMP_BASE/$name.stderr"
  log "$name"
  printf '$' >>"$EVIDENCE"
  printf ' %q' "$@" >>"$EVIDENCE"
  printf '\n' >>"$EVIDENCE"
  set +e
  "$@" >"$stdout_file" 2>"$stderr_file"
  local code="$?"
  set -e
  {
    printf 'exit_code=%s\n' "$code"
    printf -- '--- stdout ---\n'
    cat "$stdout_file"
    printf -- '\n--- stderr ---\n'
    cat "$stderr_file"
    printf '\n'
  } >>"$EVIDENCE"
  if [ "$code" -ne 0 ]; then
    printf 'command failed: %s\n' "$name" >&2
    exit "$code"
  fi
}

assert_json_key() {
  local file="$1"
  local key="$2"
  python3 - "$file" "$key" <<'PY'
from __future__ import annotations
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
if sys.argv[2] not in payload:
    raise SystemExit(f"missing key: {sys.argv[2]}")
PY
}

assert_http_status() {
  local file="$1"
  local status="$2"
  if ! head -n 1 "$file" | grep -q " $status "; then
    printf 'expected HTTP %s in %s\n' "$status" "$file" >&2
    exit 1
  fi
}

printf 'F3 real surface QA\n' >>"$EVIDENCE"
printf 'binary=%s\nroot=%s\n' "$BIN" "$ROOT" >>"$EVIDENCE"

run_capture prepare "$BIN" prepare "$ROOT" --json
assert_json_key "$TMP_BASE/prepare.stdout" root

run_capture find-json "$BIN" find "$ROOT" "ACME renewal" --json
assert_json_key "$TMP_BASE/find-json.stdout" candidates

run_capture search-json "$BIN" search "$ROOT" "ACME" --json
assert_json_key "$TMP_BASE/search-json.stdout" candidates

run_capture brief-compact-json "$BIN" brief "$ROOT" "invoice" --compact --json
assert_json_key "$TMP_BASE/brief-compact-json.stdout" candidates

run_capture graph-status "$BIN" graph "$ROOT" status --json
assert_json_key "$TMP_BASE/graph-status.stdout" stats

run_capture clean-dry-run "$BIN" clean "$ROOT" --dry-run --json
assert_json_key "$TMP_BASE/clean-dry-run.stdout" dry_run

run_capture doctor-json "$BIN" doctor "$ROOT" --json
assert_json_key "$TMP_BASE/doctor-json.stdout" ok

run_capture gui-background "$BIN" gui "$ROOT" --host 127.0.0.1 --port 0 --background --json
GUI_URL="$(python3 - "$TMP_BASE/gui-background.stdout" <<'PY'
from __future__ import annotations
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(payload["url"])
PY
)"
GUI_PID="$(python3 - "$TMP_BASE/gui-background.stdout" <<'PY'
from __future__ import annotations
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(payload["pid"])
PY
)"
GUI_TOKEN="$(python3 - "$TMP_BASE/gui-background.stdout" <<'PY'
from __future__ import annotations
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(payload["manage_token"])
PY
)"

run_capture gui-status curl -i "$GUI_URL/api/status"
assert_http_status "$TMP_BASE/gui-status.stdout" 200

run_capture gui-search curl -i "$GUI_URL/api/search?q=ACME"
assert_http_status "$TMP_BASE/gui-search.stdout" 200

run_capture gui-find curl -i "$GUI_URL/api/find?q=invoice"
assert_http_status "$TMP_BASE/gui-find.stdout" 200

run_capture gui-refresh curl -i -X POST "$GUI_URL/api/refresh?token=$GUI_TOKEN"
assert_http_status "$TMP_BASE/gui-refresh.stdout" 200

run_capture gui-download-traversal curl -i "$GUI_URL/download?path=../outside.txt"
assert_http_status "$TMP_BASE/gui-download-traversal.stdout" 403

printf '\nPASS: real surface CLI and GUI QA completed\n' >>"$EVIDENCE"
