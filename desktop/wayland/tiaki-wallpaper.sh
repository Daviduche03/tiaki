#!/bin/sh
set -eu

dir=/usr/share/tiaki/wallpapers
state_dir="${XDG_CONFIG_HOME:-$HOME/.config}/tiaki"
state="$state_dir/wallpaper"
mkdir -p "$state_dir"

current=""
if [ -f "$state" ]; then
  current=$(cat "$state")
fi

mode="${1:-init}"
case "$mode" in
  init)
    if [ -z "$current" ] || [ ! -f "$dir/$current" ]; then
      current=drawn-01-ink-field.jpg
    fi
    ;;
  next|prev)
    files=$(cd "$dir" && ls *.jpg 2>/dev/null | sort)
    [ -z "$files" ] && exit 1
    count=0
    for file in $files; do count=$((count + 1)); done
    index=0
    position=1
    for file in $files; do
      if [ "$file" = "$current" ]; then index=$position; fi
      position=$((position + 1))
    done
    if [ "$mode" = next ]; then
      index=$((index % count + 1))
    else
      index=$((index - 1))
      [ "$index" -lt 1 ] && index=$count
    fi
    current=$(printf '%s\n' $files | sed -n "${index}p")
    ;;
  *)
    current="$mode"
    ;;
esac

if [ ! -f "$dir/$current" ]; then
  current=drawn-01-ink-field.jpg
fi
printf '%s\n' "$current" > "$state"
pkill -x swaybg 2>/dev/null || true
sleep 0.2
swaybg -i "$dir/$current" >/dev/null 2>&1 &
