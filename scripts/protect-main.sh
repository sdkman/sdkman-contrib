#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <app-id>" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

jq --argjson app_id "$1" '.bypass_actors[0].actor_id = $app_id' "$root/.github/rulesets/main.json" |
  gh api --method POST repos/sdkman/sdkman-contrib/rulesets --input -
