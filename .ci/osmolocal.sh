#!/usr/bin/env bash

set -euo pipefail

if [[ ${1:-ignore} == "--no-terminal" ]]
then
  FLAG=""
else
  FLAG="-it"
fi

docker run --rm $FLAG --name osmolocaltest -p 26657:26657 -p 1317:1317 -p 9090:9090 -p 9091:9091 ghcr.io/levana-protocol/localosmosis:af593018cd05934d33869db3d0e0fa23cd96eb68
