FROM alpine:3.21

ADD --chmod=755 https://github.com/fpco/health-check/releases/download/v0.7.0/health-check-aarch64-unknown-linux-musl /usr/bin/health-check

RUN apk update && apk add fontconfig

ADD --chmod=444 https://static.levana.finance/PublicSans-Regular.ttf /usr/share/fonts/PublicSans-Regular.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/BarlowSemiCondensed-Medium.ttf /usr/share/fonts/BarlowSemiCondensed-Medium.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/BarlowSemiCondensed-Regular.ttf /usr/share/fonts/BarlowSemiCondensed-Regular.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/BarlowSemiCondensed-SemiBold.ttf /usr/share/fonts/BarlowSemiCondensed-SemiBold.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/Montserrat-Bold.ttf /usr/share/fonts/Montserrat-Bold.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/Montserrat-ExtraBold.ttf /usr/share/fonts/Montserrat-ExtraBold.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/Montserrat-Regular.ttf /usr/share/fonts/Montserrat-Regular.ttf
ADD --chmod=444 https://static.rujiperps.com/fonts/Montserrat-SemiBold.ttf /usr/share/fonts/Montserrat-SemiBold.ttf

# Force re-generation of font information cache files
RUN fc-cache -fv

WORKDIR /app

RUN mkdir -p /app/packages/perps-exes/assets

ENV NO_COLOR=1

COPY --chmod=755 ./mainnet-factories.toml /app/packages/perps-exes/assets
COPY perps-companion /usr/bin/perps-companion

ENTRYPOINT [ "health-check", "--task-output-timeout", "400" ]

CMD [ "--app-description", "Override this value", "perps-companion" ]
