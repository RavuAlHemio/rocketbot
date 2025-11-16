CREATE TABLE aviate.airlines
( iata_code character varying(2) NOT NULL
, name character varying(255) NOT NULL
, CONSTRAINT pk_airlines PRIMARY KEY (iata_code)
);

CREATE SEQUENCE aviate.alliance_id AS bigint;
CREATE TABLE aviate.alliances
( id bigint NOT NULL DEFAULT nextval('aviate.alliance_id')
, name character varying(255) NOT NULL
, CONSTRAINT pk_alliances PRIMARY KEY (alliance_id)
);

CREATE TABLE aviate.alliances_airlines
( alliance_id bigint NOT NULL
, airline_iata_code character varying(2) NOT NULL
, CONSTRAINT pk_alliances_airlines PRIMARY KEY (alliance_id, airline_iata_code)
, CONSTRAINT fk_alliances_airlines_alliances FOREIGN KEY (alliance_id) REFERENCES aviate.alliances (id)
, CONSTRAINT fk_alliances_airlines_airlines FOREIGN KEY (airline_iata_code) REFERENCES aviate.airlines (iata_code)
);

CREATE TABLE aviate.equipment
( code character varying(8) NOT NULL
, description character varying(255) NOT NULL
, CONSTRAINT pk_equipment PRIMARY KEY (code)
);

CREATE TABLE aviate.airports
( iata_code character varying(3) NOT NULL
, latitude numeric(7, 4) NOT NULL
, longitude numeric(7, 4) NOT NULL
, name character varying(255) NOT NULL
, CONSTRAINT pk_airports PRIMARY KEY (iata_code)
, CONSTRAINT ck_airports CHECK (latitude >= -90 AND latitude <= 90 AND longitude > -180 AND longitude <= 180)
);

CREATE SEQUENCE aviate.route_id AS bigint;
CREATE TABLE aviate.routes
( id bigint NOT NULL DEFAULT nextval('aviate.route_id')
, airline_iata_code character varying(2) NOT NULL
, from_airport_iata_code character varying(3) NOT NULL
, to_airport_iata_code character varying(3) NOT NULL
, codeshare boolean NOT NULL
, CONSTRAINT pk_routes PRIMARY KEY (id)
, CONSTRAINT uq_routes_line_port_port UNIQUE (airline_iata_code, from_airport_iata_code, to_airport_iata_code)
);
CREATE INDEX idx_routes_airports ON aviate.routes (from_airport_iata_code, to_airport_iata_code);

CREATE TABLE aviate.route_equipment
( route_id bigint NOT NULL
, equipment_code character varying(8) NOT NULL
, CONSTRAINT pk_route_equipment PRIMARY KEY (route_id, equipment_code)
, CONSTRAINT fk_route_equipment_route FOREIGN KEY (route_id) REFERENCES aviate.routes (id)
, CONSTRAINT fk_route_equipment_equipment FOREIGN KEY (equipment_code) REFERENCES aviate.equipment (code)
);
