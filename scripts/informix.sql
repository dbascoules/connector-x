DROP TABLE IF EXISTS test_table;

CREATE TABLE test_table(
    test_id VARCHAR(8) NOT NULL,
    test_text VARCHAR(32),
    test_nullable VARCHAR(32)
);

INSERT INTO test_table VALUES ('1', 'alpha', 'note-1');
INSERT INTO test_table VALUES ('2', 'beta', NULL);
INSERT INTO test_table VALUES ('3', NULL, 'note-3');