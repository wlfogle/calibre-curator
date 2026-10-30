#!/usr/bin/env fish
set repo (dirname (status --current-filename))
if not test -x "$repo/target/release/calibre-curator"
    printf '%s\n' "Build first: pnpm --dir $repo package" >&2
    exit 1
end
exec "$repo/target/release/calibre-curator" $argv
