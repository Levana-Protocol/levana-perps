FROM alpine:3.21

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.7.0/health-check-aarch64-unknown-linux-musl /usr/bin/health-check
ENV NO_COLOR=1

COPY --chmod=755 --chown=1000 ./mainnet-factories.toml /app/packages/perps-exes/assets
COPY --chmod=755 --chown=1000 perps-deploy /usr/bin/perps-deploy

WORKDIR /app

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Perps Deploy", "--can-exit", "perps-deploy" ]

CMD [ "util", "top-traders" ]
