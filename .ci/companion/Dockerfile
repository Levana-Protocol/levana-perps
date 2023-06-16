FROM alpine:3.14.2

RUN apk update && apk add fontconfig

ADD https://github.com/fpco/pid1/releases/download/v0.1.3.1/pid1 /usr/bin/pid1

RUN chmod +x /usr/bin/pid1

ADD https://s3.ap-northeast-2.amazonaws.com/static.meteors.levana.finance/PublicSans-Regular.ttf /usr/share/fonts/PublicSans-Regular.ttf

# Force re-generation of font information cache files
RUN fc-cache -fv

COPY perps-companion /usr/bin/perps-companion

ENTRYPOINT [ "pid1" ]

CMD [ "perps-companion" ]
