FROM alpine:3.19.1

RUN apk upgrade
RUN apk add chromium

COPY --chmod=755 health-check /usr/bin/health-check
COPY --chmod=755 perps-market-params /usr/bin/perps-market-params

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Market Analyzer", "--app-environment", "K8s Mainnet", "perps-market-params" ]

CMD [ "serve" ]
