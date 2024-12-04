#!/usr/bin/env bash

set -euo pipefail

if [[ ${1:-ignore} == "--no-terminal" ]]
then
  FLAG=""
else
  FLAG="-it"
fi

docker run --rm $FLAG --name osmolocaltest -p 26657:26657 -p 1317:1317 -p 9090:9090 -p 9091:9091 ghcr.io/levana-protocol/localosmosis:cb1d38ef898dc287380a4ee8f647fd4f89361e53
