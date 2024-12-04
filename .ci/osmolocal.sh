#!/usr/bin/env bash

set -euo pipefail

if [[ ${1:-ignore} == "--no-terminal" ]]
then
  FLAG=""
else
  FLAG="-it"
fi

docker run --rm $FLAG --name osmolocaltest -p 26657:26657 -p 1317:1317 -p 9090:9090 -p 9091:9091 ghcr.io/levana-protocol/localosmosis:e039701eea59c6576009814fefd3c1b73f7666ec
