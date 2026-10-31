# Reproducible lab image. Build from the repo root:
#   docker build -f lab/Dockerfile -t mininet-lab .
# Run against an EXPORT of the code, read-only and as yourself, never a
# read/write bind of a checkout (a container must not be able to plant files that
# the host later executes, e.g. via .git/config):
#   X=$(mktemp -d) && git archive HEAD | tar -x -C "$X"
#   docker run --rm --user "$(id -u):$(id -g)" -v "$X":/work:ro -w /work \
#       -e CARGO_HOME=/tmp/cargo -e CARGO_TARGET_DIR=/tmp/target mininet-lab lab/doctor.sh
# Nothing needs --privileged. lab/netlab uses rootless user namespaces, which
# some container runtimes block (then run it on the host as a normal user).
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
COPY lab/bootstrap.sh /tmp/bootstrap.sh
COPY rust-toolchain.toml /work/rust-toolchain.toml
WORKDIR /work
RUN apt-get update -qq && apt-get install -y -qq sudo curl ca-certificates \
 && sed 's#cd "$(dirname "${BASH_SOURCE\[0\]}")/.."#cd /work#' /tmp/bootstrap.sh > /tmp/b.sh \
 && bash /tmp/b.sh \
 && chmod -R a+rX /opt/rustup /opt/cargo
# Unprivileged by default; override with --user to match your host uid/gid.
RUN useradd --create-home --uid 1000 lab
USER lab
