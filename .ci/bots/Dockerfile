FROM alpine:3.14.2

COPY perps-bots /usr/bin
COPY amber.yaml /etc/amber.yaml

ADD "https://github.com/fpco/amber/releases/download/v0.1.3/amber-x86_64-unknown-linux-musl" "/usr/bin/amber"
RUN chmod 755 /usr/bin/amber

ADD "https://github.com/Levana-Protocol/devops/releases/download/v0.1/health-check" "/usr/bin/health-check"
RUN chmod 755 /usr/bin/health-check

RUN apk add --no-cache curl bash aws-cli

ENV AMBER_YAML=/etc/amber.yaml

ENTRYPOINT [ "health-check" ]

CMD ["/usr/bin/amber", "--amber-yaml", "/etc/amber.yaml", "--unmasked", "exec", "/usr/bin/perps-bots"]
