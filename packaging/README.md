# Releasing and packaging

Everything here is a template for the maintainer; none of it is built into the
program.

## Cut a release

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows,
   and commit both.
2. Make sure CI is green on `master` (Linux and macOS).
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
   `.github/workflows/release.yml` refuses a tag that doesn't match the crate
   version, runs the tests, builds Linux and macOS (Intel and Apple Silicon)
   binaries, and attaches `formatui-<tag>-<target>.tar.gz` plus a `.sha256`
   to the GitHub release. Runner labels change over time; if a leg can't find
   its runner, update `os:` in that workflow.

## crates.io (`cargo install formatui`)

```
cargo publish --dry-run     # then, if happy:
cargo publish
```

Needs a crates.io token (`cargo login`). `exclude` in `Cargo.toml` keeps
`.github/`, `packaging/` and `example/` out of the crate. The name was free
when this was written; check again before the first publish.

## Publishing to Homebrew and the AUR

`release.yml` has two jobs that run after the binaries build, on every plain
`vX.Y.Z` tag (pre-release tags such as `v0.2.0-rc1` are skipped):

- **homebrew** commits the new `url` and `sha256` to your tap's formula.
- **aur** sets `pkgver`, fills in the checksums, test-builds the package in an
  Arch container, regenerates `.SRCINFO` and pushes to aur.archlinux.org.

Both stay off until you finish the one-time setup below and set the matching
repository variable, so tagging a release never fails half-configured.

### One-time setup: Homebrew tap

1. Create the tap repo (public, name must start with `homebrew-`):
   ```
   gh repo create simoncoulton/homebrew-formatui --public \
     --description "Homebrew tap for formatui"
   ```
2. Seed it with the formula, with a real checksum for the first release.
   Do this *after* you have pushed the `v0.1.0` tag, because the tarball has
   to exist:
   ```
   git clone git@github.com:simoncoulton/homebrew-formatui.git && cd homebrew-formatui
   mkdir Formula && cp ../formatui/packaging/homebrew/formatui.rb Formula/
   sha=$(curl -fsSL https://github.com/simoncoulton/formatui/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256 | cut -d' ' -f1)
   sed -i '' "s/REPLACE_WITH_SHA256_OF_THE_TARBALL/$sha/" Formula/formatui.rb   # GNU sed: drop the ''
   git add . && git commit -m "formatui 0.1.0" && git push
   ```
3. Test it on a Mac:
   ```
   brew tap simoncoulton/formatui
   brew install --build-from-source formatui
   brew test formatui
   brew audit --new --strict simoncoulton/formatui/formatui
   ```
   The formula's test sets `HOME` to a temp dir, expects `formatui --version`
   to print the version, and expects
   `formatui test Example.Show.2024.S01E02.1080p.mkv` to print
   `Example Show - S01E02.mkv` using the default config (checked against the
   0.1.0 binary).
4. Let the workflow push to the tap. Create a **fine-grained personal access
   token** (GitHub > Settings > Developer settings): repository access limited
   to `homebrew-formatui`, permission *Contents: Read and write*. Then in the
   `formatui` repo:
   - Settings > Secrets and variables > Actions > **Secrets**: add
     `HOMEBREW_TAP_TOKEN` with that token.
   - **Variables**: add `PUBLISH_HOMEBREW` = `true`.

Users install with `brew install simoncoulton/formatui/formatui`.

### One-time setup: AUR

1. Create an account at <https://aur.archlinux.org/register> and confirm your
   email.
2. Make a key used only for the AUR and add the **public** half under
   My Account > SSH Public Key:
   ```
   ssh-keygen -t ed25519 -f ~/.ssh/aur_formatui -C "aur formatui" -N ""
   cat ~/.ssh/aur_formatui.pub
   ```
3. Check the name is still free: <https://aur.archlinux.org/packages?K=formatui>.
   If it is taken, change `pkgname` in `packaging/aur/PKGBUILD` and in the
   workflow's `pkgname:`.
4. Check `packaging/aur/PKGBUILD`: the `# Maintainer:` email, and the
   `depends` list (build once on Arch, then `namcap` the result).
5. In the `formatui` repo, Settings > Secrets and variables > Actions:
   - **Secrets**: `AUR_SSH_PRIVATE_KEY` = the contents of
     `~/.ssh/aur_formatui` (the private half), and `AUR_EMAIL` = the email to
     put on AUR commits.
   - **Variables**: `PUBLISH_AUR` = `true`.
6. The first tag push creates the package: pushing to a package name that does
   not exist yet creates it on the AUR. If you would rather do the first
   upload by hand, use the manual steps below.

Manual AUR upload (first time, or when the workflow is off):

```
git clone ssh://aur@aur.archlinux.org/formatui.git aur-formatui   # empty for a new package
cp packaging/aur/PKGBUILD aur-formatui/ && cd aur-formatui
updpkgsums                          # fills in sha256sums from the tag
makepkg -f                          # builds and runs the tests
namcap PKGBUILD formatui-*.pkg.tar.zst
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "formatui 0.1.0" && git push
```

Optional extra AUR packages: `formatui-git` (build from `master`) and
`formatui-bin` (install the release tarball instead of compiling).

### Order of events for a release

1. Bump the version, commit, wait for CI (see "Cut a release").
2. `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The workflow builds binaries, publishes the GitHub release, then updates the
   tap and the AUR. Check the **Actions** tab; each job names what it did.
4. `brew update && brew upgrade formatui` on a Mac and `yay -Syu formatui`
   (or `paru`) on Arch to confirm.

The very first release has one manual step (seeding the tap, above) because the
formula needs a real checksum before the workflow can edit it.
