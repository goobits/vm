workspace=$1
shift
while test "$#" -ge 2; do
  name=$1
  destination=$2
  shift 2
  path="$workspace/$destination"
  test -d "$path" || continue
  path_root=$(CDPATH= cd -P "$path" 2>/dev/null && pwd) || continue
  repository_root=$(git -C "$path" rev-parse --show-toplevel 2>/dev/null) || continue
  repository_root=$(CDPATH= cd -P "$repository_root" 2>/dev/null && pwd) || continue
  test "$path_root" = "$repository_root" || continue
  printf '%s\t%s\n' "$name" "$destination"
done
