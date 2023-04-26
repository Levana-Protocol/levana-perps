FROM alpine:3.14.2

COPY perps-bots /usr/bin
COPY amber.yaml /etc/amber.yaml

ADD "https://github.com/fpco/amber/releases/download/v0.1.3/amber-x86_64-unknown-linux-musl" "/usr/bin/amber"
RUN chmod 755 /usr/bin/amber

ENV AMBER_YAML=/etc/amber.yaml

CMD ["/usr/bin/amber", "--amber-yaml", "/etc/amber.yaml", "--unmasked", "exec", "/usr/bin/perps-bots"]
