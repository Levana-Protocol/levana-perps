#!/usr/bin/env bash

set -euo pipefail

pushd source-tarballs
for f in levana-perps-$1{.tar.gz,-checksums.txt}
do
  set -x
  aws s3 cp $f s3://static.meteors.levana.finance/perps-source/$f
  curl -i --head https://static.levana.finance/perps-source/$f
  set +x
done
popd

DEST=../docs/src/source-code.md
echo "| \`$1\` | [Code](https://static.levana.finance/perps-source/levana-perps-$1.tar.gz) | [Checksums](https://static.levana.finance/perps-source/levana-perps-$1-checksums.txt) |" >> "$DEST"
echo "Don't forget to push changes for $DEST"
