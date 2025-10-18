FROM alpine
RUN apk add --no-cache curl transmission transmission-cli transmission-daemon shadow su-exec
ADD entrypoint.sh /usr/bin/entrypoint.sh

ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ENV UID=1000
ENV GID=1000
ENV RPC_PORT=9091
ENV PEER_PORT=51413
ENV USERNAME=username
ENV PASSWORD=password

CMD ["/bin/sh", "-c",  "usr/bin/transmission-daemon --foreground --config-dir /data/.config --port $RPC_PORT --peerport $PEER_PORT --username $USERNAME --password $PASSWORD"]
ENTRYPOINT [ "/usr/bin/entrypoint.sh" ]
