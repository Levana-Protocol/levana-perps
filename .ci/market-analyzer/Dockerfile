FROM alpine:3.20.1

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.4.0/health-check-x86_64-unknown-linux-musl /usr/bin/health-check

ENV NO_COLOR=1

RUN mkdir -p /app/packages/perps-exes/assets

WORKDIR /app

COPY --chmod=755 ./mainnet-factories.toml /app/packages/perps-exes/assets
COPY --chmod=755 perps-market-params /usr/bin/perps-market-params

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Market Analyzer (K8s mainnet)", "perps-market-params" ]

CMD [ "serve" ]
