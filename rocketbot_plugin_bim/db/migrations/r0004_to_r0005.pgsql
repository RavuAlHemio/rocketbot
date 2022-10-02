CREATE OR REPLACE FUNCTION bim.natural_compare
( left_oper text
, right_oper text
) RETURNS integer
LANGUAGE plpgsql
IMMUTABLE LEAKPROOF STRICT
PARALLEL SAFE
AS $$
    DECLARE
        left_index integer NOT NULL := 1;
        right_index integer NOT NULL := 1;
        smaller_length integer;
        left_chunks text[];
        right_chunks text[];
        left_chunk text;
        right_chunk text;
        left_number numeric;
        right_number numeric;
    BEGIN
        IF left_oper IS NULL OR right_oper IS NULL THEN
            RETURN NULL;
        END IF;

        WHILE left_index <= LENGTH(left_oper) AND right_index <= LENGTH(right_oper) LOOP
            -- try grabbing digits from left
            left_chunks := regexp_match(SUBSTRING(left_oper FROM left_index), '^[0-9]+');
            right_chunks := regexp_match(SUBSTRING(right_oper FROM right_index), '^[0-9]+');
            left_chunk := left_chunks[1];
            right_chunk := right_chunks[1];

            IF left_chunk IS NULL OR right_chunk IS NULL THEN
                EXIT;
            END IF;

            -- handle mixed cases first
            IF LENGTH(left_chunk) > 0 AND LENGTH(right_chunk) = 0 THEN
                -- sort digits first: left < right
                RETURN -1;
            END IF;
            IF LENGTH(left_chunk) = 0 AND LENGTH(right_chunk) > 0 THEN
                -- sort digits first: left > right
                RETURN 1;
            END IF;

            IF LENGTH(left_chunk) > 0 THEN
                -- handle numbers
                left_number = CAST(left_chunk AS numeric);
                right_number = CAST(right_chunk AS numeric);
                IF left_number < right_number THEN
                    RETURN -1;
                END IF;
                IF left_number > right_number THEN
                    RETURN 1;
                END IF;

                -- numbers are equal

                -- sort equal-but-not-identical numbers (e.g. due to leading zeroes) ASCIIbetically
                IF left_chunk < right_chunk THEN
                    RETURN -1;
                END IF;
                IF left_chunk > right_chunk THEN
                    RETURN 1;
                END IF;

                -- numbers are identical; skip over them and keep going
                left_index := left_index + LENGTH(left_chunk);
                right_index := right_index + LENGTH(right_chunk);
            END IF;

            -- grab non-digit characters from left
            left_chunks := regexp_match(SUBSTRING(left_oper FROM left_index), '^[^0-9]+');
            right_chunks := regexp_match(SUBSTRING(right_oper FROM right_index), '^[^0-9]+');
            left_chunk := left_chunks[1];
            right_chunk := right_chunks[1];

            IF left_chunk IS NULL OR right_chunk IS NULL THEN
                EXIT;
            END IF;

            -- compare ASCIIbetically
            IF left_chunk < right_chunk THEN
                RETURN -1;
            END IF;
            IF left_chunk > right_chunk THEN
                RETURN 1;
            END IF;

            -- still the same; loop over
            left_index := left_index + LENGTH(left_chunk);
            right_index := right_index + LENGTH(right_chunk);
        END LOOP;

        -- common prefix to both functions; compare lengths
        IF LENGTH(left_oper) < LENGTH(right_oper) THEN
            RETURN -1;
        END IF;
        IF LENGTH(left_oper) > LENGTH(right_oper) THEN
            RETURN 1;
        END IF;

        -- equal lengths as well
        RETURN 0;
    END;
$$;

CREATE OR REPLACE FUNCTION bim.natural_lt(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) = -1;
CREATE OR REPLACE FUNCTION bim.natural_leq(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) <> 1;
CREATE OR REPLACE FUNCTION bim.natural_gt(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) = 1;
CREATE OR REPLACE FUNCTION bim.natural_geq(left_oper text, right_oper text) RETURNS boolean
LANGUAGE sql IMMUTABLE LEAKPROOF STRICT PARALLEL SAFE
RETURN bim.natural_compare(left_oper, right_oper) <> -1;

CREATE OPERATOR bim.<~<
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_lt
, COMMUTATOR = OPERATOR(bim.>~>)
, NEGATOR = OPERATOR(bim.>~>=)
);
CREATE OPERATOR bim.<~<=
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_leq
, COMMUTATOR = OPERATOR(bim.>~>=)
, NEGATOR = OPERATOR(bim.>~>)
);
CREATE OPERATOR bim.>~>
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_gt
, COMMUTATOR = OPERATOR(bim.<~<)
, NEGATOR = OPERATOR(bim.<~<=)
);
CREATE OPERATOR bim.>~>=
( LEFTARG = text
, RIGHTARG = text
, FUNCTION = bim.natural_geq
, COMMUTATOR = OPERATOR(bim.<~<=)
, NEGATOR = OPERATOR(bim.<~<)
);

CREATE OPERATOR CLASS bim.natural_compare_class
FOR TYPE text
USING btree
AS  OPERATOR 1 bim.<~<
,   OPERATOR 2 bim.<~<=
,   OPERATOR 3 =
,   OPERATOR 4 bim.>~>=
,   OPERATOR 5 bim.>~>
,   FUNCTION 1 bim.natural_compare
;

UPDATE bim.schema_revision SET sch_rev=5;
