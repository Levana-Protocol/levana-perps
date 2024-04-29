FROM alpine:3.19.1

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.1.0/health-check-x86_64-unknown-linux-musl /usr/bin/health-check

USER 1000

COPY --chmod=755 --chown=1000 perps-market-params /usr/bin/perps-market-params

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Market Analyzer (K8s mainnet)", "perps-market-params" ]

CMD [ "serve" ]
