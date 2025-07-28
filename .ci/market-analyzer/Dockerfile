FROM alpine:3.21

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.7.0/health-check-aarch64-unknown-linux-musl /usr/bin/health-check

ENV NO_COLOR=1

COPY --chmod=755 ./mainnet-factories.toml /app/packages/perps-exes/assets
COPY --chmod=755 perps-market-params /usr/bin/perps-market-params

WORKDIR /app

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Market Analyzer (ECS Testnet)", "perps-market-params" ]

CMD [ "serve" ]
