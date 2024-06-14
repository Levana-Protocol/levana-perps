FROM alpine:3.19.1

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.3.0/health-check-x86_64-unknown-linux-musl /usr/bin/health-check

RUN mkdir -p /app/packages/perps-exes/assets

USER 1000

WORKDIR /app

COPY --chmod=755 --chown=1000 ./mainnet-factories.toml /app/packages/perps-exes/assets
COPY --chmod=755 --chown=1000 perps-deploy /usr/bin/perps-deploy

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Perps Deploy (K8s mainnet)", "perps-deploy" ]

CMD [ "serve" ]
