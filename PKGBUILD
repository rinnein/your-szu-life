# Maintainer: rinnein
# GCC LTO objects from bundled C libraries cannot be linked by Rust's LLD.
options=('!lto')
pkgname=your-szu-life-git
pkgver=r1.g0000000
pkgrel=1
pkgdesc='Shenzhen University campus life application'
arch=('x86_64')
url='https://github.com/rinnein/your-szu-life'
license=('unknown')
depends=(
  'libayatana-appindicator'
  'webkit2gtk-4.1'
  'gtk3'
)
makedepends=(
  'base-devel'
  'dpkg'
  'git'
  'pnpm'
  'rust'
)
provides=('your-szu-life')
conflicts=('your-szu-life')
source=("$pkgname::git+https://github.com/rinnein/your-szu-life.git")
sha256sums=('SKIP')

pkgver() {
  cd "$srcdir/$pkgname"
  printf 'r%s.%s\n' "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

build() {
  cd "$srcdir/$pkgname"
  pnpm install --frozen-lockfile
  pnpm tauri build --ci --bundles deb

  local debs=(src-tauri/target/release/bundle/deb/*.deb)
  if (( ${#debs[@]} != 1 )); then
    printf 'expected one Debian package, found %s\n' "${#debs[@]}" >&2
    return 1
  fi

  rm -rf "$srcdir/package-root"
  dpkg-deb -x "${debs[0]}" "$srcdir/package-root"
}

package() {
  cp -a "$srcdir/package-root/usr" "$pkgdir/"
}
