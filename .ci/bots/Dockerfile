FROM alpine:3.21

ADD --chmod=755 https://github.com/fpco/amber/releases/download/v0.1.7/amber-aarch64-unknown-linux-musl /usr/bin/amber
ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.7.0/health-check-aarch64-unknown-linux-musl /usr/bin/health-check

ENV AMBER_YAML=/etc/amber.yaml
ENV LEVANA_BOTS_PRICE_CONFIG=/etc/config-price.toml
ENV LEVANA_BOTS_CONFIG_TESTNET=/etc/config-testnet.toml
ENV LEVANA_BOTS_CONFIG_CHAIN=/etc/config-chain.toml
ENV NO_COLOR=1

COPY amber.yaml /etc/amber.yaml
COPY ["etc/*.toml", "/etc"]
COPY --chmod=755 perps-bots /usr/bin

ENTRYPOINT [ "health-check", "--task-output-timeout", "120" ]

CMD ["/usr/bin/amber", "--amber-yaml", "/etc/amber.yaml", "--unmasked", "exec", "/usr/bin/perps-bots"]
