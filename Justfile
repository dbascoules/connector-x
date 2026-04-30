set dotenv-load := true

build-release:
    cargo build --release --features all

build-debug:
    cargo build --features all

build-cpp +ARGS="":
    cd connectorx-cpp && cargo build {{ARGS}}

build-cpp-release +ARGS="":
    cd connectorx-cpp && cargo build --release {{ARGS}}

test +ARGS="": 
    cargo test --features all {{ARGS}} -- --nocapture

test-ci: 
    cargo test --features src_postgres --features dst_arrow --test test_postgres
    cargo test --features src_postgres --features src_dummy --features dst_polars --test test_polars

test-feature-gate:
    cargo c --features src_postgres
    cargo c --features src_mysql
    cargo c --features src_mssql
    cargo c --features src_sqlite
    cargo c --features src_oracle
    cargo c --features src_trino
    cargo c --features src_clickhouse
    cargo c --features dst_arrow

cleanup:
    cargo clean
    cd connectorx-python && cargo clean
    rm connectorx-python/connectorx/connectorx*.so

bootstrap-python:
    cd connectorx-python && poetry install

setup-java:
    cd $ACCIO_PATH/rewriter && mvn package -Dmaven.test.skip=true
    cp -f $ACCIO_PATH/rewriter/target/accio-rewriter-1.0-SNAPSHOT-jar-with-dependencies.jar connectorx-python/connectorx/dependencies/federated-rewriter.jar

setup-python:
    cd connectorx-python && poetry run maturin develop --release
    
test-python +opts="": setup-python
    cd connectorx-python && poetry run pytest connectorx/tests -v -s {{opts}}

test-python-s +opts="":
    cd connectorx-python && poetry run pytest connectorx/tests -v -s {{opts}}

seed-db:
    #!/bin/bash
    psql $POSTGRES_URL -f scripts/postgres.sql
    sqlite3 ${SQLITE_URL#sqlite://} < scripts/sqlite.sql
    mysql --protocol tcp -h$MYSQL_HOST -P$MYSQL_PORT -u$MYSQL_USER -p$MYSQL_PASSWORD $MYSQL_DB < scripts/mysql.sql

# dbs not included in ci
seed-db-more:
    mssql-cli -S$MSSQL_HOST -U$MSSQL_USER -P$MSSQL_PASSWORD -d$MSSQL_DB -i scripts/mssql.sql
    psql $REDSHIFT_URL -f scripts/redshift.sql
    ORACLE_URL_SCRIPT=`echo ${ORACLE_URL#oracle://} | sed "s/:/\//"`
    cat scripts/oracle.sql | sqlplus $ORACLE_URL_SCRIPT
    mysql --protocol tcp -h$MARIADB_HOST -P$MARIADB_PORT -u$MARIADB_USER -p$MARIADB_PASSWORD $MARIADB_DB < scripts/mysql.sql
    trino $TRINO_URL --catalog=$TRINO_CATALOG < scripts/trino.sql
    clickhouse-client -h $CLICKHOUSE_HOST --port $CLICKHOUSE_PORT -u $CLICKHOUSE_USER --password $CLICKHOUSE_PASSWORD -d $CLICKHOUSE_DB < scripts/clickhouse.sql

seed-db-informix container="informix" db="connectorx":
    docker exec -i {{container}} bash -lc "printf 'CREATE DATABASE {{db}} WITH LOG;' | dbaccess sysmaster -"
    docker exec -i {{container}} bash -lc "dbaccess {{db}} -" < scripts/informix.sql

seed-db-informix-devcontainer db="connectorx":
    docker compose -f .devcontainer/docker-compose.yml --profile informix exec -T -e INFORMIXSERVER=informix informix bash -lc "printf 'CREATE DATABASE {{db}} WITH LOG;' | dbaccess sysmaster -"
    docker compose -f .devcontainer/docker-compose.yml --profile informix exec -T -e INFORMIXSERVER=informix informix bash -lc "dbaccess {{db}} -" < scripts/informix.sql

# Cross-compile les tests Informix pour linux/amd64 puis les exécute dans Docker.
# Prérequis : .libdb2-x86_64/clidriver-full/ (IBM CLI Driver) et conteneur informix démarré.
test-informix:
    #!/usr/bin/env bash
    set -euo pipefail
    CLIDIR="$(pwd)/.libdb2-x86_64"
    echo "=== Compilation cross (x86_64-unknown-linux-gnu) ==="
    IBM_DB_HOME="${CLIDIR}/clidriver-full" \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=zig-cc-x86 \
      cargo test --target x86_64-unknown-linux-gnu \
        -p connectorx --features src_informix,dst_arrow \
        --test test_informix --no-run
    BINARY="$(find target/x86_64-unknown-linux-gnu/debug/deps -maxdepth 1 -name 'test_informix-*' ! -name '*.d' | xargs ls -t 2>/dev/null | head -1)"
    if [[ -z "$BINARY" ]]; then echo "ERROR: binaire de test introuvable" >&2; exit 1; fi
    echo "=== Binaire : $BINARY ==="
    INFORMIX_IP="$(docker inspect --format '{{{{range .NetworkSettings.Networks}}}}{{{{.IPAddress}}}}{{{{end}}}}' informix 2>/dev/null || echo 172.17.0.2)"
    echo "=== Exécution dans Docker (informix=$INFORMIX_IP) ==="
    docker run --rm --platform linux/amd64 \
      --add-host "informix:${INFORMIX_IP}" \
      -v "${BINARY}:/test_informix:ro" \
      -v "${CLIDIR}/clidriver-full/lib:/clidriver/lib:ro" \
      -v "${CLIDIR}/ibm_db.libs:/ibm_db_libs:ro" \
      ubuntu:22.04 \
      bash -c "LD_LIBRARY_PATH=/clidriver/lib:/ibm_db_libs INFORMIX_URL=informix://informix:in4mix@informix:9089/connectorx /test_informix --include-ignored --nocapture"

# benches 
flame-tpch conn="POSTGRES_URL":
    cd connectorx-python && PYO3_PYTHON=$HOME/.pyenv/versions/3.8.6/bin/python3.8 PYTHONPATH=$HOME/.pyenv/versions/conn/lib/python3.8/site-packages LD_LIBRARY_PATH=$HOME/.pyenv/versions/3.8.6/lib/ cargo run --no-default-features --features executable --features fptr --features nbstr --features dsts --features srcs --release --example flame_tpch {{conn}}

build-tpch:
    cd connectorx-python && cargo build --no-default-features --features executable --features fptr --release --example tpch

cachegrind-tpch: build-tpch
    valgrind --tool=cachegrind target/release/examples/tpch

python-tpch name +ARGS="": setup-python
    #!/bin/bash
    export PYTHONPATH=$PWD/connectorx-python
    cd connectorx-python && \
    poetry run python ../benchmarks/tpch-{{name}}.py {{ARGS}}

python-tpch-ext name +ARGS="":
    cd connectorx-python && poetry run python ../benchmarks/tpch-{{name}}.py {{ARGS}}

python-ddos name +ARGS="": setup-python
    #!/bin/bash
    export PYTHONPATH=$PWD/connectorx-python
    cd connectorx-python && \
    poetry run python ../benchmarks/ddos-{{name}}.py {{ARGS}}

python-ddos-ext name +ARGS="":
    cd connectorx-python && poetry run python ../benchmarks/ddos-{{name}}.py {{ARGS}}


python-shell:
    cd connectorx-python && \
    poetry run ipython

benchmark-report: setup-python
    cd connectorx-python && \
    poetry run pytest connectorx/tests/benchmarks.py --benchmark-json ../benchmark.json
    
# releases
build-python-wheel:
    cd connectorx-python && maturin build --release -i python

# release with federation enabled
build-python-wheel-fed:
    # need to get the j4rs dependency first
    cd connectorx-python && maturin build --release -i python
    # copy files
    cp -rf connectorx-python/target/release/jassets connectorx-python/connectorx/dependencies
    # build final wheel
    cd connectorx-python && maturin build --release -i python