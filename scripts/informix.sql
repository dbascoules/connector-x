DROP TABLE IF EXISTS test_types;

CREATE TABLE test_types(
    test_bool BOOLEAN,
    test_smallint SMALLINT,
    test_int INTEGER,
    test_bigint BIGINT,
    test_float REAL,
    test_double DOUBLE PRECISION,
    test_decimal DECIMAL(10,2),
    test_char CHAR(5),
    test_varchar VARCHAR(16),
    test_text LVARCHAR(128),
    test_date DATE,
    test_datetime DATETIME YEAR TO FRACTION(5),
    test_nullable VARCHAR(16)
);

INSERT INTO test_types VALUES (
    CAST('t' AS BOOLEAN),
    -32767, -- SMALLINT range is -32768 to 32767 on Informix
    0,
    9223372036854775807,
    -1.1,
    -1.12345,
    -1234.56,
    'abc',
    'varchar',
    'informix text',
    '1970-01-01',
    '1970-01-01 00:00:01.12345',
    'row-one'
);
INSERT INTO test_types VALUES (
    CAST('f' AS BOOLEAN),
    32767,
    2147483647,
    -9223372036854775807, -- BIGINT range is -9223372036854775807 to 9223372036854775807 on Informix
    3.14159,
    2.71828,
    12345.67,
    'xyz',
    'varchar2',
    'longer informix text',
    '9999-12-31',
    '9999-12-31 23:59:59.99999',
    NULL
);
INSERT INTO test_types VALUES (
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    NULL
);