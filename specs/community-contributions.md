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

**Requests are immutable.** A merged request file is never edited or deleted. A
correction is a new request. It overwrites only the rows it posts. A platform row it leaves
out keeps its earlier archive until a maintainer removes it.

**Every PR is approved by a SDKMAN maintainer.** Approval is never delegated to candidate
owners or vendors. `main` is protected: changes land only through approved PRs, except for
the apply bot's receipts.

**Applying happens after merge, in file-name order, one run at a time.** Each push to `main`
starts an apply run. Runs queue and are never cancelled. A run applies every *pending*
request in the order of their file names, which start with a UTC timestamp, across all
candidates, and stops at the first failure. A request is pending until its
receipt exists. A failed request therefore stays pending, is retried by the next run, and
blocks every request ordered after it until it succeeds or a maintainer voids it. That
keeps a release from landing before the registration it depends on.

**A maintainer can retry or void.** `Apply` also runs on `workflow_dispatch`, which
retries the queue. Its optional `void` input names one pending request. The bot commits
that request's receipt with `voided: <reason>` and no `rows` or `tag`, and writes nothing
to State. A voided request counts as applied.

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
contrib-candidates.txt                      # candidates open to requests (rule 3)
```

A request lives at `requests/<candidate>/<timestamp>-<kind>-<slug>.yaml`:

| Part | Rule |
|---|---|
| `<candidate>` | the directory; must match the file's `candidate` |
| `<timestamp>` | when the request was written, in UTC, ISO 8601 basic format: `YYYYMMDDTHHMMSSZ` |
| `<kind>` | `register` or `release`; must match the file's content |
| `<slug>` | lowercase letters, digits, `.` and `-`; a gist of the request for humans, never parsed. `contrib new` derives it from the candidate (registration) or the version (release) by lowercasing it and replacing every other character with `-` |

The timestamp leads, so names sort as text into the order they were written, with no
colons to trip Windows checkouts. Pending requests apply in that order, comparing file names
across every candidate directory, with the path relative to `requests/` as the tie-break. `contrib new` stamps
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
      LINUX_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip
      MAC_ARM64:
        url: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-mac-arm64.zip
        sha256: 41ab...
        sha512: 7d0e...
      WINDOWS_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-windows-x64.zip
default: 1.2.0
```

| Field | Required | Rule |
|---|---|---|
| `candidate` | yes | a registered candidate |
| `versions` | no | one or more versions |
| `versions[].version` | yes | a version string State accepts for this candidate |
| `versions[].platforms` | yes | a map from State platform id to an archive |
| platform value | yes | either a URL, or `url` with any of `md5`, `sha256`, `sha512` (hex) |
| `default` | no | a version in this request; its rows are posted tagged `stable` |

A release needs `versions`; `default` is optional. A `default` that is not one of the
request's versions fails the check. Moving the default to an existing version means
re-posting that version.

Platform ids are State's, verbatim: `LINUX_X32`, `LINUX_X64`, `LINUX_ARM32HF`,
`LINUX_ARM32SF`, `LINUX_ARM64`, `MAC_X64`, `MAC_ARM64`, `WINDOWS_X64`, `UNIVERSAL`.

There is no `distribution`, `visible` or `tags` field; `default` is how a request sets a tag. Distributions and hidden versions only
exist for java, which this repo never touches. The `stable` tag is the only tag a request
controls.

Re-posting a version replaces each of its rows whole: its checksums are exactly those this
request writes.

## Business Rules

1. **File names follow the convention, and their timestamps can't jump the queue.** A name
   that doesn't match the pattern fails the check. So does a timestamp in the future, or one
   earlier than its candidate's latest applied request, so a stale or back-dated request can
   never be ordered ahead of what State already holds. The guard holds at merge: `main`'s
   protection requires a PR branch to be up to date with `main`, so the check re-runs against
   the latest receipts before every merge.
2. **java is refused.** DISCO owns java, so it is never listed in `contrib-candidates.txt`
   (rule 3), and any request naming it fails the check.
3. **Only listed candidates are open.** The committed `contrib-candidates.txt` lists every
   candidate whose vendor publishes through neither State's vendor role nor the Vendor API
   (`vendor-release`). Any request naming a candidate not on it fails the check, so a vendor
   remains the only writer of its candidates. A maintainer adds a new candidate to the list
   before its registration PR.
4. **Nothing is deleted or hidden, for now.** No request kind deletes a version or a
   candidate; a maintainer does that directly. A request can overwrite a version (a new URL or
   checksum) by posting it again, but never remove one. State's community role already
   allows deletes, so adding a delete request later needs no State release.
5. **Unknown fields are rejected.** Parsing is strict, so a typo fails the check and is
   never silently ignored. Every value is its scalar's exact text, so `version: 8.10` is
   `8.10`. A block-scalar description loses its trailing newline and is otherwise posted as
   written.
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
   | the URL does not name a file ending in `.zip` or `.tar.gz`, or does not answer `200` over `https` without a redirect | the base directory is not `<candidate>-<version>` |
   | the archive is not a zip or tar.gz | the file name does not follow the convention |
   | it does not hold exactly one base directory | |
   | there is no `bin/` directly under the base directory | |
   | `bin/` holds no executable file (any platform except `WINDOWS_X64`) | |

9. **Every State rule is checked by State.** Version syntax, the candidate registry,
   and description rules are evaluated by State's validation twins, never re-implemented
   here. Rules live in one place and can't drift.
10. **The default is tagged by State.** The default must be a version in the request. Its
    rows are posted with `tags: ["stable"]`; every other row is posted with no `tags` field.
    Moving the default to an existing version means re-posting that version. State enforces
    every tagging rule; this repo holds none of that logic and fails fast on whatever State
    refuses. A default with neither a `UNIVERSAL` nor a `LINUX_X64` row never becomes the
    default, and is for the reviewing maintainer to catch. `stable` is the tag State's default cascade reads first
    ([`default-tag-cascade.md`](../../../do/sdkman-state/specs/default-tag-cascade.md)).
11. **Receipts are written only by the bot.** A contributor PR that touches `.applied/` fails
    the check. The bot may only add files under `.applied/`.
12. **The check needs no secrets.** Forks run it unchanged. Only the apply job, on `main`,
    holds a State credential.

## The check (on every PR)

A PR that touches `requests/` or `.applied/` is a request PR, and gets the check below. Any
other PR (the `contrib` code, workflows, the README, Dependabot bumps) runs the Rust build and
tests instead, and fails if it also touches `requests/` or `.applied/`. Both run under the one
`Pull Requests` workflow.

The check validates only the request the PR adds. It:

1. confirms the PR adds exactly one file under `requests/` and changes nothing else;
2. checks the file name (rule 1), parses the file strictly, and confirms its directory and kind match its content;
3. downloads every archive, applies rules 6 to 8, and computes each archive's sha256;
4. posts each body the apply will post, computed sha256 included, to its State validation
   twin (one per row for a release) and collects every answer. Only `204` passes: any other
   answer, a `400`, a `404` from a missing route, or a server error, fails the check;
5. writes a report listing each row as `OK`, `WARN` or `FAIL`, with its checksums and whether each declared one matched, to the job summary and as a run artifact. A fork PR's token is read-only, so the check cannot comment itself; the `Check Report` workflow posts the report as one PR comment.

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
- **Release:** downloads each archive again, verifies every declared checksum against it
  (rule 7), and re-runs the archive rules (rules 6 and 8). The apply trusts nothing from the
  PR run: an archive that fails any of these fails here, before anything for that request is
  written. A declared checksum is the only thing that pins an archive to what was reviewed.
  An archive with none, swapped after review for one that is still well formed, is accepted
  and written with the sha256 of what was downloaded. It then posts each row with its
  checksums, the default's rows tagged `stable` (rule 10).
- **Then:** commits the receipt, runs `git pull --rebase` and pushes, before the next
  request.

The run logs in as the community account once at the start, and logs in again whenever its
token expires (10 minutes today), so a release slow enough to outlast one token still
applies in a single run. State throttles only login, so this never trips the limit.

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
  stable: 1.2.0
```

`commit` is the merge commit that added the request. A registration's receipt carries the
metadata it sent instead of `rows` and `tag`.

## What State must provide

This repo cannot ship until `sdkman-state` has the following, specced in
[`community-role.md`](../../../do/sdkman-state/specs/community-role.md),
[`public-validation.md`](../../../do/sdkman-state/specs/public-validation.md) and
[`non-java-version-rules.md`](../../../do/sdkman-state/specs/non-java-version-rules.md):

- **A `community` role** for every candidate, able to write candidates, versions and
  tags. This repo uses only part of that: it never deletes, only sets `stable`, and never touches java. Those limits are this repo's
  rules, enforced by the check and the apply, not by State.
- **Public validation**: open twins of the write routes, `POST /validate/candidates` and
  `POST /validate/versions`. Each takes exactly its write route's body, runs the same
  validation, and answers as the write would (`204` on success), without writing or a login.
  The check posts exactly the bodies the apply will post: one
  call for a registration, one per row for a release, the default's rows with their
  `stable` tag.
- **Non-java version rules** on `POST /versions` and its validation twin: a version
  grammar, one platform shape per version, and a `stable` tag that always agrees with the
  candidate's default. Until State ships them, rule 9's version syntax check is a no-op for
  every candidate this repo accepts.

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
dedicated `sdkman-contrib` GitHub App, the only actor allowed to skip `main`'s PR
requirement. GitHub cannot limit that to one folder; the limit is `contrib apply`, the only
code that pushes with the App, which only adds files under `.applied/`.

## GitHub workflows

The repo uses the same workflows as every other SDKMAN repo. Each is a thin caller of a
reusable workflow in `sdkman/.github`, so alerting and triage behave here as they do
elsewhere.

| Workflow | Trigger | Job |
|---|---|---|
| `Pull Requests` | every PR | runs `contrib check` on a request PR, the Rust build and tests on any other; no secrets |
| `Apply` | push to `main`, ignoring pushes that only touch `.applied/`; `workflow_dispatch`, with an optional `void` input | runs `contrib apply`, one run at a time, never cancelled; holds the State credential and the App token |
| `Release` | a version tag | builds and publishes the `contrib` binaries |
| `Workflow Failure Alert` | `workflow_run` of any workflow on `main` | `reusable-workflow-failure-alert.yml`: a failed apply notifies the maintainer through the shared failure alert; `Release` runs on tags, outside the alert's `main` scope, and shows its failure on its own run only |
| `Triage: Issues, Comments, PRs` | issue, comment or PR opened | `reusable-triage-events.yml` |
| `Check Report` | `workflow_run` of `Pull Requests` | posts the check's report artifact as the PR comment; holds write permission, never runs the PR's code |
| `Triage: Fork PR` | `workflow_run` of `Pull Requests` | `reusable-triage-fork-pr.yml`: surfaces fork PRs and their check result |
| `Dependabot Digest` | weekly schedule | `reusable-dependabot-digest.yml`, with `pr_ci_workflow: "Pull Requests"` |
| `Claude Code` | issue and PR comments | as in the other repos |

Ignoring `.applied/`-only pushes matters because receipts are pushed with the App's token,
and unlike the default Actions token, those pushes do trigger workflows. Without the filter,
every receipt would start another, empty apply run.

The PR workflow must be named `Pull Requests`, because the fork-PR triage and the Dependabot
digest find it by that name. The failure alert is scoped to `main`, so a contributor's failed
check alerts nobody; the contributor reads the check's report comment instead.

## Rollout

1. `sdkman-state` ships the `community` role, public validation and the non-java version rules.
2. This repo ships the check, the apply, the bot and a contributor guide (the README), with
   `main` protected.
3. Confirm in Cloudflare that no edge rate-limit rule on `state.sdkman.io` throttles the
   validation routes or community writes; State itself throttles only login.
4. A maintainer registers a candidate end to end through the repo before announcing it.
5. Each of the 17 closed `sdkman-db-migrations` PRs gets a comment linking the guide, with
   the YAML version of what it asked for. Their authors resubmit; nobody ports them for
   them.
6. The guide is posted in Discord, and the archived `sdkman-db-migrations` README points at
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

  Scenario: An undeclared archive replaced after review is re-checked at apply
    Given a merged request declaring no checksums
      And the archive at its URL has since been replaced by one with no bin/
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
      And the default's rows carry the stable tag

  Scenario: The default moves to an existing version
    Given "tomcat 11.0.15" exists in State
    When a release re-posting "11.0.15" with "default: 11.0.15" is applied
    Then "tomcat"'s default is 11.0.15

  Scenario: A default outside the request fails the check
    When a contributor opens a release whose default is not one of its versions
    Then the check fails

  Scenario: A contributor cannot write a receipt
    When a contributor opens a PR touching .applied/
    Then the check fails
```

## Out of Scope

- java.
- Candidates a vendor publishes through State's vendor role or the Vendor API.
- Deleting or hiding versions, and deleting candidates.
- Tags other than `stable`, and distributions.
- Installing archives in CI (a smoke `sdk install`).
- Delegating approval to candidate owners.
- Reconciling State to match the repo, or detecting drift.
- Bulk PRs, and batching several candidates into one request.

## Acceptance Criteria

- [ ] A PR adding one well-formed request passes the check, and the check's per-row report, with its checksums, is posted as a PR comment, fork PRs included
- [ ] A file name not matching `<timestamp>-<kind>-<slug>.yaml`, a future timestamp, or one earlier than the candidate's latest applied request fails the check
- [ ] A request whose check passed before a newer request for its candidate was applied cannot merge until its check passes again
- [ ] `contrib new register` and `contrib new release` write correctly named skeletons that pass the check once their placeholders are filled
- [ ] A PR adding more than one file, editing an existing request, or touching `.applied/` fails the check
- [ ] Unknown fields, a path that doesn't match the `candidate`, and a file mixing `register` with `versions` or `default` each fail the check
- [ ] java fails the check
- [ ] A candidate not listed in `contrib-candidates.txt` fails the check
- [ ] Each hard archive rule fails the check, and each convention only warns
- [ ] Any declared `md5`, `sha256` or `sha512` that does not match the download fails the check, and fails the apply before anything for that request is written
- [ ] Every row written to State carries a computed sha256, plus any declared `md5` or `sha512`
- [ ] A version mixing `UNIVERSAL` with other platforms fails the check
- [ ] The check runs on fork PRs with no secrets
- [ ] The apply runs one at a time, is never cancelled, applies pending requests in file-name order across all candidates, and stops at the first failure
- [ ] A failed request is retried by the next run, and requests ordered after it wait
- [ ] A maintainer can void a pending request, and the queue resumes
- [ ] The default's rows are posted with `tags: ["stable"]`, every other row with no `tags` field, and a default outside the request fails the check
- [ ] Each applied request gets exactly one receipt under `.applied/`, at the request's mirrored path, committed by the GitHub App
- [ ] Nothing in this repo deletes or hides anything in State
- [ ] `main` accepts changes only through maintainer-approved PRs, except the receipts `contrib apply` pushes with the App
- [ ] The repo carries the standard SDKMAN workflows, and a failed apply on `main` notifies the maintainer through the shared failure alert
- [ ] The README is a complete contributor guide: both request kinds, the two-PR flow, how a candidate gets onto `contrib-candidates.txt`, the archive rules including the URL rule, how to move the default by re-posting a version, and how to read the check's comment, with a worked YAML example of a registration, a release, and a release that moves the default
