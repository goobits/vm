root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools/state"
for state in "$root"/*.state; do
  test -f "$state" || continue
  cat "$state"
done
