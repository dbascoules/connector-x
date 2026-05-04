import datetime
import os
from decimal import Decimal

import pandas as pd
import pytest
from pandas.testing import assert_frame_equal

from .. import ConnectionUrl, get_meta, read_sql

_SKIP = pytest.mark.skipif(
    not os.environ.get("INFORMIX_URL"),
    reason="Test Informix only when INFORMIX_URL is set",
)

_QUERY_ALL = (
    "select test_bool, test_smallint, test_int, test_bigint, test_float, test_double, "
    "test_decimal, test_char, test_varchar, test_text, test_date, test_datetime, test_nullable "
    "from test_types "
    "order by case when test_int is null then 1 else 0 end, test_int"
)


@pytest.fixture(scope="module")
def informix_url() -> str:
    return os.environ["INFORMIX_URL"]


# ---------------------------------------------------------------------------
# Pandas
# ---------------------------------------------------------------------------


@_SKIP
def test_informix_without_partition(informix_url: str) -> None:
    df = read_sql(informix_url, _QUERY_ALL)
    expected = pd.DataFrame(
        data={
            "test_bool": pd.Series([True, False, None], dtype="boolean"),
            "test_smallint": pd.Series([-32767, 32767, None], dtype="Int64"),
            "test_int": pd.Series([0, 2147483647, None], dtype="Int64"),
            "test_bigint": pd.Series(
                [9223372036854775807, -9223372036854775807, None], dtype="Int64"
            ),
            "test_float": pd.Series([-1.1, 3.14159, None], dtype="float64"),
            "test_double": pd.Series([-1.12345, 2.71828, None], dtype="float64"),
            "test_decimal": pd.Series([-1234.56, 12345.67, None], dtype="float64"),
            "test_char": pd.Series(["abc  ", "xyz  ", None], dtype="object"),
            "test_varchar": pd.Series(["varchar", "varchar2", None], dtype="object"),
            "test_text": pd.Series(
                ["informix text", "longer informix text", None], dtype="object"
            ),
            "test_date": pd.Series(
                ["1970-01-01", "9999-12-31", None], dtype="datetime64[us]"
            ),
            "test_datetime": pd.Series(
                ["1970-01-01 00:00:01.12345", "9999-12-31 23:59:59.99999", None],
                dtype="datetime64[us]",
            ),
            "test_nullable": pd.Series(["row-one", None, None], dtype="object"),
        },
    )
    assert_frame_equal(df, expected, check_names=True)


@_SKIP
def test_informix_multiple_queries(informix_url: str) -> None:
    """Two disjoint queries concatenated into one DataFrame."""
    q1 = "select test_int, test_varchar from test_types where test_int = 0"
    q2 = "select test_int, test_varchar from test_types where test_int = 2147483647"
    df = read_sql(informix_url, [q1, q2])
    assert len(df) == 2
    assert set(df["test_int"].tolist()) == {0, 2147483647}


# ---------------------------------------------------------------------------
# Arrow
# ---------------------------------------------------------------------------


@_SKIP
def test_informix_arrow_schema(informix_url: str) -> None:
    """Verify Arrow schema types for all Informix type mappings."""
    import pyarrow as pa

    table = read_sql(informix_url, _QUERY_ALL, return_type="arrow")

    schema = table.schema
    assert pa.types.is_boolean(schema.field("test_bool").type)
    assert schema.field("test_smallint").type == pa.int16()
    assert schema.field("test_int").type == pa.int32()
    assert schema.field("test_bigint").type == pa.int64()
    assert schema.field("test_float").type == pa.float32()
    assert schema.field("test_double").type == pa.float64()
    assert pa.types.is_decimal(schema.field("test_decimal").type)
    assert pa.types.is_large_unicode(schema.field("test_varchar").type)
    assert pa.types.is_large_unicode(schema.field("test_text").type)
    assert schema.field("test_date").type == pa.date32()
    # Timestamp stored as microseconds (no timezone)
    assert schema.field("test_datetime").type == pa.timestamp("us")


@_SKIP
def test_informix_arrow_values(informix_url: str) -> None:
    """Verify row counts and a selection of values via Arrow → pandas."""
    import pyarrow as pa

    table = read_sql(informix_url, _QUERY_ALL, return_type="arrow")
    assert table.num_rows == 3

    df = table.to_pandas()

    # Integers
    assert df["test_int"][0] == 0
    assert df["test_int"][1] == 2147483647
    assert df["test_int"][2] is None or pd.isna(df["test_int"][2])

    assert df["test_bigint"][0] == 9223372036854775807
    assert df["test_bigint"][1] == -9223372036854775807

    # Floating point
    assert abs(df["test_float"][0] - (-1.1)) < 1e-5
    assert abs(df["test_double"][1] - 2.71828) < 1e-9

    # Decimal (Decimal128 → Python Decimal)
    assert df["test_decimal"][0] == Decimal("-1234.5600000000")
    assert df["test_decimal"][1] == Decimal("12345.6700000000")
    assert df["test_decimal"][2] is None or pd.isna(df["test_decimal"][2])

    # Boolean
    assert df["test_bool"][0] == True   # noqa: E712
    assert df["test_bool"][1] == False  # noqa: E712

    # Date (date32 → datetime.date)
    assert df["test_date"][0] == datetime.date(1970, 1, 1)
    assert df["test_date"][1] == datetime.date(9999, 12, 31)
    assert df["test_date"][2] is None or pd.isna(df["test_date"][2])

    # Timestamp (microseconds → datetime64[us])
    assert pd.Timestamp(df["test_datetime"][0]) == pd.Timestamp("1970-01-01 00:00:01.123450")
    assert pd.Timestamp(df["test_datetime"][1]) == pd.Timestamp("9999-12-31 23:59:59.999990")
    assert df["test_datetime"][2] is None or pd.isna(df["test_datetime"][2])

    # Text
    assert df["test_varchar"][0] == "varchar"
    assert df["test_varchar"][2] is None or pd.isna(df["test_varchar"][2])
    assert df["test_nullable"][0] == "row-one"
    assert df["test_nullable"][1] is None or pd.isna(df["test_nullable"][1])


@_SKIP
def test_informix_arrow_stream(informix_url: str) -> None:
    """Arrow streaming reader yields the correct rows."""
    import pyarrow as pa

    reader = read_sql(informix_url, _QUERY_ALL, return_type="arrow_stream", batch_size=2)
    batches = list(reader)
    table = pa.Table.from_batches(batches)

    assert table.num_rows == 3

    # Schema checks (same as batch path)
    assert table.schema.field("test_int").type == pa.int32()
    assert table.schema.field("test_datetime").type == pa.timestamp("us")
    assert pa.types.is_decimal(table.schema.field("test_decimal").type)

    df = table.to_pandas()
    assert set(df["test_varchar"].dropna().tolist()) == {"varchar", "varchar2"}
    assert df["test_bigint"][0] == 9223372036854775807


# ---------------------------------------------------------------------------
# get_meta
# ---------------------------------------------------------------------------


@_SKIP
def test_informix_get_meta(informix_url: str) -> None:
    query = "select test_int, test_bool, test_date, test_datetime, test_decimal from test_types"
    df = get_meta(informix_url, query)
    expected = pd.DataFrame(
        data={
            "test_int": pd.Series([], dtype="Int64"),
            "test_bool": pd.Series([], dtype="boolean"),
            "test_date": pd.Series([], dtype="datetime64[us]"),
            "test_datetime": pd.Series([], dtype="datetime64[us]"),
            "test_decimal": pd.Series([], dtype="float64"),
        },
    )
    assert_frame_equal(df, expected, check_names=True)


# ---------------------------------------------------------------------------
# ConnectionUrl builder (no DB required)
# ---------------------------------------------------------------------------


def test_connection_url_informix_builder() -> None:
    conn = ConnectionUrl(
        backend="informix",
        username="informix",
        password="in4mix",
        server="localhost",
        port=9088,
        database="sysmaster",
    )
    assert str(conn) == "informix://informix:in4mix@localhost:9088/sysmaster"
