# mongodb-scraper

[![Conventional Commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-yellow.svg)](https://conventionalcommits.org)

## Specifications

### MongoDB

This service will query the configured database and collection at regular intervals.

### InfluxDB

After having obtained documents from MongoDB, this service will send data points to InfluxDB (line protocol), one for each document in the collection. The measurement will be set to the namespace of the collection.

Data points will have following characteristics:

- document primary key (`_id`) as a tag value, with `id` as the tag key;
- each key and value pair in the `val` sub-document, respectively as field key and value;
- each key and value pair in the `ts` sub-document, respectively as field key (appended with `Age`) and value;

## Data flow

```mermaid
sequenceDiagram
    participant MongoDB
    participant Me as This service
    participant TSDB
    critical
        Me->>+MongoDB: Establish connection
        MongoDB-->>-Me: Connection established
    end
    loop Each collect interval
        Me->>+MongoDB: Query documents
        MongoDB-->>-Me: Replies with documents
        activate Me
        Me-)TSDB: Sends data points
        deactivate Me
    end
```

## Usage

```ShellSession
$ mongodb-scraper --help
Usage: mongodb-scraper [OPTIONS] --mongodb-database <MONGODB_DATABASE> --mongodb-collection <MONGODB_COLLECTION> --influxdb-api-token <INFLUXDB_API_TOKEN> --influxdb-org <INFLUXDB_ORG> --influxdb-bucket <INFLUXDB_BUCKET>

Options:
      --interval <INTERVAL>
          Scraping interval [env: INTERVAL=] [default: 1m]
      --mongodb-uri <MONGODB_URI>
          URI of MongoDB server [env: MONGODB_URI=] [default: mongodb://mongodb]
      --mongodb-database <MONGODB_DATABASE>
          MongoDB database [env: MONGODB_DATABASE=]
      --mongodb-collection <MONGODB_COLLECTION>
          MongoDB collection [env: MONGODB_COLLECTION=]
      --influxdb-url <INFLUXDB_URL>
          InfluxDB root URL [env: INFLUXDB_URL=] [default: http://influxdb:8086]
      --influxdb-api-token <INFLUXDB_API_TOKEN>
          InfluxDB API token with write-buckets permission [env: INFLUXDB_API_TOKEN=]
      --influxdb-org <INFLUXDB_ORG>
          InfluxDB organization name or ID [env: INFLUXDB_ORG=]
      --influxdb-bucket <INFLUXDB_BUCKET>
          InfluxDB bucket to write to [env: INFLUXDB_BUCKET=]
  -v, --verbose...
          More output per occurrence
  -q, --quiet...
          Less output per occurrence
  -h, --help
          Print help
```
