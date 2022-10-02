WITH nlt_tests(test_id, test_pass) AS (
    SELECT 0, bim.natural_compare('', '') = 0
    UNION ALL
    SELECT 1, bim.natural_compare('', 'a') = -1
    UNION ALL
    SELECT 2, bim.natural_compare('a', '') = 1
    UNION ALL
    SELECT 3, bim.natural_compare('', '4') = -1
    UNION ALL
    SELECT 4, bim.natural_compare('4', '') = 1
    UNION ALL
    SELECT 5, bim.natural_compare('3', '12') = -1
    UNION ALL
    SELECT 6, bim.natural_compare('12', '3') = 1
    UNION ALL
    SELECT 7, bim.natural_compare('abc3', 'abc12') = -1
    UNION ALL
    SELECT 8, bim.natural_compare('abc12', 'abc3') = 1
    UNION ALL
    SELECT 9, bim.natural_compare('abc3def', 'abc12def') = -1
    UNION ALL
    SELECT 10, bim.natural_compare('abc12def', 'abc3def') = 1
    UNION ALL
    SELECT 11, bim.natural_compare('3abc', '12abc') = -1
    UNION ALL
    SELECT 12, bim.natural_compare('12abc', '3abc') = 1
    UNION ALL
    SELECT 13, bim.natural_compare('3abc', '3def') = -1
    UNION ALL
    SELECT 14, bim.natural_compare('3def', '3abc') = 1
    UNION ALL
    SELECT 15, bim.natural_compare('3abc3', '3abc3') = 0
    UNION ALL
    SELECT 16, bim.natural_compare('abc3def', 'abc3def') = 0
)
SELECT
    test_id failing_test_id
FROM
    nlt_tests
WHERE
    NOT test_pass
;
