FROM mcr.microsoft.com/devcontainers/rust:1

ARG INFORMIX_CLIDRIVER_URL=""
ARG INFORMIX_CLIDRIVER_SHA256=""


# Install system dependencies
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
    sqlite3 \
    pkg-config \
    libclang-dev \
    postgresql-client \
    default-mysql-client \
    unzip \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Optional: download an IBM CLI Driver archive and install it under /opt/ibm/clidriver.
RUN mkdir -p /opt/ibm/clidriver \
    && if [ -n "${INFORMIX_CLIDRIVER_URL}" ]; then \
        tmpdir="$(mktemp -d)" \
        && archive_path="$tmpdir/clidriver-archive" \
        && extract_dir="$tmpdir/extract" \
        && mkdir -p "$extract_dir" \
        && echo "Downloading IBM CLI Driver from ${INFORMIX_CLIDRIVER_URL}" \
        && curl -fsSL "${INFORMIX_CLIDRIVER_URL}" -o "$archive_path" \
        && if [ -n "${INFORMIX_CLIDRIVER_SHA256}" ]; then \
            echo "${INFORMIX_CLIDRIVER_SHA256}  ${archive_path}" | sha256sum -c -; \
        fi \
        && case "${INFORMIX_CLIDRIVER_URL}" in \
            *.zip) unzip -q "$archive_path" -d "$extract_dir" ;; \
            *.tar.gz|*.tgz) tar -xzf "$archive_path" -C "$extract_dir" ;; \
            *.tar.Z) tar -xZf "$archive_path" -C "$extract_dir" ;; \
            *.tar) tar -xf "$archive_path" -C "$extract_dir" ;; \
            *) tar -xf "$archive_path" -C "$extract_dir" || unzip -q "$archive_path" -d "$extract_dir" ;; \
        esac \
        && libdb2_path="$(find "$extract_dir" -type f -path '*/lib/libdb2.so*' -print -quit)" \
        && if [ -z "$libdb2_path" ]; then \
            echo "Could not locate libdb2 under extracted archive (expected path like */lib/libdb2.so*)" >&2; \
            exit 1; \
        fi \
        && clidriver_root="$(dirname "$(dirname "$libdb2_path")")" \
        && cp -a "$clidriver_root"/. /opt/ibm/clidriver/ \
        && if [ ! -f /opt/ibm/clidriver/lib/libdb2.so ]; then \
            so_candidate="$(find /opt/ibm/clidriver/lib -maxdepth 1 -type f -name 'libdb2.so*' -print -quit)" \
            && if [ -n "$so_candidate" ]; then \
                ln -s "$(basename "$so_candidate")" /opt/ibm/clidriver/lib/libdb2.so; \
            fi; \
        fi \
        && test -f /opt/ibm/clidriver/lib/libdb2.so \
        && rm -rf "$tmpdir"; \
    else \
        echo "INFORMIX_CLIDRIVER_URL is not set; skip CLI Driver auto-download"; \
    fi \
    && chown -R vscode:vscode /opt/ibm/clidriver

# Install Python and Poetry
ENV POETRY_HOME=/opt/poetry
ENV POETRY_VERSION=2.1.3
ENV PATH="/opt/poetry/bin:$PATH"
RUN curl -sSL https://install.python-poetry.org | python3 - \
    && poetry config virtualenvs.create false

# Install Rust components
RUN rustup component add rustfmt clippy

# Set environment variables
ENV PATH="/home/vscode/.local/bin:${PATH}"
ENV IBM_DB_HOME="/opt/ibm/clidriver"
ENV LD_LIBRARY_PATH="/opt/ibm/clidriver/lib:${LD_LIBRARY_PATH}"
ENV PYTHONPATH="/workspaces/connector-x:${PYTHONPATH}"

# Adjust UID/GID for macOS Colima compatibility
ARG USERNAME=vscode
ARG GROUPNAME=vscode
ARG USER_UID=1000
ARG USER_GID=1000
RUN groupmod --gid $USER_GID -o $GROUPNAME \
    && usermod --uid $USER_UID --gid $USER_GID $USERNAME \
    && chown -R $USER_UID:$USER_GID /home/$USERNAME
