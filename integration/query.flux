import "array"
import "testing"

import "influxdata/influxdb/schema"

check = (r) =>
    r.integer >= 200 and
    r.integer <= 300 and
    r.float >= 10.0 and
    r.float <= 25.0 and
    r.firstAge >= 1 and
    not exists r.secondAge and
    r.thirdAge >= 1

want = array.from(rows: [{ satisfies: 4 }])

from(bucket: "integration_tests_bucket")
    |> range(start: -10s)
    |> schema.fieldsAsCols()
    // Tests start below
    |> map(fn: (r) => ({ satisfies: check(r: r) }))
    |> count(column: "satisfies")
    |> testing.assertEquals(name: "assertion", want: want)
