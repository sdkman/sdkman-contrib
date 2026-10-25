# Review Follow-ups

A review of `sdkman-contrib` before launch found gaps that would stop the process working for
real contributors, or would let a request pass the check without the check being trustworthy.
The biggest is that most community candidates publish on GitHub releases, and every GitHub
release download URL answers a redirect, which the check fails today. The README also never
tells a new contributor how to install `contrib`, so the local check, the fastest feedback a
contributor can get, is out of reach for most of them.

This spec states what must change. It amends
[`001-community-contributions.md`](001-community-contributions.md); where the two disagree, this one
wins.

*Reference: review of `main` at `9dd792b` on 2026-10-25.*

## Behaviour

**`contrib` is a SDKMAN candidate.** A contributor installs it with `sdk install contrib` and
keeps it current with `sdk upgrade contrib`, on Linux x64, Linux ARM64, macOS x64, macOS ARM64
and Windows x64. Every contributor is a SDKMAN user, so the tool reaches them through the
channel they already have, and an upgrade brings them the check's latest rules. They need no
Rust toolchain, no clone of the repo and no prior knowledge of GitHub releases.

**Every release of `contrib` reaches SDKMAN on its own.** A maintainer cuts a release with
`cargo release` on `main`. It sets the `version` in `Cargo.toml`, commits, tags the commit
`v<version>` and pushes, and the tag push publishes it as a new `contrib` version in State, for
every platform above, and makes it the default. *(Assumption, planning pass 2026-10-25:
`main`'s ruleset refuses a direct push, a maintainer's included, so the version bump lands
through a PR. On the merged commit a maintainer runs `cargo release tag` and
`cargo release push`, which tag it `v<version>` and push only the tag.)* Only maintainers may
create `v*` tags, a tag ruleset enforces that, and the `Release` workflow fails before
publishing unless the tagged commit is on `main`. The vendor credential lives in a GitHub
environment that only `v*` tags can deploy from. It is published as a vendor publishes its own candidate, through State's
vendor role, never through a `contrib` request. So `contrib` is never listed in
`contrib-candidates.txt`, and a broken `contrib` can never block its own fix. Each published
archive is well formed by SDKMAN's own rules: one `contrib-<version>` base directory holding
`bin/contrib`. Linux and macOS archives are `.tar.gz` and Windows is `.zip`, so `sdkman-hooks`
must serve `contrib` its tarball post-install hook on Linux and macOS before the first release.
That hooks change is delegated to Marco and is out of scope here (see *Out of Scope*).

**`contrib` is registered once, by hand, before its first release.** Creating the candidate is
delegated to Marco and is out of scope here (see *Out of Scope*); this repo only depends on it.
A State admin registers it with `POST /admin/candidates`:

```json
{
  "candidate": "contrib",
  "name": "SDKMAN Contrib",
  "description": "SDKMAN Contrib is the command-line tool for contributing candidates and versions to SDKMAN, for SDKs that rely on community contributions rather than being published by their own vendors. It writes and checks the YAML requests that a pull request adds, so a contributor sees locally the same verdict that the pull request check gives.",
  "website_url": "https://github.com/sdkman/sdkman-contrib"
}
```

The admin also creates a vendor account scoped to `contrib`. Only the `Release` workflow holds
that account's credential. This amends 001 rule 12.

**The README tells a contributor how to install it.** Installing through SDKMAN is the
recommended path: run `sdk update`, open a new terminal, then run `sdk install contrib`. It
gives the contributor `contrib new` and `contrib check` before they push. The README also names
the GitHub release binaries as a fallback for anyone who cannot use SDKMAN. A hand-written file
stays valid, but the README no longer presents installing as an optional extra.

*Side note for the README: a contributor who already has a Rust toolchain can build from
source with `cargo install --git https://github.com/sdkman/sdkman-contrib`. The README
mentions this in passing, never as a recommended path. `contrib` is not published to
crates.io: it would ask contributors for a Rust toolchain and a compile, and add a release
channel to maintain.*

**A published release exists before the process is announced.** At least one tagged release
is installable with `sdk install contrib` on each platform above, and carries a binary for
each on GitHub too. Contributors can reach the releases and the README without being members
of the `sdkman` organisation.

**Archive URLs that redirect are accepted.** A URL that answers a redirect, as every GitHub
release download does, passes the URL rule when the redirect leads to the archive. Every hop
uses `https`. The check downloads, inspects and hashes the archive at the end of the chain,
and the row written to State keeps the URL exactly as the contributor wrote it, not where it
redirected to.

**The check's verdict cannot be changed by the PR it judges.** The result of the check on a
request PR comes from the `contrib` code already on `main`, never from code in the PR. A
request PR that also changes the tool, a workflow or anything else outside its one request
still fails, and changing the tool in the same PR cannot turn that failure, or any other,
into a pass.

Request PRs are judged by a `pull_request_target` job. It builds `contrib` from the base
commit, reads the PR head only as data, and posts its verdict as the commit status
`contrib/request-check`, with success for a PR that touches neither `requests/` nor
`.applied/`. The ruleset requires that status, and a fork PR's read-only token cannot post
it. Code PRs keep a separate `pull_request` job for build and tests. Check Report and the
failure alert accept the new event. This amends 001's "Both run under the one
`Pull Requests` workflow". *(Assumption, planning pass 2026-10-25: the `pull_request_target`
job runs in its own `Request Check` workflow. One workflow serving both events would report
each run's other job as skipped, and GitHub counts a skipped job as passing a required
check. Fork-PR triage, which follows `Pull Requests`, also stays at one notification per
PR.)*

**What is applied is what was reviewed.** The archive the apply downloads must be the same
archive the check downloaded for the reviewed PR, byte for byte, whether or not the contributor
declared a checksum. If an archive changed between the check and the apply, that request fails,
writes nothing to State, and stays pending for a maintainer to void. The apply records the
failure, skips every later pending request for the same candidate, and goes on to the
requests of other candidates. A later request for that candidate never lands before the
blocked one, so a default can never move out of order. The blocked request and the
requests skipped behind it stay pending until a maintainer voids it. Each run that meets a
blocked request fails, so the failure alert fires on every run until it is voided.
*(Assumption, planning pass 2026-10-25: an archive the apply cannot download has not changed.
That failure keeps 001's rule: the run stops there, and the next run retries it.)*
*(Assumption, planning pass 2026-10-25: the apply compares digests before it re-runs the
archive rules and the declared checksums. An archive replaced by one that also breaks a rule
has changed, so it blocks its candidate rather than stopping the run.)*

The check writes each row's computed sha256 to a `digests.json` artifact of its run. Only
the trusted `pull_request_target` check run produces digests the apply accepts; an artifact
from a `pull_request` run, which runs with the PR's code, is never read. The artifact is
kept for as long as the repository's maximum artifact retention, so a request blocked in
the queue does not lose its digests while it waits; a request whose artifact has expired
has to be voided and requested again. The
apply reads that artifact, through the Actions API with `actions: read`, from the last
successful check run on the PR head commit that merged. A missing artifact counts as a
mismatch. This amends 001's "The apply trusts nothing from the PR run" for these digests
only. *(Assumption, planning pass 2026-10-25: the repository is public from launch, and
GitHub caps a public repository's artifact retention at 90 days, so the check keeps its
artifact for 90 days.)* *(Assumption, planning pass 2026-10-25: a lookup that fails in
transport, or that the Actions API answers with an unexpected status, has not found the
artifact missing. That failure keeps 001's rule: the run stops there, and the next run
retries it.)*

**A default that will not move is flagged.** A release whose `default` version has neither a
`UNIVERSAL` nor a `LINUX_X64` row gets a `WARN`. The WARN says the default will not move, and
that State will clear `stable` from every other version's `UNIVERSAL` row, which leaves a
candidate whose default is a `UNIVERSAL` version with no default at all.

**The committed ruleset is the live ruleset.** `.github/rulesets/main.json` describes `main`'s
protection as it is actually applied, including the GitHub App that may bypass it. It holds no
placeholder that has to be filled in elsewhere. *(Assumption, planning pass 2026-10-25: the App
does not exist yet, so its id cannot be committed. Until a maintainer commits it, `main.json`
names the App with `actor_id` 0, and `scripts/protect-main.sh` refuses to apply or check while
any ruleset does.)* *(Assumption, planning pass 2026-10-25: the `v*` tag ruleset is committed
beside it as `release-tags.json`, and the same script applies and checks both. Its bypass
actors are the repository's Maintain and Admin roles.)* *(Assumption, planning pass 2026-10-25:
`--check` also compares each ruleset's `enforcement`, because a ruleset switched off in the
settings protects nothing.)* *(Assumption, planning pass 2026-10-25: GitHub answers a rule's
parameters with defaults for those a ruleset left out, such as a pull request rule's allowed
merge methods. So `--check` compares only the parameters the committed file names, and a ruleset
it has just applied checks clean.)*

## Business Rules

1. **Every contributor platform is covered.** Linux x64, Linux ARM64, macOS x64, macOS ARM64
   and Windows x64 each get a `contrib` version in State and a binary on GitHub with every
   release. A platform without one is not listed as supported. *(Assumption, planning pass
   2026-10-25: the Linux binaries link musl statically, so they run on any glibc.)*
   *(Assumption, planning pass 2026-10-25: the Windows binary links its C runtime statically,
   so it runs without the Visual C++ redistributable.)*
2. **Install steps are verifiable.** The README shows how to confirm the install worked, by
   running `contrib --version`, which prints `contrib <version>`. `<version>` is the `version`
   in `Cargo.toml`, and the release reads it from there: the tag is `v<version>`, and the
   State version and the `contrib-<version>` base directory are `<version>`.
3. **`contrib` publishes itself as a vendor.** Its releases reach State through the vendor
   role, and `contrib` is never listed in `contrib-candidates.txt`.
4. **A release reaches SDKMAN only when it is complete.** A release posts all five rows
   without tags, then tags them `stable`, with `LINUX_X64` last. If any step fails before that
   last tag, the workflow deletes the rows it posted for that version (the vendor role may
   delete its own candidate's versions) and fails. The default moves only with the last tag.
   *(Assumption, planning pass 2026-10-25: State refuses to delete a row that holds a tag. So
   the rollback first moves each `stable` it moved back to the version that held it, or
   removes it where none did. It deletes only rows that did not exist before the run, so
   re-running a published release never deletes it.)* *(Assumption, planning pass 2026-10-25:
   State's reads are cached at the edge for up to ten minutes, so the publish reads past that
   cache. A stale answer would restore the wrong version, or delete a row that existed before
   the run.)*
5. **Redirect chains stay on `https`.** A chain with any non-`https` hop fails the URL rule.
   A chain that never ends in a `200` fails it too, as does one with more than 5 redirects. A
   written URL whose path contains `/releases/latest/` fails the URL rule. The written URL
   must name a fixed release, such as `/releases/download/<tag>/<file>`.
6. **The URL as written names the archive; the final file is what the archive rules judge.**
   The URL in the request must end in `.zip` or `.tar.gz`, and that extension sets the expected
   format. The archive format, the base directory and `bin/` rules apply to the file at the
   end of the chain.
7. **State receives the URL as written.** A redirect is never resolved into the row.
8. **The check runs trusted code.** No file added or changed by a PR is executed or built,
   and no workflow, build or tool configuration from the PR is used, while checking that PR's
   request. The check reads the PR only as data: its change set and the one request file it
   adds. *(Assumption, planning pass 2026-10-25: a request file that is a symbolic link fails
   the check unread. Following it would read a file outside the PR into a public report.)*
9. **A changed archive blocks its request.** A mismatch between the archive seen at check
   time and at apply time fails the apply for that request before anything for it is written.
10. **An unreachable default only warns.** It never fails the check, because State, not this
   repo, owns the default cascade.

## Out of Scope

- The `sdkman-hooks` change that serves `contrib` its tarball post-install hook. Delegated to
  Marco, who releases `sdkman-hooks`.
- Creating the `contrib` candidate in State and its scoped vendor account. Delegated to Marco.

- Following redirects to anything other than an `https` archive.
- Changing what State stores for a URL.
- Updating `contrib` automatically on a contributor's machine.
- Publishing `contrib` to crates.io.

## Acceptance Criteria

- [ ] Once Marco has made the hooks change and created the candidate: on a SDKMAN installed before `contrib` was registered, `sdk update` followed by `sdk install contrib` installs a working `contrib` on Linux x64, Linux ARM64, macOS x64, macOS ARM64 and Windows x64, and `sdk upgrade contrib` moves to a newer release
- [ ] After that one-time setup, a `cargo release` on `main` publishes it to State through the vendor role as the new default `contrib` version for all five platforms, with no manual step
- [ ] Every published `contrib` archive passes SDKMAN's well-formed archive rules
- [ ] `contrib` is not listed in `contrib-candidates.txt`
- [ ] A release that fails to publish any platform leaves the previous default in place and shows as failed
- [ ] Each release also carries a working binary per platform on GitHub
- [ ] The README presents `sdk install contrib` as the recommended install path, names the GitHub binaries as the fallback, and shows how to confirm the install worked
- [ ] The README mentions `cargo install --git` as a side note for contributors who already have Rust
- [ ] The README and the releases can be read by someone outside the `sdkman` organisation
- [ ] A request whose archive URL is a GitHub release download passes the URL rule, and the archive rules judge the downloaded file
- [ ] A redirect chain with a non-`https` hop, no final `200`, or no end fails the check
- [ ] A chain of 5 `https` redirects ending in `200` passes, and a chain of 6 fails
- [ ] The row written to State carries the URL as the contributor wrote it
- [ ] A request PR that also changes `contrib`'s code cannot make its own check pass, and still fails for touching files outside its request
- [ ] No code from a PR is executed while checking that PR's request
- [ ] An archive that changed between the check and the apply fails the apply for that request, nothing for it is written, and it stays pending
- [ ] While a request is blocked, later requests for the same candidate stay pending and requests for other candidates still apply
- [ ] The apply never reads digests from a `pull_request` run, and a missing or expired `digests.json` blocks the request
- [ ] A release whose `default` version has neither a `UNIVERSAL` nor a `LINUX_X64` row gets a `WARN`, and the check still passes
- [ ] `scripts/protect-main.sh` creates the `main` ruleset if it is absent and otherwise updates it with `PUT`. `scripts/protect-main.sh --check` exits non-zero when the live ruleset's `conditions`, `rules` or `bypass_actors` differ from `main.json`, and it exits zero against the live repository
