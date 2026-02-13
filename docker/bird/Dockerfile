FROM alpine:3.20
RUN apk add --no-cache bird socat
RUN mkdir -p /run/bird
COPY bird.conf /etc/bird/bird.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
CMD ["/entrypoint.sh"]
