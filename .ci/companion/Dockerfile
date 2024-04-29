FROM alpine:3.19.1

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.1.0/health-check-x86_64-unknown-linux-musl /usr/bin/health-check

RUN apk update && apk add fontconfig

ADD --chmod=444 https://static.levana.finance/PublicSans-Regular.ttf /usr/share/fonts/PublicSans-Regular.ttf

# Force re-generation of font information cache files
RUN fc-cache -fv

COPY perps-companion /usr/bin/perps-companion

ENTRYPOINT [ "health-check", "--task-output-timeout", "400" ]

CMD [ "--app-description", "Override this value", "perps-companion" ]
