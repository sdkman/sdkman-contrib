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

**A contributor can install `contrib` without building it.** The README tells a new
contributor how to get a working `contrib` on their machine, step by step, for every platform
SDKMAN supports for contributors: Linux x64, Linux ARM64, macOS x64, macOS ARM64 and Windows
x64. They need no Rust toolchain, no clone of the repo and no prior knowledge of GitHub
releases. Installing is the recommended path, because it gives the contributor `contrib new`
and `contrib check` before they push. A hand-written file stays valid, but the README no
longer presents installing as optional extra.

**A published release exists before the process is announced.** At least one tagged release
carries a binary for each platform above, and the install steps in the README work against it
as written. Contributors can reach the releases and the README without being members of the
`sdkman` organisation.

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

1. **Install steps cover every contributor platform.** Linux x64, Linux ARM64, macOS x64,
   macOS ARM64 and Windows x64 each have install steps in the README, and each has a binary in
   every release. A platform without a binary is not listed as supported.
2. **Install steps are verifiable.** The README shows how to confirm the install worked, for
   example by running `contrib` and seeing its version.
3. **Redirect chains stay on `https`.** A chain with any non-`https` hop fails the URL rule.
   A chain that never ends in a `200` fails it too, as does one longer than a sane bound.
4. **The final file is what the archive rules judge.** The `.zip` or `.tar.gz` rule, the
   archive format, the base directory and `bin/` rules all apply to the archive at the end of
   the chain.
5. **State receives the URL as written.** A redirect is never resolved into the row.
6. **The check runs trusted code.** No file added or changed by a PR is executed, built or
   loaded while checking that PR's request.
7. **A changed archive blocks its request.** A mismatch between the archive seen at check
   time and at apply time fails the apply for that request before anything for it is written.
8. **An unreachable default only warns.** It never fails the check, because State, not this
   repo, owns the default cascade.

## Open Questions

- Should `contrib` also be installable through SDKMAN itself (`sdk install contrib`)? It
  would be the most natural channel for this audience, but it makes the tool a SDKMAN
  candidate with its own release path.

## Out of Scope

- Following redirects to anything other than an `https` archive.
- Changing what State stores for a URL.
- Updating `contrib` automatically on a contributor's machine.
- Any change to how the apply orders, retries or voids requests.

## Acceptance Criteria

- [ ] The README has install steps for Linux x64, Linux ARM64, macOS x64, macOS ARM64 and Windows x64 that need no Rust toolchain or clone, and a way to confirm the install worked
- [ ] The README presents installing `contrib` as the recommended path for contributors
- [ ] A published release carries a working binary for each of those five platforms, and the README's steps work against it as written
- [ ] The README and the releases can be read by someone outside the `sdkman` organisation
- [ ] A request whose archive URL is a GitHub release download passes the URL rule, and the archive rules judge the downloaded file
- [ ] A redirect chain with a non-`https` hop, no final `200`, or no end fails the check
- [ ] The row written to State carries the URL as the contributor wrote it
- [ ] A request PR that also changes `contrib`'s code cannot make its own check pass, and still fails for touching files outside its request
- [ ] No code from a PR is executed while checking that PR's request
- [ ] An archive that changed between the check and the apply fails the apply for that request, nothing for it is written, and it stays pending
- [ ] A release whose `default` version has neither a `UNIVERSAL` nor a `LINUX_X64` row gets a `WARN`, and the check still passes
- [ ] `.github/rulesets/main.json` names the real bypass App and matches `main`'s live protection
