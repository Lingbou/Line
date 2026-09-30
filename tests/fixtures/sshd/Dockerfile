FROM alpine:latest

RUN apk add --no-cache openssh \
    && ssh-keygen -A

EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
