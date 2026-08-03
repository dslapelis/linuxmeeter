# Publishing to the AUR

`packaging/PKGBUILD` is the source of truth for the `linuxmeeter-git` AUR package.
`.github/workflows/aur.yml` pushes it to the AUR; `.github/workflows/packaging.yml`
builds it for real in a clean Arch container on every change so it cannot rot.

The workflow cannot bootstrap itself — the three steps below need a human with the
AUR account, and only have to be done once.

## 1. Register an SSH key with the AUR

Generate a key used *only* for this. Leave the passphrase empty; CI cannot type one.

```sh
ssh-keygen -t ed25519 -C "aur@linuxmeeter-ci" -f ~/.ssh/aur_linuxmeeter -N ""
```

Sign in at <https://aur.archlinux.org/> → *My Account* → paste the contents of
`~/.ssh/aur_linuxmeeter.pub` into **SSH Public Key** → save.

## 2. Create the package repo with a first manual push

The AUR creates a package repo on first push, so this one has to happen from your
machine. From a clean directory:

```sh
git clone ssh://aur@aur.archlinux.org/linuxmeeter-git.git
cd linuxmeeter-git
cp /path/to/linuxmeeter/packaging/PKGBUILD .
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial import"
git push origin master
```

Build it once locally before pushing — `makepkg -si` — so the first thing users
see is not a broken package.

## 3. Add the private key as a repo secret

```sh
gh secret set AUR_SSH_PRIVATE_KEY < ~/.ssh/aur_linuxmeeter
```

Or: GitHub → Settings → Secrets and variables → Actions → New repository secret,
named `AUR_SSH_PRIVATE_KEY`, with the contents of the **private** key file.

Until this secret exists the publish workflow logs a warning and exits cleanly, so
it is safe to merge before you get to this.

## After that

The publish workflow runs on a `v*` tag, on any change to `packaging/PKGBUILD`
landing on `main`, and on manual dispatch. It regenerates `.SRCINFO`, and pushes
only if something actually changed.

Note that a `-git` package tracks HEAD: users get new commits whenever they rebuild,
without anything being republished. Pushing to the AUR only matters when the
*PKGBUILD itself* changes — a new dependency, a moved install path, a changed build
step. The tag trigger is there because you asked for it, not because a tag requires
a republish.

## Adding a stable package later

When you tag a real release, the stable `linuxmeeter` package is a second PKGBUILD
alongside this one: `source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")`
with a real `sha256sums`, no `pkgver()`, and a release workflow that bumps `pkgver`
and updates the checksum. Keep the `-git` package published either way — they
coexist, and AUR users expect both.
