#!/bin/sh
# runzoem.sh DEVICE INPUT.azm OUTPUT
#
# Run zoem in the current directory until its auxiliary files are stable. pud/man.zmm
# writes <base>.<device>.zmt (table of contents) and .zmr (references) during a run and
# reads them at the start of the next, so a change in sections or references needs a
# second run (plan/docstep1-notes.txt, item 3). Missing auxiliary files are created
# empty first, so that the first run does not report 'failure opening (rerun?)'.

set -eu

dev=$1
in=$2
out=$3
zoem=${ZOEM:-zoem}

# Markdown: the device directive \I (indent) is worth two spaces, the indent of the
# continuation lines of a list item ('- '); zoem's default tab length is 0.
tl=0
[ "$dev" = md ] && tl=2

base=$(basename "$in" .azm)
aux="$base.$dev"
touch "$aux.zmt" "$aux.zmr"

# zoem's messages are shown for the last run only: an earlier run reports the
# references that the .zmr does not hold yet (item 9). A reference still missing in
# the last run is an error (decision 5): zoem's ref.zmm reports it as '[REF] ...'.
msgs=$(mktemp)
trap 'rm -f "$msgs"' EXIT

i=0
while :; do
  before=$(cat "$aux.zmt" "$aux.zmr" | cksum)
  if ! "$zoem" -tl "$tl" -d "$dev" -i "$in" -o "$out" 2> "$msgs"; then
    cat "$msgs" >&2
    exit 1
  fi
  after=$(cat "$aux.zmt" "$aux.zmr" | cksum)
  [ "$before" = "$after" ] && break
  i=$((i + 1))
  if [ $i -ge 3 ]; then
    cat "$msgs" >&2
    echo "runzoem.sh: $aux.zmt/.zmr still changing after 3 runs" >&2
    exit 1
  fi
done
cat "$msgs" >&2
if grep -q '\[REF\]' "$msgs"; then
  echo "runzoem.sh: $in: reference to an unknown id (device $dev)" >&2
  exit 1
fi
