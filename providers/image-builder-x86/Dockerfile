# Local proof runtime only; no host package installation or device passthrough.
FROM ubuntu@sha256:4fbb8e6a8395de5a7550b33509421a2bafbc0aab6c06ba2cef9ebffbc7092d90
ENV DEBIAN_FRONTEND=noninteractive
# APT authenticates repository metadata and package digests before execution.
# Every added/changed package, including transitive dependencies, is version
# pinned. Unchanged base packages are covered by the immutable base digest.
COPY runtime-packages.lock /runtime-packages.lock
RUN apt-get update \
    && tr -d '\r' < /runtime-packages.lock > /runtime-packages.linux \
    && xargs -r -a /runtime-packages.linux apt-get install -y --no-install-recommends \
    && dpkg-query -W > /runtime-packages.tsv \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /proof
USER 1000:1000
ENTRYPOINT ["python3", "/proof/builder.py"]
