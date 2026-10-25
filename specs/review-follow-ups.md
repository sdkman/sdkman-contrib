# Review Follow-ups

A review of `sdkman-contrib` before launch found gaps that would stop the process working for
real contributors, or would let a request pass the check without the check being trustworthy.
The biggest is that most community candidates publish on GitHub releases, and every GitHub
release download URL answers a redirect, which the check fails today. The README also never
tells a new contributor how to install `contrib`, so the local check, the fastest feedback a
contributor can get, is out of reach for most of them.

This spec states what must change. It amends
[`community-contributions.md`](community-contributions.md); where the two disagree, this one
wins.

*Reference: review of `main` at `9dd792b` on 2026-10-25.*

## Behaviour

**`contrib` is a SDKMAN candidate.** A contributor installs it with `sdk install contrib` and
keeps it current with `sdk upgrade contrib`, on Linux x64, Linux ARM64, macOS x64, macOS ARM64
and Windows x64. Every contributor is a SDKMAN user, so the tool reaches them through the
channel they already have, and an upgrade brings them the check's latest rules. They need no
Rust toolchain, no clone of the repo and no prior knowledge of GitHub releases.

**Every release of `contrib` reaches SDKMAN on its own.** Tagging a release publishes it as a
new `contrib` version in State, for every platform above, and makes it the default. It is
published as a vendor publishes its own candidate, through State's vendor role, never through
a `contrib` request. So `contrib` is never listed in `contrib-candidates.txt`, and a broken
`contrib` can never block its own fix. Each published archive is well formed by SDKMAN's own
rules: one `contrib-<version>` base directory holding `bin/contrib`.

**The README tells a contributor how to install it.** Installing through SDKMAN is the
recommended path, because it gives the contributor `contrib new` and `contrib check` before
they push. The README also names the GitHub release binaries as a fallback for anyone who
cannot use SDKMAN. A hand-written file stays valid, but the README no longer presents
installing as an optional extra.

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

**What is applied is what was reviewed.** The archive the apply downloads must be the same
archive the check downloaded for the reviewed PR, byte for byte, whether or not the
contributor declared a checksum. If an archive changed between the check and the apply, that
request fails, writes nothing to State, and stays pending for a maintainer to void or for a
corrected request to supersede.

**A default that can never take effect is flagged.** A release whose `default` version has
neither a `UNIVERSAL` nor a `LINUX_X64` row gets a `WARN` on the check, so the reviewer sees it
instead of having to spot it.

**The committed ruleset is the live ruleset.** `.github/rulesets/main.json` describes `main`'s
protection as it is actually applied, including the GitHub App that may bypass it. It holds no
placeholder that has to be filled in elsewhere.

## Business Rules

1. **Every contributor platform is covered.** Linux x64, Linux ARM64, macOS x64, macOS ARM64
   and Windows x64 each get a `contrib` version in State and a binary on GitHub with every
   release. A platform without one is not listed as supported.
2. **Install steps are verifiable.** The README shows how to confirm the install worked, for
   example by running `contrib` and seeing its version.
3. **`contrib` publishes itself as a vendor.** Its releases reach State through the vendor
   role, and `contrib` is never listed in `contrib-candidates.txt`.
4. **A release reaches SDKMAN only when it is complete.** A release that cannot publish every
   platform shows as a failed release, and the previous default stays the default.
5. **Redirect chains stay on `https`.** A chain with any non-`https` hop fails the URL rule.
   A chain that never ends in a `200` fails it too, as does one longer than a sane bound.
6. **The final file is what the archive rules judge.** The `.zip` or `.tar.gz` rule, the
   archive format, the base directory and `bin/` rules all apply to the archive at the end of
   the chain.
7. **State receives the URL as written.** A redirect is never resolved into the row.
8. **The check runs trusted code.** No file added or changed by a PR is executed, built or
   loaded while checking that PR's request.
9. **A changed archive blocks its request.** A mismatch between the archive seen at check
   time and at apply time fails the apply for that request before anything for it is written.
10. **An unreachable default only warns.** It never fails the check, because State, not this
   repo, owns the default cascade.

## Out of Scope

- Following redirects to anything other than an `https` archive.
- Changing what State stores for a URL.
- Updating `contrib` automatically on a contributor's machine.
- Publishing `contrib` to crates.io.
- Any change to how the apply orders, retries or voids requests.

## Acceptance Criteria

- [ ] `sdk install contrib` installs a working `contrib` on Linux x64, Linux ARM64, macOS x64, macOS ARM64 and Windows x64, and `sdk upgrade contrib` moves to a newer release
- [ ] Tagging a release publishes it to State through the vendor role as the new default `contrib` version for all five platforms, with no manual step
- [ ] Every published `contrib` archive passes SDKMAN's well-formed archive rules
- [ ] `contrib` is not listed in `contrib-candidates.txt`
- [ ] A release that fails to publish any platform leaves the previous default in place and shows as failed
- [ ] Each release also carries a working binary per platform on GitHub
- [ ] The README presents `sdk install contrib` as the recommended install path, names the GitHub binaries as the fallback, and shows how to confirm the install worked
- [ ] The README mentions `cargo install --git` as a side note for contributors who already have Rust
- [ ] The README and the releases can be read by someone outside the `sdkman` organisation
- [ ] A request whose archive URL is a GitHub release download passes the URL rule, and the archive rules judge the downloaded file
- [ ] A redirect chain with a non-`https` hop, no final `200`, or no end fails the check
- [ ] The row written to State carries the URL as the contributor wrote it
- [ ] A request PR that also changes `contrib`'s code cannot make its own check pass, and still fails for touching files outside its request
- [ ] No code from a PR is executed while checking that PR's request
- [ ] An archive that changed between the check and the apply fails the apply for that request, nothing for it is written, and it stays pending
- [ ] A release whose `default` version has neither a `UNIVERSAL` nor a `LINUX_X64` row gets a `WARN`, and the check still passes
- [ ] `.github/rulesets/main.json` names the real bypass App and matches `main`'s live protection
