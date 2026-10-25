#!/usr/bin/env bash
set -euo pipefail

mode=apply
case "$#:${1:-}" in
  0:) ;;
  1:--check) mode=check ;;
  *)
    echo "usage: $0 [--check]" >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rulesets=("$root"/.github/rulesets/*.json)
api=repos/sdkman/sdkman-contrib/rulesets

placeholders=0
for file in "${rulesets[@]}"; do
  if jq -e 'any(.bypass_actors[]?; .actor_type == "Integration" and .actor_id == 0)' "$file" >/dev/null; then
    echo "$(basename "$file") names an Integration bypass actor with actor_id 0; commit the real App id" >&2
    placeholders=1
  fi
done
if [ "$placeholders" -ne 0 ]; then
  exit 2
fi

live_rulesets="$(gh api "$api?includes_parents=false")"

# Keep only the parameters the committed rule names: GitHub fills in defaults for the rest.
# shellcheck disable=SC2016 # jq variables, not shell ones
normalise_live='
  .rules |= map(
      . as $rule
      | ($committed.rules | map(select(.type == $rule.type)) | first) as $match
      | if $match == null then .
        elif $match.parameters == null then del(.parameters)
        elif ($rule.parameters | type) == "object" then
          .parameters |= with_entries(select(.key as $key | $match.parameters | has($key)))
        else .
        end)'
shape='{enforcement, conditions, rules: (.rules | sort_by(.type)), bypass_actors: (.bypass_actors | sort_by(.actor_id, .actor_type))}'

drifted=0
for file in "${rulesets[@]}"; do
  name="$(jq -r '.name' "$file")"
  id="$(jq -r --arg name "$name" 'first(.[] | select(.name == $name) | .id) // empty' <<<"$live_rulesets")"

  if [ "$mode" = apply ]; then
    if [ -z "$id" ]; then
      gh api --method POST "$api" --input "$file"
    else
      gh api --method PUT "$api/$id" --input "$file"
    fi
    continue
  fi

  if [ -z "$id" ]; then
    echo "$name is absent from the live repository" >&2
    drifted=1
    continue
  fi
  expected="$(jq -S "$shape" "$file")"
  actual="$(gh api "$api/$id" | jq -S --argjson committed "$(cat "$file")" "$normalise_live | $shape")"
  if [ "$expected" != "$actual" ]; then
    echo "$name differs from the live ruleset" >&2
    drifted=1
  fi
done

exit "$drifted"
