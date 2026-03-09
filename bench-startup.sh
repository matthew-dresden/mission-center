#!/usr/bin/env bash
# Benchmark startup time by running the app N times in benchmark mode.
# Usage: ./bench-startup.sh [N]   (default: 50)
set -e
cd "$(dirname "$0")"

N=${1:-50}
LABEL=${2:-baseline}

export GSETTINGS_SCHEMA_DIR="$PWD/builddir/data"
export MC_RESOURCE_DIR="$PWD/builddir/resources"
export MC_BENCHMARK=1
# Suppress GTK warnings to keep output clean
export G_MESSAGES_DEBUG=""

# Make sure it's built with benchmark instrumentation
meson configure builddir -Dbenchmark=true 2>/dev/null || meson setup builddir -Dbenchmark=true 2>/dev/null
ninja -C builddir 2>&1 | tail -3

TMPFILE=$(mktemp /tmp/mc-bench-XXXXXX.txt)
TOTALS_FILE=$(mktemp /tmp/mc-bench-totals-XXXXXX.txt)

echo "=== Mission Center Startup Benchmark ==="
echo "Label: $LABEL"
echo "Runs:  $N"
echo ""

for i in $(seq 1 "$N"); do
    printf "\r  Running %d/%d..." "$i" "$N" >&2
    # Run the app, capture BENCH lines
    timeout 60 ./builddir/src/missioncenter 2>&1 | grep "^BENCH" >> "$TMPFILE"
    echo "---" >> "$TMPFILE"
    # Extract BENCH_TOTAL for this run
    tail -5 "$TMPFILE" | grep "^BENCH_TOTAL:" | awk '{print $2}' >> "$TOTALS_FILE"
done

echo "" >&2
echo ""

# Print per-phase averages
echo "=== Per-Phase Averages (ms) ==="
for phase in \
    "gettext init" \
    "resources loaded" \
    "GTK app created" \
    "activate.. entered" \
    "MagpieClient::new.. done" \
    "Window::new.. done" \
    "set_initial_readings.. entered" \
    "perf_page.set_initial_readings" \
    "apps_page.set_initial_readings" \
    "services_page.set_initial_readings" \
    "set_initial_readings.. done" \
; do
    # Use a flexible grep pattern
    vals=$(grep -oP "BENCH:.*${phase}[^:]*: \K[0-9.]+" "$TMPFILE" || true)
    if [ -n "$vals" ]; then
        avg=$(echo "$vals" | awk '{s+=$1; n++} END {if(n>0) printf "%.1f", s/n; else print "N/A"}')
        min=$(echo "$vals" | sort -n | head -1)
        max=$(echo "$vals" | sort -n | tail -1)
        printf "  %-45s avg=%7s  min=%7s  max=%7s\n" "$phase:" "$avg" "$min" "$max"
    fi
done

echo ""
echo "=== TOTAL Startup Time (ms) ==="
if [ -s "$TOTALS_FILE" ]; then
    avg=$(awk '{s+=$1; n++} END {printf "%.1f", s/n}' "$TOTALS_FILE")
    min=$(sort -n "$TOTALS_FILE" | head -1)
    max=$(sort -n "$TOTALS_FILE" | tail -1)
    med=$(sort -n "$TOTALS_FILE" | awk -v n="$N" 'NR==int(n/2)+1{print}')
    p95=$(sort -n "$TOTALS_FILE" | awk -v n="$N" 'NR==int(n*0.95)+1{print}')
    echo "  Average: ${avg}ms"
    echo "  Median:  ${med}ms"
    echo "  Min:     ${min}ms"
    echo "  Max:     ${max}ms"
    echo "  P95:     ${p95}ms"
fi

echo ""
echo "Raw data: $TMPFILE"
echo "Totals:   $TOTALS_FILE"

rm -f "$TMPFILE" "$TOTALS_FILE"
