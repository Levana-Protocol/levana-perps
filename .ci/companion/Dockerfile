FROM alpine:3.14.2

RUN apk update && apk add fontconfig

ADD https://github.com/fpco/pid1/releases/download/v0.1.3.1/pid1 /usr/bin/pid1

RUN chmod +x /usr/bin/pid1

ADD https://static.levana.finance/PublicSans-Regular.ttf /usr/share/fonts/PublicSans-Regular.ttf

# Force re-generation of font information cache files
RUN fc-cache -fv

COPY perps-companion /usr/bin/perps-companion

ADD https://raw.githubusercontent.com/Levana-Protocol/docker-images/health_check/images/base_image/health_check.sh /health_check.sh
ADD https://raw.githubusercontent.com/Levana-Protocol/docker-images/health_check/images/base_image/logging.sh /logging.sh
RUN apk add --no-cache curl bash aws-cli && chmod a+x /health_check.sh /logging.sh

ENTRYPOINT [ "pid1", "/logging.sh" ]

CMD [ "perps-companion" ]
