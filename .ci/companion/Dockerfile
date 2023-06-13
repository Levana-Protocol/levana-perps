FROM alpine:3.14.2

# Install necessary fonts
RUN apk --no-cache add msttcorefonts-installer fontconfig && \
    update-ms-fonts && \
    fc-cache -f

COPY perps-companion /usr/bin

CMD ["/usr/bin/perps-companion"]
