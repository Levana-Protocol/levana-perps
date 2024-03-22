FROM ubuntu:24.04

RUN apt update -y
RUN apt install wget file -y

RUN wget -q https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb
RUN apt-get install -y ./google-chrome-stable_current_amd64.deb

USER 1000

COPY --chmod=755 health-check /usr/bin/health-check
COPY --chmod=755 --chown=1000 perps-market-params /usr/bin/perps-market-params

ENTRYPOINT [ "/usr/bin/health-check", "--task-output-timeout", "90000", "--app-description", "Market Analyzer", "--app-environment", "K8s Mainnet", "perps-market-params" ]

CMD [ "serve" ]
