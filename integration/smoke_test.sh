#!/usr/bin/env bash

me="$0"
log_file=

teardown () {
    if [ "$log_file" ]; then
        docker compose stop
        docker compose logs --timestamps > "$log_file"
    fi
    docker compose down --volumes
}

die () {
    echo "$1" >&2
    teardown
    exit 1
}

while :; do
    case $1 in
        -h|--help)
            echo "Usage: $me [--log-file path]"
            exit 2
            ;;
        --log-file)
            if [ "$2" ]; then
                if touch "$2"; then
                    log_file=$2
                    shift
                else
                    die "log file error"
                fi
            else
                die '"--log-file" requires a non-empty option argument'
            fi
            ;;
        *)
            break
    esac
done

set -eux

# Build services images
docker compose build

# Start services
docker compose up -d --quiet-pull


# Wait for mongodb-scraper to be healthy
max_attempts=6
wait_success=
for i in $(seq 1 $max_attempts); do
    if docker compose exec mongodb-scraper /usr/local/bin/healthcheck; then
        wait_success="true"
        break
    fi
    echo "Waiting for mongodb-scraper to be healthy: try #$i failed" >&2
    [[ $i != "$max_attempts" ]] && sleep 5
done
if [ "$wait_success" != "true" ]; then
    die "Failure waiting for mongodb-scraper to be healthy"
fi

# Feed MongoDB with data
docker compose exec mongodb mongosh --quiet --norc /usr/src/push-data.mongodb

# Show the data that will be tested
awk '/\/\/ Tests start below/ {exit} {print}' query.flux \
| docker compose exec -T influxdb influx query -

# Run the tests with influxdb CLI
docker compose exec -T influxdb influx query - < query.flux

echo "🎉 success"
teardown
