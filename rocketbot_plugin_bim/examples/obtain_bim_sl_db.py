#!/usr/bin/env python3
import argparse
import csv
import enum
from enum import auto
import io
import itertools
import re
from typing import Dict, List, Optional, Set, Tuple, TypeVar
import cbor2


CLASS_TO_ICE_TYPE_MANUF = {
    "401": ("ICE 1", "Siemens"),
    "402": ("ICE 2", "Siemens"),
    "403": ("ICE 3", "Siemens"),
    "406": ("ICE 3", "Siemens"),
    "407": ("ICE 3", "Siemens"),
    "408": ("ICE 3neo", "Siemens"),
    "410": ("ICE S", "Siemens"),
    "411": ("ICE T", "Siemens"),
    "412": ("ICE 4", "Siemens"),
    "412.2": ("ICE 4", "Siemens"),
    "412.4": ("ICE 4", "Siemens"),
    "415": ("ICE T", "Siemens"),
    "605": ("ICE TD", "Siemens"),
}


LONG_ICE_RE = re.compile("^[0-9]{4} [0-9]{3}-[0-9]$")
SHORT_ICE_RE = re.compile("^[0-9]{3} [0-9]{3}-[0-9]$")


class VehicleClass(enum.Enum):
    Tram = auto()
    Metro = auto()
    PreMetro = auto()
    Bus = auto()
    Trolleybus = auto()
    TramTrain = auto()
    RegionalTrain = auto()
    LongDistanceTrain = auto()
    HorseDrawnCarriage = auto()
    Funicular = auto()
    AerialTramway = auto()
    JBarLift = auto()
    TBarLift = auto()
    SeatLift = auto()
    GondolaLift = auto()
    SeatAndGondolaLift = auto()
    Ship = auto()
    Hovercraft = auto()
    Taxibus = auto()

    @property
    def json_str(self) -> str:
        return {
            VehicleClass.Tram: "tram",
            VehicleClass.Metro: "metro",
            VehicleClass.PreMetro: "pre-metro",
            VehicleClass.Bus: "bus",
            VehicleClass.Trolleybus: "trolleybus",
            VehicleClass.TramTrain: "tram-train",
            VehicleClass.RegionalTrain: "regional-train",
            VehicleClass.LongDistanceTrain: "long-distance-train",
            VehicleClass.HorseDrawnCarriage: "horse-drawn-carriage",
            VehicleClass.Funicular: "funicular",
            VehicleClass.AerialTramway: "aerial-tramway",
            VehicleClass.JBarLift: "j-bar-lift",
            VehicleClass.TBarLift: "t-bar-lift",
            VehicleClass.SeatLift: "seat-lift",
            VehicleClass.GondolaLift: "gondola-lift",
            VehicleClass.SeatAndGondolaLift: "seat-and-gondola-lift",
            VehicleClass.Ship: "ship",
            VehicleClass.Hovercraft: "hovercraft",
            VehicleClass.Taxibus: "taxibus",
        }[self]


class PowerSource(enum.Enum):
    Unpowered = auto()
    OverheadWire = auto()
    AdditionalRail = auto()
    AdditionalRailPair = auto()
    Battery = auto()
    Hydrogen = auto()
    Gasoline = auto()
    Diesel = auto()
    Kerosene = auto()
    Cng = auto()
    Lng = auto()
    Lpg = auto()
    Human = auto()
    Animal = auto()
    Rope = auto()
    GuideBar = auto()

    @property
    def json_str(self) -> str:
        return {
            PowerSource.Unpowered: "unpowered",
            PowerSource.OverheadWire: "overhead-wire",
            PowerSource.AdditionalRail: "additional-rail",
            PowerSource.AdditionalRailPair: "additional-rail-pair",
            PowerSource.Battery: "battery",
            PowerSource.Hydrogen: "hydrogen",
            PowerSource.Gasoline: "gasoline",
            PowerSource.Diesel: "diesel",
            PowerSource.Kerosene: "kerosene",
            PowerSource.Cng: "cng",
            PowerSource.Lng: "lng",
            PowerSource.Lpg: "lpg",
            PowerSource.Human: "human",
            PowerSource.Animal: "animal",
            PowerSource.Rope: "rope",
            PowerSource.GuideBar: "guide-bar",
        }[self]


class Vehicle:
    def __init__(
        self,
        number: str,
        vehicle_class: VehicleClass,
        power_sources: Set[PowerSource],
        type_code: str,
        in_service_since: Optional[str],
        out_of_service_since: Optional[str],
        manufacturer: Optional[str],
        depot: Optional[str],
        other_data: Dict[str, str],
        fixed_coupling: List[str],
    ):
        self.number: str = number
        self.vehicle_class: VehicleClass = vehicle_class
        self.power_sources: Set[PowerSource] = power_sources
        self.type_code: str = type_code
        self.in_service_since: Optional[str] = in_service_since
        self.out_of_service_since: Optional[str] = out_of_service_since
        self.manufacturer: Optional[str] = manufacturer
        self.depot: Optional[str] = depot
        self.other_data: Dict[str, str] = other_data
        self.fixed_coupling: List[str] = fixed_coupling

    @property
    def jsonable(self) -> str:
        return {
            "number": self.number,
            "vehicle_class": self.vehicle_class.json_str,
            "power_sources": sorted(ps.json_str for ps in self.power_sources),
            "type_code": self.type_code,
            "in_service_since": self.in_service_since,
            "out_of_service_since": self.out_of_service_since,
            "manufacturer": self.manufacturer,
            "depot": self.depot,
            "other_data": self.other_data,
            "fixed_coupling": self.fixed_coupling,
        }


def coalesce_strs(*xs: str) -> Optional[str]:
    for x in xs:
        if x:
            return x
    return None


def luhn(digits: str) -> int:
    actual_digits = [int(d) for d in digits if "0" <= d <= "9"]
    actual_digits.reverse()
    multipliers = itertools.cycle((2, 1))
    products = (d * m for (d, m) in zip(actual_digits, multipliers))
    digit_sums = ((p - 9 if p > 9 else p) for p in products)
    total_sum = sum(digit_sums)
    return (10 - (total_sum % 10)) % 10


def make_spotlog_fixes(prefix: str, *wrongs: str) -> Dict[str, str]:
    ret = {}
    for wrong in wrongs:
        # get the variant with the prefix but without the check digit
        nocd = prefix + wrong.split("-")[0]

        # calculate the new check digit
        cd = luhn(nocd)

        # store that mapping
        ret[wrong] = f"{nocd}-{cd}"
    return ret


def read_csv_to_vehicles(text_reader: io.TextIOBase) -> List[Vehicle]:
    reader = csv.DictReader(text_reader)
    vehicles = []
    for entry in reader:
        if entry["Subset"] != "{40}ICE High-Speed Train":
            continue
        (type_code, manuf) = CLASS_TO_ICE_TYPE_MANUF[entry["Class"]]
        vehicle_numbers_db_uic: List[Tuple[str, str]] = []
        for vehicle in entry["Form"].split(", "):
            # 7812 212-5 => 93 80 7812 212-5
            if LONG_ICE_RE.match(vehicle) is not None:
                uic_number = f"93 80 {vehicle}"
            elif SHORT_ICE_RE.match(vehicle) is not None:
                uic_number = f"93 80 5{vehicle}"

            # check if that can be true
            uic_parts = uic_number.split("-")
            expected_check = int(uic_parts[1])
            calculated_check = luhn(uic_parts[0])
            if expected_check != calculated_check:
                print(f"warning: invalid UIC number {uic_number!r}: calculated check digit is {calculated_check}")
                # assume SpotLog got it wrong
                uic_number = f"{uic_parts[0]}-{calculated_check}"

            # 7812 212-5 => 7812212
            db_number = vehicle.split("-")[0].replace(" ", "")

            vehicle_numbers_db_uic.append((db_number, uic_number))

        fixed_coupling = [db_uic[0] for db_uic in vehicle_numbers_db_uic]

        iss: Optional[str] = None
        ooss: Optional[str] = None

        if entry["Status"] in ("A", "S"): # active, S???
            iss = coalesce_strs(entry["InService"], "?")
            ooss = None
        elif entry["Status"] in ("W", "X"): # withdrawn, scrapped
            iss = coalesce_strs(entry["InService"], "?")
            ooss = coalesce_strs(entry["Withdrawn"], entry["Scrapped"], "?")
        elif entry["Status"] == "": # unknown?
            iss = None
            ooss = None
        elif entry["Status"] in ("D", "P"): # ???
            iss = None
            ooss = None
        elif entry["Status"] == "N": # new
            iss = None
            ooss = None
        else:
            print(entry)
            raise ValueError(f"unknown status {entry['Status']!r}")

        for (db_number, uic_number) in vehicle_numbers_db_uic:
            vehicles.append(Vehicle(
                number=db_number,
                vehicle_class=VehicleClass.LongDistanceTrain,
                power_sources={PowerSource.OverheadWire},
                type_code=type_code,
                in_service_since=iss,
                out_of_service_since=ooss,
                manufacturer=manuf,
                depot=None,
                other_data={
                    "UIC-Nummer": uic_number,
                },
                fixed_coupling=fixed_coupling,
            ))

    return vehicles


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        dest="input_csv",
    )
    parser.add_argument(
        dest="output_cbor",
    )
    args = parser.parse_args()

    with open(args.input_csv, "r", encoding="utf-16le") as in_file:
        vehicles = read_csv_to_vehicles(in_file)
    with open(args.output_cbor, "wb") as out_file:
        cbor2.dump([v.jsonable for v in vehicles], out_file)


if __name__ == "__main__":
    main()
