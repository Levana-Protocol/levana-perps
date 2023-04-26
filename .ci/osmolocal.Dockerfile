FROM ghcr.io/levana-protocol/levana-perps/localosmosis:size

WORKDIR /osmosis

ADD ./setup.sh /osmosis/setup.sh

ENTRYPOINT [ "/osmosis/setup.sh" ]
