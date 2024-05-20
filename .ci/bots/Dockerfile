FROM alpine:3.19.1

ADD --chmod=755  https://github.com/fpco/amber/releases/download/v0.1.3/amber-x86_64-unknown-linux-musl /usr/bin/amber
ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.1.0/health-check-x86_64-unknown-linux-musl /usr/bin/health-check

COPY amber.yaml /etc/amber.yaml
COPY packages/perps-exes/assets/*.toml /etc
COPY perps-bots /usr/bin

RUN chmod 755 /usr/bin/perps-bots

ENV AMBER_YAML=/etc/amber.yaml
ENV LEVANA_BOTS_PRICE_CONFIG=/etc/config-price.toml
ENV LEVANA_BOTS_CONFIG_TESTNET=/etc/config-testnet.toml
ENV LEVANA_BOTS_CONFIG_CHAIN=/etc/config-chain.toml

ENTRYPOINT [ "health-check", "--task-output-timeout", "120" ]

CMD ["/usr/bin/amber", "--amber-yaml", "/etc/amber.yaml", "--unmasked", "exec", "/usr/bin/perps-bots"]
