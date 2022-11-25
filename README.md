# mongodb-scraper

[![Conventional Commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-yellow.svg)](https://conventionalcommits.org)

## Specifications

### MongoDB

This service will query the configured database and collection at regular intervals.

### InfluxDB

After having obtained documents from MongoDB, this service will send data points to InfluxDB (line protocol), one for each document in the collection.

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
🚧
```
