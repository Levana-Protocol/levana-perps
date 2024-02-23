FROM alpine:3.19.1

RUN apk update && apk add fontconfig

ADD https://static.levana.finance/PublicSans-Regular.ttf /usr/share/fonts/PublicSans-Regular.ttf

# Force re-generation of font information cache files
RUN fc-cache -fv

COPY health-check /usr/bin
COPY perps-companion /usr/bin/perps-companion

RUN chmod 755 /usr/bin/health-check

ENTRYPOINT [ "health-check", "--task-output-timeout", "400", "--app-description", "companion" ]

CMD [ "perps-companion" ]
