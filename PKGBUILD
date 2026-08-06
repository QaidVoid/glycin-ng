# Maintainer: Samueru-sama xdglawyer@outlook.com

pkgbase=glycin-ng
pkgname=('glycin-ng' 'glycin-ng-librsvg')
pkgver=0.3.2
pkgrel=1
pkgdesc="In-process image decoder without bwrap dependency or useless bloat"
arch=('x86_64' 'aarch64')
url=https://github.com/QaidVoid/glycin-ng
license=('MIT' 'Apache-2.0')
makedepends=('cargo' 'rust' 'git')
# ABI version the librsvg shim implements; used for its provides,
# pkg-config file, and the upstream headers fetched below.
_librsvg_ver=2.62.3
source=("$pkgbase::git+https://github.com/QaidVoid/glycin-ng.git#tag=$pkgver"
        "glycin-2-header::https://raw.githubusercontent.com/GNOME/glycin/80463391d9e8f3f136f48e5fd6a63c0bf116e884/libglycin/include/glycin.h"
        "https://raw.githubusercontent.com/GNOME/librsvg/$_librsvg_ver/include/librsvg/rsvg.h"
        "https://raw.githubusercontent.com/GNOME/librsvg/$_librsvg_ver/include/librsvg/rsvg-cairo.h"
        "https://raw.githubusercontent.com/GNOME/librsvg/$_librsvg_ver/include/librsvg/rsvg-pixbuf.h"
        "https://raw.githubusercontent.com/GNOME/librsvg/$_librsvg_ver/include/librsvg/rsvg-features.h.in"
        "https://raw.githubusercontent.com/GNOME/librsvg/$_librsvg_ver/include/librsvg/rsvg-version.h.in")
sha256sums=('SKIP'
            'SKIP'
            'SKIP'
            'SKIP'
            'SKIP'
            'SKIP'
            'SKIP')

build() {
  cd "$srcdir"/"$pkgbase"
  cargo build --release -p glycin-ng-c
  cargo build --release -p glycin-ng-libglycin-shim
  cargo build --release -p glycin-ng-librsvg-shim
}

package_glycin-ng() {
  depends=('glibc')
  provides=('glycin')
  conflicts=('glycin')
  replaces=('glycin')

  cd "$srcdir"/"$pkgbase"
  install -Dm755 ./target/release/libglycin_ng.so \
    "$pkgdir"/usr/lib/libglycin_ng.so
  install -Dm755 ./target/release/libglycin_2.so \
    "$pkgdir"/usr/lib/libglycin-2.so.0
  install -Dm644 ./include/glycin_ng.h \
    "$pkgdir"/usr/include/glycin-ng/glycin_ng.h
  install -Dm644 "$srcdir"/glycin-2-header \
    "$pkgdir"/usr/include/glycin-2/glycin.h

  install -d "$pkgdir"/usr/lib/pkgconfig
  sed -e "s|@PREFIX@|/usr|g" \
      -e "s|@VERSION@|$pkgver|g" \
    ./pkgconfig/glycin-ng.pc.in \
    > "$pkgdir"/usr/lib/pkgconfig/glycin-ng.pc

  sed -e "s|@PREFIX@|/usr|g" \
      -e "s|@VERSION@|2.1.1|g" \
    ./pkgconfig/glycin-2.pc.in \
    > "$pkgdir"/usr/lib/pkgconfig/glycin-2.pc
}

package_glycin-ng-librsvg() {
  pkgdesc="librsvg-2.so.2 compat shim backed by glycin-ng's SVG engine"
  depends=('cairo' 'gcc-libs' 'gdk-pixbuf2' 'glib2' 'glibc' 'glycin-ng')
  provides=("librsvg=2:$_librsvg_ver" 'librsvg-2.so=2-64')
  conflicts=('librsvg')
  replaces=('librsvg')

  cd "$srcdir"/"$pkgbase"
  install -Dm755 ./target/release/librsvg_2.so \
    "$pkgdir"/usr/lib/librsvg-2.so.2
  ln -s librsvg-2.so.2 "$pkgdir"/usr/lib/librsvg-2.so

  # Headers are upstream's own; the two generated ones get the same
  # substitutions meson applies.
  local _v=(${_librsvg_ver//./ })
  install -Dm644 "$srcdir"/rsvg.h \
    "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg.h
  install -Dm644 "$srcdir"/rsvg-cairo.h \
    "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-cairo.h
  install -Dm644 "$srcdir"/rsvg-pixbuf.h \
    "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-pixbuf.h
  sed -e "s|@LIBRSVG_HAVE_PIXBUF@|TRUE|g" \
    "$srcdir"/rsvg-features.h.in \
    > "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-features.h
  sed -e "s|@LIBRSVG_MAJOR_VERSION@|${_v[0]}|g" \
      -e "s|@LIBRSVG_MINOR_VERSION@|${_v[1]}|g" \
      -e "s|@LIBRSVG_MICRO_VERSION@|${_v[2]}|g" \
      -e "s|@PACKAGE_VERSION@|$_librsvg_ver|g" \
    "$srcdir"/rsvg-version.h.in \
    > "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-version.h
  chmod 644 "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-features.h \
    "$pkgdir"/usr/include/librsvg-2.0/librsvg/rsvg-version.h

  install -d "$pkgdir"/usr/lib/pkgconfig
  sed -e "s|@PREFIX@|/usr|g" \
      -e "s|@VERSION@|$_librsvg_ver|g" \
    ./pkgconfig/librsvg-2.0.pc.in \
    > "$pkgdir"/usr/lib/pkgconfig/librsvg-2.0.pc
}
