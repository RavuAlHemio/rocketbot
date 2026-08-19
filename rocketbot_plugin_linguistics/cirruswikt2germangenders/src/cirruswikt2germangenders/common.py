import enum
from typing import Any, Iterable, Mapping


class GenderFlag(enum.IntFlag):
    MASCULINE = (1 << 0)
    FEMININE = (1 << 1)
    NEUTER = (1 << 2)
    SINGULARE_TANTUM = (1 << 3)
    PLURALE_TANTUM = (1 << 4)
    MALE_GIVEN = (1 << 5)
    FEMALE_GIVEN = (1 << 6)
    UNISEX_GIVEN = (1 << 7)


class FlagHolder:
    def __init__(self) -> None:
        self.flag: GenderFlag = GenderFlag(0)
        self.sane: bool = True

    @property
    def is_empty(self) -> bool:
        return self.flag == GenderFlag(0)


def handle_override_value(
    override_dict: Mapping[str, Iterable[str]],
    title: str,
) -> GenderFlag|None:
    override_value = override_dict.get(title, None)
    if override_value is None:
        return None
    gender = GenderFlag(0)
    for flag_str in override_value:
        flag_value = getattr(GenderFlag, flag_str)
        gender |= flag_value
    return gender


def get_override_dict(
    config: Mapping[str, Any],
) -> dict[str, list[str]]:
    return {
        override_obj["key"]: override_obj["gender_flags"]
        for override_obj
        in config.get("overrides", [])
    }
