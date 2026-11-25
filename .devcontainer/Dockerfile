FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1

ARG USERNAME=vscode
ARG USER_UID=1000
ARG USER_GID=1000

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        git \
        g++-mingw-w64-x86-64 \
        libssl-dev \
        libwayland-dev \
        libx11-dev \
        libx11-xcb-dev \
        libxcb-cursor-dev \
        libxcb-icccm4-dev \
        libxcb-keysyms1-dev \
        libxcb-randr0-dev \
        libxcb-render-util0-dev \
        libxcb-shape0-dev \
        libxcb-xfixes0-dev \
        libxcb-xinerama0-dev \
        libxcb-xkb-dev \
        libxkbcommon-dev \
        libxkbcommon-x11-dev \
        nasm \
        pkg-config \
        python3 \
        sudo \
    && groupadd --gid "${USER_GID}" "${USERNAME}" \
    && useradd --uid "${USER_UID}" --gid "${USER_GID}" --create-home --shell /bin/bash "${USERNAME}" \
    && echo "${USERNAME} ALL=(root) NOPASSWD:ALL" > "/etc/sudoers.d/${USERNAME}" \
    && chmod 0440 "/etc/sudoers.d/${USERNAME}" \
    && mkdir -p /home/vscode/.cargo /home/vscode/.cache/mininet-target \
    && chown -R "${USERNAME}:${USERNAME}" /home/vscode/.cargo /home/vscode/.cache/mininet-target \
    && printf 'export PATH="/usr/local/cargo/bin:$PATH"\n' > /etc/profile.d/rust-toolchain.sh \
    && rustup component add clippy rustfmt \
    && rustup target add wasm32-wasip2 x86_64-pc-windows-gnu \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_HOME=/home/vscode/.cargo \
    CARGO_TARGET_DIR=/home/vscode/.cache/mininet-target \
    PATH=/usr/local/cargo/bin:${PATH}

RUN git config --system --add safe.directory /workspaces/Mininet

USER ${USERNAME}
WORKDIR /workspaces/Mininet
