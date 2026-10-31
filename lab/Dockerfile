# Reproducible lab image. Build from the repo root:
#   docker build -f lab/Dockerfile -t mininet-lab .
#   docker run --rm -it -v "$PWD":/work -w /work mininet-lab lab/doctor.sh
# Nothing needs --privileged. lab/netlab uses rootless user namespaces, which
# some container runtimes block (then run it on the host as a normal user).
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive PATH=/root/.cargo/bin:$PATH
COPY lab/bootstrap.sh /tmp/bootstrap.sh
COPY rust-toolchain.toml /work/rust-toolchain.toml
WORKDIR /work
RUN apt-get update -qq && apt-get install -y -qq sudo curl ca-certificates \
 && sed 's#cd "$(dirname "${BASH_SOURCE\[0\]}")/.."#cd /work#' /tmp/bootstrap.sh > /tmp/b.sh \
 && bash /tmp/b.sh
