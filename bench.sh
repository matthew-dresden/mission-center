#!/usr/bin/env bash
# Benchmark startup time over N runs, extracting key phases
set -e
cd "$(dirname "$0")"

RUNS=${1:-50}
LABEL=${2:-baseline}
OUTFILE="bench_${LABEL}.csv"

BIN="./builddir/src/missioncenter"
export GSETTINGS_SCHEMA_DIR="$PWD/builddir/data"
export MC_RESOURCE_DIR="$PWD/builddir/resources"
export MC_BENCHMARK=1

# Make sure it's built with benchmark instrumentation
meson configure builddir -Dbenchmark=true 2>/dev/null || meson setup builddir -Dbenchmark=true 2>/dev/null
ninja -C builddir 2>&1 | tail -3

# Cleanup before starting
pkill -9 -f "missioncenter-magpie" 2>/dev/null || true
pkill -9 -f "missioncenter" 2>/dev/null || true
rm -f /tmp/magpie_*.ipc 2>/dev/null || true
sleep 0.5

echo "run,gtk_app_created,activate_entered,magpie_done,window_done,ipc_all_done,idle_posted,idle_dispatched,initial_readings_entered,perf_page,apps_page,services_page,total,status" > "$OUTFILE"

echo "=== Running $RUNS iterations ($LABEL) ==="
echo ""
printf "%-4s  %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s  %s\n" \
  "Run" "GTK" "Activ" "Magpie" "Window" "IPC" "IdleP" "IdleD" "InitR" "Perf" "Apps" "Svcs" "TOTAL" "Status"
printf '%0.s-' {1..130}
echo ""

HUNG=0

for i in $(seq 1 "$RUNS"); do
    OUTPUT=$(timeout 15 "$BIN" 2>&1) && STATUS="ok" || STATUS="timeout"

    if [ "$STATUS" = "timeout" ]; then
        HUNG=$((HUNG+1))
    fi

    extract() {
        echo "$OUTPUT" | grep -m1 "$1" | grep -oP '[\d.]+(?=ms)' || echo "NA"
    }

    gtk_app=$(extract "BENCH: GTK app created:")
    activate=$(extract "BENCH: activate() entered:")
    magpie=$(extract "BENCH: MagpieClient::new() done:")
    window=$(extract "BENCH: Window::new() done:")
    ipc_all=$(extract "BENCH: gather_and_proxy all IPC done:")
    idle_posted=$(extract "BENCH: idle_add_once posted:")
    idle_dispatched=$(extract "BENCH: idle_add_once DISPATCHED")
    init_enter=$(extract "BENCH: set_initial_readings() entered:")
    perf=$(extract "BENCH:   perf_page")
    apps=$(extract "BENCH:   apps_page")
    services=$(extract "BENCH:   services_page")
    total=$(echo "$OUTPUT" | grep -m1 "BENCH_TOTAL:" | awk '{print $2}' || echo "NA")

    echo "$i,$gtk_app,$activate,$magpie,$window,$ipc_all,$idle_posted,$idle_dispatched,$init_enter,$perf,$apps,$services,$total,$STATUS" >> "$OUTFILE"

    printf "%-4d  %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s %-8s  %s\n" \
      "$i" "$gtk_app" "$activate" "$magpie" "$window" "$ipc_all" "$idle_posted" "$idle_dispatched" "$init_enter" "$perf" "$apps" "$services" "$total" "$STATUS"

    # Cleanup between runs
    sleep 0.3
    pkill -9 -f "missioncenter-magpie" 2>/dev/null || true
    rm -f /tmp/magpie_*.ipc 2>/dev/null || true
done

echo ""
echo "=== Summary ($LABEL) — $RUNS runs, $HUNG hung ==="
echo "Saved to $OUTFILE"
echo ""

# Print summary stats
awk -F',' 'NR>1 && $NF=="ok" && $(NF-1)!="NA" {
    n++; sum+=$(NF-1); v=$(NF-1);
    if(n==1 || v<min) min=v;
    if(n==1 || v>max) max=v;
    vals[n]=v;

    # Phase breakdowns (accumulate for averages)
    s_gtk+=$2; s_act+=$3; s_mag+=$4; s_win+=$5; s_ipc+=$6; s_idlep+=$7; s_idled+=$8; s_init+=$9; s_perf+=$10; s_apps+=$11; s_svc+=$12;
}
END {
    if(n==0) { print "No valid runs"; exit 1 }
    avg=sum/n;

    # Sort for median/p95
    for(i=1;i<=n;i++) for(j=i+1;j<=n;j++) if(vals[i]>vals[j]) {t=vals[i];vals[i]=vals[j];vals[j]=t}
    med=vals[int(n/2)+1];
    p95=vals[int(n*0.95)+1];

    printf "  Total (ms):  min=%.1f  avg=%.1f  median=%.1f  p95=%.1f  max=%.1f  (n=%d)\n", min, avg, med, p95, max, n;
    printf "\n  Phase averages (ms, cumulative from startup):\n";
    printf "    GTK app created:             %7.1f\n", s_gtk/n;
    printf "    activate() entered:          %7.1f\n", s_act/n;
    printf "    MagpieClient::new() done:    %7.1f\n", s_mag/n;
    printf "    Window::new() done:          %7.1f\n", s_win/n;
    printf "    All IPC done (bg thread):    %7.1f\n", s_ipc/n;
    printf "    idle_add_once posted:        %7.1f\n", s_idlep/n;
    printf "    idle dispatched (main):      %7.1f\n", s_idled/n;
    printf "    set_initial_readings entered:%7.1f\n", s_init/n;
    printf "    perf_page init:              %7.1f\n", s_perf/n;
    printf "    apps_page init:              %7.1f\n", s_apps/n;
    printf "    services_page init:          %7.1f\n", s_svc/n;
}' "$OUTFILE"

echo ""