# Community Contributions

`sdkman-db-migrations` was how the community added candidates and versions to SDKMAN: a
contributor wrote a Scala changelog, a maintainer merged it, and the merge wrote MongoDB. It
was archived on 2026-10-07, when Postgres became the master datastore, and its 17 open
candidate PRs were closed with a promise of a new process.

This repo is that process. A contributor writes one small YAML file and opens a PR. CI checks
the file, and also checks every archive it points at, then posts the result on the PR. A
SDKMAN maintainer reviews and merges it. A job on `main` then writes the request to the State
API and records a receipt. Contributors need no Docker, no JVM and no build; editing YAML is
the whole job, and `contrib new` writes the skeleton for them.

*Reference: phase 3 of the MongoDB to PostgreSQL move, end-game §14
([`application-end-game.md`](../../../docs/specs/application-end-game.md)). Settled in a
grilling session on 2026-10-07. Depends on a `community` role and public validation in
`sdkman-state`, specced separately (see* What State must provide*). The well-formed archive
rules are the State wiki's
[Well-formed SDK Archives](https://github.com/sdkman/sdkman-state/wiki/Well-formed-SDK-Archives).*

## Behaviour

**A request is a one-shot change, not desired state.** A merged file asks State to make a
change once. State stays the master copy of every candidate and version, and the repo is a
log of what was asked and when it was applied. The repo never reconciles State to match it,
so it never fights other writers (DISCO, the Vendor API, `vendor-release`) and never deletes
anything.

**There are two kinds of request, and a new candidate takes two PRs.** A *registration*
registers a candidate or updates its metadata. A *release* adds or overwrites versions of a
registered candidate and can move its default. One file is one kind, never both. A new
candidate is a registration PR, followed by a release PR once the registration is applied.
The release PR's check fails until then, because the candidate is not yet in the registry.

**One PR carries exactly one request.** It adds one file under `requests/` and changes
nothing else. Review stays small, and an apply failure points at exactly one request.

**Requests are immutable.** A merged request file is never edited or deleted. A correction
is a new request, which overwrites what the earlier one wrote.

**Every PR is approved by a SDKMAN maintainer.** Approval is never delegated to candidate
owners or vendors. `main` is protected: changes land only through approved PRs, except for
the apply bot's receipts.

**Applying happens after merge, in file-name order, one run at a time.** Each push to `main`
starts an apply run. Runs queue and are never cancelled. A run applies every *pending*
request in the order of their file names, which start with a UTC timestamp, across all
candidates, and stops at the first failure. A request is pending until its
receipt exists. A failed request therefore stays pending, is retried by the next run, and
blocks every request ordered after it until it succeeds or a maintainer intervenes. That
keeps a release from landing before the registration it depends on.

**Applying is safe to repeat.** Every write is an upsert, so re-running a request that half
applied re-does the parts already applied without changing anything, then finishes the rest.
There is no rollback.

**A receipt records what State received.** After a request applies, the bot commits a
receipt under `.applied/` that mirrors the request's path. The receipt is the only thing that
marks a request as applied. It records what was actually sent, including checksums that CI
computed and the contributor never wrote.

## Repository layout

```
requests/
  jpx/
    20261008T140211Z-register-jpx.yaml
    20261014T094057Z-release-1.2.0.yaml
  tomcat/
    20261009T081500Z-release-11.0.15.yaml
.applied/                                   # bot-owned receipts, same relative paths
  jpx/
    20261008T140211Z-register-jpx.yaml
  tomcat/
    20261009T081500Z-release-11.0.15.yaml
```

A request lives at `requests/<candidate>/<timestamp>-<kind>-<slug>.yaml`:

| Part | Rule |
|---|---|
| `<candidate>` | the directory; must match the file's `candidate` |
| `<timestamp>` | when the request was written, in UTC, ISO 8601 basic format: `YYYYMMDDTHHMMSSZ` |
| `<kind>` | `register` or `release`; must match the file's content |
| `<slug>` | lowercase letters, digits, `.` and `-`; by convention the candidate for a registration and the version for a release |

The timestamp leads, so names sort as text into the order they were written, with no
colons to trip Windows checkouts. Pending requests apply in that order, comparing file names
across every candidate directory, with the full name as the tie-break. `contrib new` stamps
the current time. Hand-written names are fine as long as they match.

## Request format

### Registration

```yaml
candidate: jpx
register:
  name: JPX
  description: JPX is a command-line tool for ...
  website: https://jpx.dev/
```

| Field | Required | Rule |
|---|---|---|
| `candidate` | yes | short lowercase identifier, as State defines it |
| `register.name` | yes | display name |
| `register.description` | yes | one paragraph of printable ASCII, at most 2000 characters |
| `register.website` | yes | an `https` URL |

A registration for a candidate that already exists updates its metadata. That includes the
descriptions curated under [`docs/decisions/0010`](../../../docs/decisions/0010-candidate-descriptions-edited.md),
so the reviewer has to judge any rewrite of one.

### Release

```yaml
candidate: jpx
versions:
  - version: 1.2.0
    platforms:
      LINUX_X64: https://github.com/jpx/jpx/releases/download/v1.2.0/jpx-1.2.0-linux-x64.zip
      MAC_ARM64:
        url: https://github.com/jpx/jpx/releases/download/v1.2.0/jpx-1.2.0-mac-arm64.zip
        sha256: 41ab...
        sha512: 7d0e...
      WINDOWS_X64: https://github.com/jpx/jpx/releases/download/v1.2.0/jpx-1.2.0-windows-x64.zip
default: 1.2.0
```

| Field | Required | Rule |
|---|---|---|
| `candidate` | yes | a registered candidate |
| `versions` | no | one or more versions |
| `versions[].version` | yes | a version string State accepts for this candidate |
| `versions[].platforms` | yes | a map from State platform id to an archive |
| platform value | yes | either a URL, or `url` with any of `md5`, `sha256`, `sha512` (hex) |
| `default` | no | a version in this request or already in State; becomes the `lts` tag |

A release needs `versions`, `default`, or both. With `default` alone, it moves the default to
a version that already exists.

Platform ids are State's, verbatim: `LINUX_X32`, `LINUX_X64`, `LINUX_ARM32HF`,
`LINUX_ARM32SF`, `LINUX_ARM64`, `MAC_X64`, `MAC_ARM64`, `WINDOWS_X64`, `UNIVERSAL`.

There is no `distribution`, `visible` or `tags` field. Distributions and hidden versions only
exist for java, which this repo never touches. The `lts` tag is the only tag a request
controls.

## Business Rules

1. **File names follow the convention, and their timestamps can't jump the queue.** A name
   that doesn't match the pattern fails the check. So does a timestamp in the future, or one
   earlier than its candidate's latest applied request, so a stale or back-dated request can
   never be ordered ahead of what State already holds.
2. **java is refused.** DISCO owns java. Any request naming it fails the check.
3. **Vendor-published candidates are not refused.** A candidate a vendor also publishes
   through the Vendor API is open to requests here. A request that would overwrite a vendor's
   own rows is for the reviewing maintainer to catch.
   The vendor remains its only writer.
4. **Nothing is deleted or hidden, for now.** No request kind deletes a version or a
   candidate; a maintainer does that directly. A request can overwrite a version (a new URL or
   checksum) by posting it again, but never remove one. State's community role already
   allows deletes, so adding a delete request later needs no State release.
5. **Unknown fields are rejected.** Parsing is strict, so a typo fails the check and is never
   silently ignored.
6. **`UNIVERSAL` excludes every other platform for one version.** A version is either one
   universal archive or a set of platform archives, never both.
7. **Every archive is downloaded, and every checksum is verified against it.** A request may
   declare any of `md5`, `sha256` and `sha512` for an archive. Each one declared must match
   the downloaded bytes, or the check fails, and so does the apply. CI also computes the
   sha256 of every archive, so every row written to State carries at least a sha256 even when
   the contributor declared none. What gets written is the computed sha256 plus any declared
   `md5` or `sha512`, all verified against the same download.
8. **Archives must be well formed.** The wiki's hard rules fail the check, and its
   conventions only warn:

   | Fails | Warns |
   |---|---|
   | the URL does not resolve to `200` over `https` | the base directory is not `<candidate>-<version>` |
   | the archive is not a zip or tar.gz | the file name does not follow the convention |
   | it does not hold exactly one base directory | |
   | there is no `bin/` directly under the base directory | |
   | `bin/` holds no executable file (any platform except `WINDOWS_X64`) | |

9. **Every State rule is checked by State.** Version syntax, the candidate registry,
   description rules and the role's permissions are evaluated by State's public validation,
   never re-implemented here. Rules live in one place and can't drift.
10. **The default is applied last.** Within a release, the `lts` tag moves only after every
   version in the request is written, so the default never points at a missing row. It is
   set on every platform row of the default version.
11. **Receipts are written only by the bot.** A contributor PR that touches `.applied/` fails
    the check. The bot may only add files under `.applied/`.
12. **The check needs no secrets.** Forks run it unchanged. Only the apply job, on `main`,
    holds a State credential.

## The check (on every PR)

The check validates only the request the PR adds. It:

1. confirms the PR adds exactly one file under `requests/` and changes nothing else;
2. checks the file name (rule 1), parses the file strictly, and confirms its directory and kind match its content;
3. asks State's public validation whether the request would be accepted, and, when `default`
   names a version outside the release, confirms through State's public read routes that it exists;
4. downloads every archive and applies rules 6 to 8;
5. posts one PR comment listing each row as `OK`, `WARN` or `FAIL`, with its checksums and whether each declared one matched.

```
FAIL  jpx 1.2.0 LINUX_X64    no bin/ under jpx-1.2.0/
WARN  jpx 1.2.0 MAC_ARM64    base directory is 'jpx' (expected 'jpx-1.2.0')
OK    jpx 1.2.0 WINDOWS_X64  sha256 9c1e...
```

Any `FAIL` fails the check, and a failed check blocks the merge. Warnings leave the call to
the reviewer.

## The apply (on every push to `main`)

For each pending request, in file-name order:

- **Registration:** registers or updates the candidate.
- **Release:** downloads each archive again and verifies every declared checksum against it
  (rule 7). The apply trusts nothing from the PR run: an archive that changed after review
  fails here, before anything for that request is written. It then posts each version with
  its checksums, then moves the `lts` tag.
- **Then:** commits the receipt.

On the first failure the run stops, fails, and leaves that request and everything after it
pending. The failure reaches the maintainer through the org's standard failure alert (see
*GitHub workflows*).

A receipt:

```yaml
applied_at: 2026-10-14T09:40:57Z
commit: 9e07d13
run: https://github.com/sdkman/sdkman-contrib/actions/runs/123
rows:
  - version: 1.2.0
    platform: LINUX_X64
    sha256: 9c1e...
  - version: 1.2.0
    platform: MAC_ARM64
    sha256: 41ab...
    sha512: 7d0e...
tag:
  lts: 1.2.0
```

`commit` is the merge commit that added the request. A registration's receipt carries the
metadata it sent instead of `rows` and `tag`.

## What State must provide

This repo cannot ship until `sdkman-state` has the following, specced in
[`community-role.md`](../../../do/sdkman-state/specs/community-role.md) and
[`public-validation.md`](../../../do/sdkman-state/specs/public-validation.md):

- **A `community` role** for every candidate except java. It can do to candidates, versions and tags whatever the admin can, deletes included,
  but cannot manage vendors. This repo uses only part of that: it never deletes, and only
  moves `lts`. Those limits are this repo's rules, enforced by the check and the apply, not
  by State.
- **Public validation** that never writes, needs no login, and is not throttled. It applies
  the write routes' own rules and refuses java, answering with the status a
  write would get, for a registration or for a release
  as a whole (its versions plus optional default), listing every failure at once. A release's
  default may name a version in the same release. A default naming any other version is not
  checked by State; the check confirms it exists through State's public read routes.

## Tooling

One Rust binary, `contrib`, does all of it:

| Command | Who runs it | What it does |
|---|---|---|
| `contrib new register <candidate>` | contributor | writes a registration skeleton at the right path, with the current UTC timestamp |
| `contrib new release <candidate> <version> --platforms <ids>` | contributor | writes a release skeleton with one URL placeholder per platform |
| `contrib check <file>` | CI, or a contributor locally | the check, as above |
| `contrib apply` | CI on `main` | the apply, as above |

```
$ contrib new release jpx 1.2.0 --platforms LINUX_X64,MAC_ARM64
wrote requests/jpx/20261014T094057Z-release-1.2.0.yaml
```

A contributor never has to install the binary: a correctly named, hand-written file is just
as good. Pre-built binaries are published with each release for those who want it. The receipts are committed by a
dedicated `sdkman-contrib` GitHub App, the only actor allowed to bypass `main`'s protection,
which it can only use to add files under `.applied/`.

## GitHub workflows

The repo uses the same workflows as every other SDKMAN repo. Each is a thin caller of a
reusable workflow in `sdkman/.github`, so alerting and triage behave here as they do
elsewhere.

| Workflow | Trigger | Job |
|---|---|---|
| `Pull Requests` | every PR | runs `contrib check`; no secrets |
| `Apply` | push to `main`, ignoring pushes that only touch `.applied/` | runs `contrib apply`, one run at a time, never cancelled; holds the State credential and the App token |
| `Release` | a version tag | builds and publishes the `contrib` binaries |
| `Workflow Failure Alert` | `workflow_run` of any workflow on `main` | `reusable-workflow-failure-alert.yml`: a failed apply (or release) notifies the maintainer through Chloé |
| `Triage: Issues, Comments, PRs` | issue, comment or PR opened | `reusable-triage-events.yml` |
| `Triage: Fork PR` | `workflow_run` of `Pull Requests` | `reusable-triage-fork-pr.yml`: surfaces fork PRs and their check result |
| `Dependabot Digest` | weekly schedule | `reusable-dependabot-digest.yml`, with `pr_ci_workflow: "Pull Requests"` |
| `Claude Code` | issue and PR comments | as in the other repos |

Ignoring `.applied/`-only pushes matters because receipts are pushed with the App's token,
and unlike the default Actions token, those pushes do trigger workflows. Without the filter,
every receipt would start another, empty apply run.

The PR workflow must be named `Pull Requests`, because the fork-PR triage and the Dependabot
digest find it by that name. The failure alert is scoped to `main`, so a contributor's failed
check alerts nobody; the contributor reads the check's own comment instead.

## Rollout

1. `sdkman-state` ships the `community` role and public validation.
2. This repo ships the check, the apply, the bot and a contributor guide (the README), with
   `main` protected.
3. A maintainer registers a candidate end to end through the repo before announcing it.
4. Each of the 17 closed `sdkman-db-migrations` PRs gets a comment linking the guide, with
   the YAML version of what it asked for. Their authors resubmit; nobody ports them for
   them.
5. The guide is posted in Discord, and the archived `sdkman-db-migrations` README points at
   it.

## Examples

```gherkin
Feature: Community contributions

  Scenario: A new candidate takes two PRs
    Given "jpx" is not registered
    When a contributor merges a registration for "jpx"
      And the apply run succeeds
    Then "jpx" is registered in State
      And a receipt exists for the registration
      And a release PR for "jpx" can now pass the check

  Scenario: A release for an unregistered candidate fails the check
    Given "jpx" is not registered
    When a contributor opens a release PR for "jpx"
    Then the check fails, citing the candidate registry

  Scenario: A PR with two requests fails the check
    When a contributor opens a PR adding two files under requests/
    Then the check fails

  Scenario: java is refused
    When a contributor opens a release PR for "java"
    Then the check fails

  Scenario: An archive without bin/ fails the check
    Given an archive whose base directory holds no bin/
    When the check runs
    Then that row is FAIL and the check fails

  Scenario Outline: A declared checksum that does not match fails the check
    Given a request declaring a <algorithm> that does not match the download
    When the check runs
    Then the check fails

    Examples:
      | algorithm |
      | md5       |
      | sha256    |
      | sha512    |

  Scenario: An archive replaced after review fails the apply
    Given a merged request whose declared sha256 matched at review
      And the archive at its URL has since been replaced
    When the apply runs
    Then nothing for that request is written
      And the run fails and the request stays pending

  Scenario: A request without checksums still gets a sha256
    Given a request declaring no checksums
    When it is applied
    Then every row written to State carries the computed sha256

  Scenario: A badly named request fails the check
    When a contributor opens a PR adding "requests/jpx/1.2.0.yaml"
    Then the check fails, citing the naming convention

  Scenario: A back-dated request fails the check
    Given "jpx"'s latest applied request is stamped 20261014T094057Z
    When a contributor opens a PR adding a "jpx" request stamped 20261001T000000Z
    Then the check fails

  Scenario: contrib new writes a valid skeleton
    When a contributor runs "contrib new register jpx"
    Then a file "requests/jpx/<now>-register-jpx.yaml" exists
      And once its placeholders are filled, the check passes

  Scenario: Pending requests apply in file-name order across candidates
    Given pending requests "tomcat/20261009T081500Z-release-11.0.15.yaml" and "jpx/20261008T140211Z-register-jpx.yaml"
    When the apply runs
    Then the "jpx" registration is applied first

  Scenario: A failed apply blocks later requests and is retried
    Given request A's file name sorts before request B's
      And A's apply fails
    Then B is not applied
    When a later push to main triggers another run
    Then A is retried before B

  Scenario: Re-applying a half-applied release is harmless
    Given a release whose first version was written before the run failed
    When the next run applies it
    Then every version is written once
      And the lts tag is moved last

  Scenario: The default moves without new versions
    Given "tomcat 11.0.15" exists in State
    When a release with only "default: 11.0.15" is applied
    Then "tomcat"'s default is 11.0.15

  Scenario: A contributor cannot write a receipt
    When a contributor opens a PR touching .applied/
    Then the check fails
```

## Out of Scope

- java.
- Deleting or hiding versions, and deleting candidates.
- Tags other than `lts`, and distributions.
- Installing archives in CI (a smoke `sdk install`).
- Delegating approval to candidate owners.
- Reconciling State to match the repo, or detecting drift.
- Bulk PRs, and batching several candidates into one request.

## Acceptance Criteria

- [ ] A PR adding one well-formed request passes the check, and the check posts a per-row comment with its checksums
- [ ] A file name not matching `<timestamp>-<kind>-<slug>.yaml`, a future timestamp, or one earlier than the candidate's latest applied request fails the check
- [ ] `contrib new register` and `contrib new release` write correctly named skeletons that pass the check once their placeholders are filled
- [ ] A PR adding more than one file, editing an existing request, or touching `.applied/` fails the check
- [ ] Unknown fields, a path that doesn't match the `candidate`, and a file mixing `register` with `versions` or `default` each fail the check
- [ ] java fails the check
- [ ] Each hard archive rule fails the check, and each convention only warns
- [ ] Any declared `md5`, `sha256` or `sha512` that does not match the download fails the check, and fails the apply before anything for that request is written
- [ ] Every row written to State carries a computed sha256, plus any declared `md5` or `sha512`
- [ ] A version mixing `UNIVERSAL` with other platforms fails the check
- [ ] The check runs on fork PRs with no secrets
- [ ] The apply runs one at a time, is never cancelled, applies pending requests in file-name order across all candidates, and stops at the first failure
- [ ] A failed request is retried by the next run, and requests ordered after it wait
- [ ] Within a release, every version is written before the `lts` tag moves
- [ ] Each applied request gets exactly one receipt under `.applied/`, at the request's mirrored path, committed by the GitHub App
- [ ] Nothing in this repo deletes or hides anything in State
- [ ] `main` accepts changes only through maintainer-approved PRs, except the App's receipts
- [ ] The repo carries the standard SDKMAN workflows, and a failed apply on `main` notifies the maintainer through the shared failure alert
- [ ] The README is a complete contributor guide: both request kinds, the two-PR flow, the archive rules, and how to read the check's comment
