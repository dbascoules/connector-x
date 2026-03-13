# Informix

## Informix Connection

ConnectorX connects to Informix via **DRDA** (port 9089) using the IBM CLI Driver (`libdb2`).

```py
import connectorx as cx

conn = "informix://username:password@server:9089/database"
query = "SELECT * FROM table"
cx.read_sql(conn, query)
```

## Development Setup (Docker, no testcontainers)

Rust integration tests require the **IBM CLI Driver** (`libdb2.so`) at link time.
The driver is publicly available from IBM and does **not** require an account.

### 1. Download the IBM CLI Driver

```bash
# Linux x86_64 (used for cross-compilation from macOS arm64)
mkdir -p .libdb2-x86_64/clidriver-full
curl -L https://public.dhe.ibm.com/ibmdl/export/pub/software/data/db2/drivers/odbc_cli/linuxx64_odbc_cli.tar.gz \
  | tar -xz -C .libdb2-x86_64/clidriver-full --strip-components=1
```

Expected layout after extraction:

```
.libdb2-x86_64/clidriver-full/
  lib/
    libdb2.so
    libdb2.so.1
    ...
  include/
  ...
```

### 2. Start the Informix server container

Pull and start the IBM Informix developer image:

```bash
docker pull --platform linux/amd64 icr.io/informix/informix-developer-database:latest

docker run -d --name informix -h informix --privileged --platform linux/amd64 \
  -e LICENSE=accept -e DBSERVERNAME=informix \
  -p 9088:9088 -p 9089:9089 -p 27017:27017 -p 27018:27018 -p 27883:27883 \
  icr.io/informix/informix-developer-database:latest
```

### macOS arm64 (Apple Silicon) notes

The Informix developer image is amd64-only. On Apple Silicon, running it via emulation can fail during bootstrap with messages such as:

- `sudo: PAM account management error`
- `Bad DBSERVERNAME`
- `KAIO initialization failed`

If this happens, use the following workaround.

1. Create a small `sudo` shim:

```bash
cat > /tmp/informix-sudo-shim <<'EOF'
#!/bin/bash
while [[ "$1" == "-n" ]]; do shift; done
if [[ "$1" == "-u" ]]; then
  shift 2
fi
exec "$@"
EOF
chmod +x /tmp/informix-sudo-shim
```

1. Build a derived image that replaces `/usr/bin/sudo` with the shim:

```bash
mkdir -p /tmp/informix-arm64-workaround
cp /tmp/informix-sudo-shim /tmp/informix-arm64-workaround/sudo-shim

cat > /tmp/informix-arm64-workaround/Dockerfile <<'EOF'
FROM --platform=linux/amd64 icr.io/informix/informix-developer-database:latest
USER root
COPY sudo-shim /usr/local/bin/sudo
RUN chmod +x /usr/local/bin/sudo && mv /usr/bin/sudo /usr/bin/sudo.orig && ln -sf /usr/local/bin/sudo /usr/bin/sudo
USER informix
EOF

docker build --platform linux/amd64 -t informix-arm64-workaround:latest /tmp/informix-arm64-workaround
```

1. Start Informix from the derived image with explicit server name:

```bash
docker rm -f -v informix 2>/dev/null || true

docker run -d --name informix -h informix --privileged --platform linux/amd64 \
  -e LICENSE=accept -e DBSERVERNAME=informix \
  -p 9088:9088 -p 9089:9089 -p 27017:27017 -p 27018:27018 -p 27883:27883 \
  informix-arm64-workaround:latest
```

1. Disable AIO/direct I/O for emulation and restart once:

```bash
docker exec informix bash -lc "cat > /opt/ibm/config/onconfig.mod <<'EOF'
DIRECT_IO 0
AUTO_AIOVPS 0
AUTO_TUNE 0
EOF"

docker restart informix
```

1. Verify database engine status:

```bash
docker inspect --format 'status={{.State.Status}} health={{.State.Health.Status}}' informix
docker exec informix bash -lc 'onstat - | head -n 5'
```

Expected state: `health=healthy` and Informix `On-Line`.

### 3. Seed the test database

```bash
just seed-db-informix informix connectorx
```

### 4. Run Informix integration tests (Apple Silicon — recommended)

On macOS arm64, cross-compile the test binary for `x86_64-unknown-linux-gnu` then run it inside a Docker container:

```bash
just test-informix
```

This recipe:
1. Cross-compiles `test_informix` for `linux/amd64` using `zig-cc-x86` as linker and `IBM_DB_HOME` pointing at `.libdb2-x86_64/clidriver-full/`
2. Finds the freshly compiled binary under `target/x86_64-unknown-linux-gnu/debug/deps/`
3. Detects the running `informix` container IP
4. Mounts the binary + CLI Driver libraries into an `ubuntu:22.04` container and runs the tests

Prerequisites:
- `zig-cc-x86` wrapper available (`brew install zig` + script in PATH)
- `.libdb2-x86_64/clidriver-full/lib/libdb2.so` present (see step 1)
- `informix` Docker container running and healthy

### Run tests natively (Linux x86_64 only)

On a native Linux x86_64 host with `IBM_DB_HOME` pointing to the CLI Driver:

```bash
export IBM_DB_HOME="/path/to/clidriver"
export LD_LIBRARY_PATH="$IBM_DB_HOME/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export INFORMIX_URL="informix://informix:in4mix@127.0.0.1:9089/connectorx"

cargo test -p connectorx --no-default-features --features fptr,dst_arrow,src_informix \
  --test test_informix -- --ignored --nocapture
```

### Troubleshooting: `ld: library 'db2' not found`

`IBM_DB_HOME` must be set to an **absolute path** containing `lib/libdb2.so`.
A relative path causes the `build.rs` discovery to fail silently.

```bash
export IBM_DB_HOME="$(pwd)/.libdb2-x86_64/clidriver-full"
ls "$IBM_DB_HOME/lib/libdb2.so"
```

### Recommended on Apple Silicon: amd64 devcontainer

If you prefer a persistent devcontainer environment instead of the cross-compile workflow:

1. Set the CLI Driver download URL (public, no account required):

```bash
export INFORMIX_CLIDRIVER_URL="https://public.dhe.ibm.com/ibmdl/export/pub/software/data/db2/drivers/odbc_cli/linuxx64_odbc_cli.tar.gz"
```

1. Reopen the workspace in the devcontainer (pinned `linux/amd64` in `.devcontainer/docker-compose.yml`). The devcontainer image downloads the CLI Driver and sets `IBM_DB_HOME=/opt/ibm/clidriver` automatically.

1. Start the Informix service profile:

```bash
docker compose -f .devcontainer/docker-compose.yml --profile informix up -d informix
```

1. Seed Informix test data:

```bash
just seed-db-informix-devcontainer db=connectorx
```

1. Run Informix integration tests inside the devcontainer:

```bash
cargo test -p connectorx --no-default-features --features fptr,dst_arrow,src_informix \
  --test test_informix -- --ignored --nocapture
```

The devcontainer sets `INFORMIX_URL=informix://informix:in4mix@informix:9089/connectorx` automatically via `remoteEnv`.

If `INFORMIX_CLIDRIVER_URL` is not set at build time, the devcontainer image builds without the CLI Driver and `cargo test --features src_informix` will fail at link time.

If you use `--name ifx -h ifx` for the server container, pass `container=ifx` to `just seed-db-informix` and adapt `INFORMIX_URL` to point at `ifx:9089`.
