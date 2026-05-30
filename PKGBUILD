# Maintainer: RouHim <rouhim@users.noreply.github.com>

pkgname=core-probe
pkgver=0.0.0
pkgrel=1
pkgdesc="A Linux CLI tool to identify unstable AMD CPU cores using mprime stress testing"
arch=('x86_64')
url="https://github.com/RouHim/core-probe"
license=('MIT')
makedepends=('cargo')
source=("${pkgname}-${pkgver}.tar.gz::${url}/archive/${pkgver}.tar.gz")
sha256sums=('SKIP')

build() {
  cd "${pkgname}-${pkgver}"
  cargo build --release --locked
}

package() {
  cd "${pkgname}-${pkgver}"
  install -Dm755 target/release/${pkgname} "${pkgdir}/usr/bin/${pkgname}"
  install -Dm644 .desktop/${pkgname}.desktop "${pkgdir}/usr/share/applications/${pkgname}.desktop"
}
