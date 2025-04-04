#!/usr/bin/env bash

set -euxo pipefail

if [ -z ${1+x} ]
then
    echo "Please provide a Git hash or other tree-ish"
    exit 1
fi

rm -rf tmp
mkdir -p tmp
git archive -o tmp/source.tar "$1"

pushd tmp
tar xf source.tar
rm source.tar
popd

DIR=levana-perps-$1
rm -rf "$DIR"
mkdir -p "$DIR"

cp -i .ci/source-tarball-misc/* "$DIR"
mv -i tmp/{Cargo.lock,LICENSE,rust-toolchain.toml} "$DIR"
mkdir "$DIR/packages"
mv -i tmp/packages/{msg,shared} "$DIR/packages"
mkdir "$DIR/contracts"
mv -i tmp/contracts/{factory,liquidity_token,position_token,market} "$DIR/contracts"

rm -rf tmp

pushd "$DIR"
cargo test # not just sanity, but also fixes up the Cargo.lock file
rm -rf target
./build.sh
popd

mkdir -p source-tarballs
cp "$DIR/wasm/artifacts/checksums.txt" "source-tarballs/$DIR-checksums.txt"
rm -rf "$DIR/wasm"
tar czfv "source-tarballs/$DIR.tar.gz" "$DIR"
rm -rf "$DIR"
