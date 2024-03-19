#!/usr/bin/env bash

set -euo pipefail

docker run --rm -it --name osmolocaltest -p 26657:26657 -p 1317:1317 -p 9090:9090 -p 9091:9091 localosmo:latest
