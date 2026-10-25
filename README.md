# sdkman-contrib

Add a candidate or a new version to SDKMAN! by writing one small YAML file and opening a pull request.

CI checks your file, downloads every archive it points at, and posts the result on your PR. A SDKMAN maintainer reviews and merges it. A job on `main` then writes the request to the State API and records a receipt. You need no Docker, no JVM and no build. Editing YAML is the whole job.

This repository replaces `sdkman-db-migrations`, which was archived when Postgres became the master datastore.

## Installing contrib

Install `contrib` through SDKMAN. Run `sdk update`, open a new terminal, then run `sdk install contrib`. Later, `sdk upgrade contrib` moves you to a newer release.

It runs on Linux x64, Linux ARM64, macOS x64, macOS ARM64 and Windows x64.

Confirm the install worked:

```
$ contrib --version
contrib <version>
```

It prints the installed version. If you cannot use SDKMAN, download your platform's archive from the GitHub release. If you already have a Rust toolchain, `cargo install --git https://github.com/sdkman/sdkman-contrib` builds it from source.

With `contrib` installed you can write a request with `contrib new` and run `contrib check` before you push.

## How it works

There are two kinds of request.

- A **registration** registers a candidate, or updates the metadata of one that exists.
- A **release** adds or overwrites versions of a registered candidate. It can also move the candidate's default version.

A file is one kind, never both. A PR adds exactly one file under `requests/` and changes nothing else.

### A new candidate takes two PRs

1. Ask a maintainer to list your candidate in `contrib-candidates.txt` (see below).
2. Open a PR with a **registration**. Wait for it to merge and for the apply run to succeed.
3. Open a PR with a **release**.

A release PR for a candidate that is not registered yet fails its check, because the candidate is not in the registry. Open it only after the registration is applied.

### Listing a candidate in `contrib-candidates.txt`

Only candidates listed in `contrib-candidates.txt` accept requests. A request naming any other candidate fails the check. The list holds candidates whose vendor publishes through neither State's vendor role nor the Vendor API, so a vendor stays the only writer of its own candidates.

A maintainer adds the candidate before its registration PR, even when the candidate already exists. The file takes one candidate per line, exactly as State names it. Blank lines and lines starting with `#` are ignored.

`java` is never listed. DISCO owns java, and any request naming it fails the check.

### Requests are one-shot

A merged request asks State to make a change once. State stays the master copy. The repo never reconciles State to match it, and it never deletes anything. A merged request file is never edited or deleted. To correct a mistake, add a new request.

A re-posted version replaces each of its rows whole. A platform row you leave out keeps its earlier archive until a maintainer removes it.

## File names

A request lives at `requests/<candidate>/<timestamp>-<kind>-<slug>.yaml`.

| Part | Rule |
|---|---|
| `<candidate>` | The directory. Must match the `candidate` in the file. |
| `<timestamp>` | When you wrote the request, in UTC: `YYYYMMDDTHHMMSSZ`, for example `20261014T094057Z`. |
| `<kind>` | `register` or `release`. Must match the content of the file. |
| `<slug>` | Lowercase letters, digits, `.` and `-`. A gist for humans, never parsed. |

Examples:

```
requests/jpx/20261008T140211Z-register-jpx.yaml
requests/jpx/20261014T094057Z-release-1.2.0.yaml
```

The timestamp leads, so names sort in the order they were written. Merged requests apply in that order, across all candidates. The check fails a timestamp in the future, and one earlier than the latest applied request of the same candidate.

## Writing a request with `contrib new`

`contrib new` writes a correctly named skeleton with the current UTC time. A hand-written file that follows the naming rules stays valid.

```
$ contrib new register jpx
wrote requests/jpx/20261014T094057Z-register-jpx.yaml

$ contrib new release jpx 1.2.0 --platforms LINUX_X64,MAC_ARM64
wrote requests/jpx/20261014T094512Z-release-1.2.0.yaml
```

The skeleton holds `REPLACE_WITH_` placeholders. Fill them all in. A release skeleton also holds a commented `# default:` line; uncomment it to make the version the default.

To run the check on your machine before you push, name the file:

```
$ contrib check requests/jpx/20261014T094512Z-release-1.2.0.yaml
```

A local run asks State's public validation routes, so it needs network access but no credential. `CONTRIB_STATE_URL` points it at another State.

## Request format

Parsing is strict. An unknown field, a repeated key, a repeated version, an empty `versions` list and a version without platforms all fail the check. Every value is the exact text of its scalar, so `version: 8.10` is `8.10`.

### Registration

```yaml
candidate: jpx
register:
  name: JPX
  description: JPX is a command-line tool for querying JSON documents.
  website: https://jpx.dev/
```

| Field | Required | Rule |
|---|---|---|
| `candidate` | yes | Short lowercase identifier, as State defines it. |
| `register.name` | yes | Display name. |
| `register.description` | yes | One paragraph of printable ASCII, at most 2000 characters. |
| `register.website` | yes | An `https` URL. |

A registration for a candidate that already exists updates its metadata. Several descriptions were edited by hand during the move to Postgres, so the reviewer judges any rewrite of one.

A block-scalar description loses its trailing newline and is otherwise posted as written.

### Release

```yaml
candidate: jpx
versions:
  - version: 1.2.0
    platforms:
      LINUX_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip
      MAC_ARM64:
        url: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-mac-arm64.zip
        sha256: 41ab5c0f7d3e9a6b8c2d1e0f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b
      WINDOWS_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-windows-x64.zip
```

| Field | Required | Rule |
|---|---|---|
| `candidate` | yes | A registered candidate. |
| `versions` | yes | One or more versions. |
| `versions[].version` | yes | A version string State accepts for this candidate. |
| `versions[].platforms` | yes | A map from State platform id to an archive. |
| platform value | yes | Either a URL, or a map with `url` and any of `md5`, `sha256`, `sha512` (hex). |
| `default` | no | A version in this request. Its rows are posted tagged `stable`. |

Platform ids are State's, verbatim: `LINUX_X32`, `LINUX_X64`, `LINUX_ARM32HF`, `LINUX_ARM32SF`, `LINUX_ARM64`, `MAC_X64`, `MAC_ARM64`, `WINDOWS_X64`, `UNIVERSAL`.

`UNIVERSAL` excludes every other platform for one version. A version is either one universal archive or a set of platform archives, never both.

There is no `distribution`, `visible` or `tags` field. `default` is the only way a request sets a tag, and `stable` is the only tag it controls.

### Moving the default

To make an existing version the default, re-post that version with `default` set. This request moves the default of `jpx` to `1.2.0`:

```yaml
candidate: jpx
default: 1.2.0
versions:
  - version: 1.2.0
    platforms:
      LINUX_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip
      MAC_ARM64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-mac-arm64.zip
      WINDOWS_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-windows-x64.zip
```

A `default` that is not one of the versions in the same request fails the check. Repeat every platform you want to keep, since a re-posted version replaces its rows whole.

A default needs a `UNIVERSAL` or a `LINUX_X64` row, or it never becomes the default. The check warns about it.

## Archives

The check downloads every archive. Each one must be well formed. A hard rule fails the check, and a convention only warns.

| Fails | Warns |
|---|---|
| The URL does not use `https`. | The base directory is not `<candidate>-<version>`. |
| The URL does not name a file ending in `.zip` or `.tar.gz`. | The file name is not `<candidate>-<version>` or `<candidate>-<version>-...`. |
| The URL does not end in a `200` after at most 5 redirects, each of them over `https`. | |
| The URL path contains `/releases/latest/`. | |
| The file is not a zip or a tar.gz. | |
| The archive does not hold exactly one base directory. | |
| There is no `bin/` directly under the base directory. | |
| `bin/` holds no executable file (any platform except `WINDOWS_X64`). | |

So a URL such as `https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip` is fine. A GitHub release download passes too, since the check follows its `https` redirects to the archive. State receives the URL as you wrote it, not the redirect target. Name a fixed release, never `/releases/latest/`.

### Checksums

You may declare any of `md5`, `sha256` and `sha512` for an archive. Each one you declare must match the downloaded bytes, or the check fails. The apply downloads again and checks again.

CI also computes the sha256 of every archive, so every row written to State carries at least a sha256, even when you declared none. State receives the computed sha256 plus any `md5` or `sha512` you declared.

The apply blocks an archive that differs, byte for byte, from the one the check downloaded, whether or not you declared a checksum.

## The check on your PR

The check runs on every request PR, and fork PRs included. It needs no secrets. It runs `contrib` from `main`, never code from your PR, and reads your PR only as data. It:

1. confirms the PR adds exactly one file under `requests/` and nothing else;
2. checks the file name, parses the file strictly, and confirms the directory and kind match the content;
3. downloads every archive and applies the archive rules and the checksum rules;
4. asks State to validate exactly what the apply will post.

The result appears as a PR comment, updated in place on every push. Each row is one line:

```
FAIL  jpx 1.2.0 LINUX_X64    no bin/ under jpx-1.2.0/
WARN  jpx 1.2.0 MAC_ARM64    base directory is 'jpx' (expected 'jpx-1.2.0')
OK    jpx 1.2.0 WINDOWS_X64  sha256 9c1e...
```

- `OK` means the row passed.
- `WARN` means a convention is broken. The reviewer decides.
- `FAIL` means a rule is broken. Any `FAIL` fails the check, and a failed check blocks the merge.

An `OK` line shows the computed sha256 and, for each checksum you declared, whether it matched. Fix the file and push again; the comment updates.

A PR that touches `.applied/` fails the check. Only the apply bot writes receipts.

## After the merge

A maintainer approves every PR. Approval is never delegated to candidate owners or vendors.

A job on `main` then applies the pending requests, in file-name order, one run at a time. It re-reads each request under the same rules, downloads each archive again, and writes to the State API. Except for a blocked request, it stops at the first failure. A failed request stays pending, the next run retries it, and the requests ordered after it wait. That keeps a release from landing before its registration.

An archive that changed since the check blocks its request. The request writes nothing to State and stays pending, and so do the later requests of the same candidate. The requests of other candidates still apply. The run fails, and the block holds until a maintainer voids the request.

Applying is safe to repeat. A half-applied release finishes on the next run, and nothing is rolled back.

### Receipts

After a request applies, the bot commits a receipt under `.applied/` at the same relative path. The receipt is the only thing that marks a request as applied. It records what State received, including the checksums that CI computed:

```yaml
applied_at: 2026-10-14T09:40:57Z
commit: 9e07d13
run: https://github.com/sdkman/sdkman-contrib/actions/runs/123
rows:
  - version: 1.2.0
    platform: LINUX_X64
    sha256: 9c1e...
tag:
  stable: 1.2.0
```

`commit` is the merge commit that added the request. A registration's receipt carries the metadata it sent instead of `rows` and `tag`.

### Voiding

A maintainer can run the `Apply` workflow by hand to retry the queue. Its optional `void` input names one pending request, and its optional `reason` input says why; an empty reason records `voided by <actor>`. The bot commits a receipt with `voided: <reason>` and writes nothing to State. A voided request counts as applied, and the queue resumes.

## What a request cannot do

- Touch `java`.
- Delete or hide a version or a candidate. A maintainer does that directly.
- Set a tag other than `stable`, or set a distribution.
- Touch a candidate that is not listed in `contrib-candidates.txt`.

## Developing `contrib`

`contrib` is a Rust crate. The design is in [`specs/001-community-contributions.md`](specs/001-community-contributions.md) and [`specs/002-review-follow-ups.md`](specs/002-review-follow-ups.md). The verification gate is in [`CLAUDE.md`](CLAUDE.md).

### Releasing `contrib`

A PR bumps `version` in `Cargo.toml` and `Cargo.lock`. Once it is merged, a maintainer runs `cargo release tag --execute` and then `cargo release push --execute` on `main`. The tag is `v<version>`, and the push sends only the tag; [`release.toml`](release.toml) holds the settings.

The `Release` workflow takes it from there. The environment `release` deploys only from `v*` tags and holds `VENDOR_EMAIL` and `VENDOR_PASSWORD`.
