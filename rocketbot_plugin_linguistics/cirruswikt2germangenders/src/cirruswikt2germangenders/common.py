import enum


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
