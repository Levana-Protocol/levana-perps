#!/usr/bin/env bash

set -exuo pipefail

ROOT=$( dirname "$( readlink -f "${BASH_SOURCE[-1]}" )" )

docker run --rm -t -p 1948:1948 -v "$ROOT:/slides" docker.io/webpronl/reveal-md:latest /slides --css assets/style.css --highlight-theme github
