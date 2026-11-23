FROM nginxinc/nginx-unprivileged:stable-alpine@sha256:f5116ed4606f3eb015dd4a8e28cb9b351476dc0ad4bc6e9f93b19cec130cdf03

ARG WEB_VERSION
ARG WEB_SOURCE_COMMIT
ARG CONTAINER_COMMIT
LABEL org.opencontainers.image.title="Hidrosentinel web preview" \
      org.opencontainers.image.description="Frontend preview; live API is proxied to flood.transbulent.id. Not a standalone backend or an official flood warning service." \
      org.opencontainers.image.source="https://github.com/gisrad/flood-map-in-rust" \
      org.opencontainers.image.version="${WEB_VERSION}" \
      org.opencontainers.image.revision="${WEB_SOURCE_COMMIT}" \
      id.transbulent.hidrosentinel.container-revision="${CONTAINER_COMMIT}"

# Keep the complete verified web archive, including data and dependency notices.
COPY release/ /opt/hidrosentinel-release/
COPY default.conf /etc/nginx/conf.d/default.conf
USER 101:101
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s CMD wget -q -O /dev/null http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["nginx"]
CMD ["-g", "daemon off;"]
