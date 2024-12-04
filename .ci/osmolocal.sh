#!/usr/bin/env bash

set -euo pipefail

if [[ ${1:-ignore} == "--no-terminal" ]]
then
  FLAG=""
else
  FLAG="-it"
fi

docker run --rm $FLAG --name osmolocaltest -p 26657:26657 -p 1317:1317 -p 9090:9090 -p 9091:9091 ghcr.io/levana-protocol/localosmosis:52fe4e899accde76d7915ea61f4fe8ece3f92a7c
