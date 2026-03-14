FROM alpine:3.19
RUN apk add --no-cache bash curl jq
COPY generate.sh /generate.sh
RUN chmod +x /generate.sh
CMD ["/generate.sh"]
