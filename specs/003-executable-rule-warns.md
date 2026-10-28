# Executable Rule Warns

Some upstream projects publish archives whose `bin/` holds no file with an executable bit.
Apache Tomcat is the first case: its `.zip` stores every `bin/*.sh` as `0644`, while its
`.tar.gz` of the same release keeps them executable. SDKMAN has served Tomcat's zips for
years, and the bash CLI routes Tomcat to the zip post-install hook, so a Tomcat tarball can't
be served without changes to `sdkman-hooks`. Today the check fails every Tomcat zip, which
blocks the candidate from community requests altogether.

This spec loosens that one rule. It amends
[`001-community-contributions.md`](001-community-contributions.md) and
[`002-review-follow-ups.md`](002-review-follow-ups.md); where they disagree, this one wins.

*Reference: dry run of a `tomcat` 11.0.27 release request against `main` at `0f7fcbc` on
2026-10-28.*

## Behaviour

**A `bin/` without an executable file warns.** An archive whose `bin/` holds no executable
file gets a `WARN`, and the check still passes. The rule still applies to every platform
except `WINDOWS_X64`, which never gets it. The `WARN` names the base directory, as the
failure did: `no executable file in <base>/bin/`.

Every other archive rule keeps its verdict. An archive with no `bin/` under its base
directory still fails.

**`contrib`'s own releases stay strict.** Publishing a `contrib` release already refuses any
archive with a warning, so a `contrib` archive without an executable `bin/contrib` still fails
the release.

## Business Rules

1. **The archive rules table.** 001 rule 8's table now reads:

   | Fails | Warns |
   |---|---|
   | the URL does not name a file ending in `.zip` or `.tar.gz`, or does not end in a `200` after at most 5 `https` redirects | the base directory is not `<candidate>-<version>` |
   | the archive is not a zip or tar.gz | the file name does not follow the convention |
   | it does not hold exactly one base directory | `bin/` holds no executable file (any platform except `WINDOWS_X64`) |
   | there is no `bin/` directly under the base directory | |

2. **The README agrees.** The README's archive table moves the rule from *Fails* to *Warns*.

## Out of Scope

- Changing `sdkman-hooks` to serve Tomcat, or any candidate, a post-install hook chosen by
  archive type.
- Fixing the bash CLI's checksum handling.
- Making executables of a contributor's archive.

## Acceptance Criteria

- [ ] A release whose archive's `bin/` holds no executable file gets a `WARN` naming
      `<base>/bin/`, and the check passes
- [ ] A `WINDOWS_X64` archive without an executable file in `bin/` gets no `WARN` for it
- [ ] An archive without a `bin/` under its base directory still fails the check
- [ ] Publishing a `contrib` release whose archive's `bin/` holds no executable file still fails
- [ ] The README lists the rule under *Warns*, not *Fails*
