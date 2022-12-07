import "array"
import "testing"

check = (r) =>
    r.integer >= 200 and
    r.integer <= 300 and
    r.float >= 10.0 and
    r.float <= 25.0

want = array.from(rows: [{ satisfies: 4 }])

from(bucket: "integration_tests_bucket")
    |> range(start: -10s)
    |> pivot(rowKey: ["_time"], columnKey: ["_field"], valueColumn: "_value")
    // Tests start below
    |> map(fn: (r) => ({ satisfies: if check(r: r) then true else false }))
    |> count(column: "satisfies")
    |> testing.assertEquals(name: "assertion", want: want)
