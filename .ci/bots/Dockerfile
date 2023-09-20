FROM alpine:3.14.2

COPY perps-bots /usr/bin
COPY amber.yaml /etc/amber.yaml

ADD "https://github.com/fpco/amber/releases/download/v0.1.3/amber-x86_64-unknown-linux-musl" "/usr/bin/amber"
RUN chmod 755 /usr/bin/amber

ADD https://github.com/fpco/pid1/releases/download/v0.1.3.1/pid1 /usr/bin/pid1

RUN chmod +x /usr/bin/pid1

ENV AMBER_YAML=/etc/amber.yaml

ADD https://levana-tools.s3.us-west-2.amazonaws.com/base_image/health_check.sh /health_check.sh
ADD https://levana-tools.s3.us-west-2.amazonaws.com/base_image/logging.sh /logging.sh

RUN apk add --no-cache bash aws-cli && chmod a+x /health_check.sh /logging.sh


ENTRYPOINT [ "pid1" ]

CMD ["/logging.sh", "/usr/bin/amber", "--amber-yaml", "/etc/amber.yaml", "--unmasked", "exec", "/usr/bin/perps-bots"]
