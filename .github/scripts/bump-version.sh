#!/usr/bin/env bash
# Bumps the workspace's minor version and prints the new one.
set -euo pipefail

current=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml)
IFS=. read -r major minor _ <<<"$current"
next="$major.$((minor + 1)).0"

perl -pi -e "s/^version = \"\Q$current\E\"\$/version = \"$next\"/" Cargo.toml
cargo update --workspace --quiet

echo "$next"
