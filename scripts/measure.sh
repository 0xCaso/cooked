#!/usr/bin/env bash
# Measure one running app: CPU, WindowServer CPU, memory footprint, energy, GPU.
#
#   scripts/measure.sh <process-name> <state-label> [runs=3] [secs=60]
#
# Prints one TSV line per run, then a "median" line. Columns:
#   app state run cpu_pct ws_cpu_pct footprint_mb energy_impact pkg_mw gpu_mw
# cpu_pct and ws_cpu_pct come from powermetrics' per-task CPU time. To get the
# WindowServer change an app causes, also run once with the app quit (any
# process name that is not running gives app columns of 0) and subtract.
# powermetrics needs root: pass a file holding the sudo password in
# SUDO_PASS_FILE, or run with cached sudo credentials.
set -euo pipefail

app=${1:?usage: measure.sh <process-name> <state-label> [runs] [secs]}
state=${2:?state label missing}
runs=${3:-3}
secs=${4:-60}

sudo_run() {
  if [[ -n "${SUDO_PASS_FILE:-}" ]]; then
    sudo -S -p '' "$@" <"$SUDO_PASS_FILE"
  else
    sudo -n "$@"
  fi
}

pid=$(pgrep -x "$app" | head -1 || true)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

printf 'app\tstate\trun\tcpu_pct\tws_cpu_pct\tfootprint_mb\tenergy_impact\tpkg_mw\tgpu_mw\n'
for ((r = 1; r <= runs; r++)); do
  out="$tmp/run$r.plist"
  sudo_run powermetrics --samplers cpu_power,gpu_power,tasks --show-process-energy \
    -f plist -i 1000 -n "$secs" >"$out" 2>/dev/null
  fp=0
  if [[ -n "$pid" ]]; then
    fp=$(footprint -p "$pid" 2>/dev/null | sed -nE 's/.*Footprint: ([0-9.]+) (KB|MB|GB).*/\1 \2/p' | head -1 |
      awk '{ v = $1; if ($2 == "KB") v /= 1024; if ($2 == "GB") v *= 1024; printf "%.1f", v }')
  fi
  /usr/bin/python3 -I - "$out" "$app" "$state" "$r" "${fp:-0}" <<'PY'
import plistlib, statistics, sys
path, app, state, run, fp = sys.argv[1:]
raw = open(path, "rb").read()
cpu, ws, energy, pkg, gpu = [], [], [], [], []
for chunk in raw.split(b"\0"):
    chunk = chunk.strip()
    if not chunk:
        continue
    s = plistlib.loads(chunk)
    tasks = s.get("tasks", [])
    a = [t for t in tasks if t.get("name") == app]
    w = [t for t in tasks if t.get("name") == "WindowServer"]
    cpu.append(sum(t.get("cputime_ms_per_s", 0) for t in a) / 10)
    ws.append(sum(t.get("cputime_ms_per_s", 0) for t in w) / 10)
    energy.append(sum(t.get("energy_impact_per_s", t.get("energy_impact", 0)) for t in a))
    proc = s.get("processor", {})
    pkg.append(proc.get("combined_power", proc.get("package_power", 0)))
    gpu.append(proc.get("gpu_power", 0))
m = lambda xs: statistics.mean(xs) if xs else 0
print(f"{app}\t{state}\t{run}\t{m(cpu):.2f}\t{m(ws):.2f}\t{fp}\t{m(energy):.2f}\t{m(pkg):.0f}\t{m(gpu):.0f}")
PY
done | tee "$tmp/lines.tsv"

/usr/bin/python3 -I - "$tmp/lines.tsv" "$app" "$state" <<'PY'
import statistics, sys
path, app, state = sys.argv[1:]
rows = [l.rstrip("\n").split("\t") for l in open(path) if l.strip()]
if rows:
    cols = [statistics.median(float(r[c]) for r in rows) for c in range(3, 9)]
    print("\t".join([app, state, "median"] + [f"{v:g}" for v in cols]))
PY
