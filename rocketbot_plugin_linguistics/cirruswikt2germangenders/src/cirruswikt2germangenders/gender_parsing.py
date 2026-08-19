from typing import NamedTuple
from parsy import ParseError, generate, regex, string


class DeclinationSuffixes(NamedTuple):
    genitive: str
    plural: str|None

class AlternativeGender(NamedTuple):
    gender: str
    plural_flag: bool
    usage_attributes: list[str]

class GenderSpec(NamedTuple):
    gender: str
    plural_flag: bool
    alternative_genders: list[AlternativeGender]
    gender_attributes: list[str]
    usage_attributes: list[str]
    suffixes: DeclinationSuffixes|None


@generate
def raw_gender():
    """
    Parses a raw gender, such as `m` or `f`.

    May instead return "p" for a _plurale tantum_.
    """
    gender = yield regex("[mfnp]")
    return gender


@generate
def gender_and_plural_flag():
    """
    Parses a gender with an optional plural flag, such as `m` or `f` or `fp` or `f-p`.
    """
    gender = yield raw_gender
    plural_flag_result = yield regex("-?p").optional()
    plural_flag = plural_flag_result is not None
    return (gender, plural_flag)


@generate
def gender_attribute():
    """
    Parses a gender attribute such as `.sg` or `.weak`.
    """
    yield string(".")
    attribute = yield regex("[a-z]+")
    return attribute


@generate
def usage_attribute():
    """
    Parses a usage attribute such as `[less common]` or `[rare]`.
    """
    yield string("[")
    attribute = yield regex("[^]]+")
    yield string("]")
    return attribute


@generate
def dotted_usage_attribute():
    """
    Parses a dotted usage attribute such as `.[less common]` or `.[rare]`.
    """
    yield string(".")
    attribute = yield usage_attribute
    return attribute


@generate
def alternative_gender():
    """
    Parses an alternative gender attached to a raw gender, e.g. the `:f` in `m:f`.
    """
    yield string(":")
    (gender, plural_flag) = yield gender_and_plural_flag
    usage_attributes = yield usage_attribute.many()
    return AlternativeGender(gender, plural_flag, usage_attributes)


@generate
def plural_suffix():
    """
    Parses a plural suffix, such as `,^e`.
    """
    yield string(",")
    plural_suffix = yield regex("[^,>]+").optional()
    return plural_suffix


@generate
def declination_suffixes():
    """
    Parses declination suffixes, such as `,s,^e`.
    """
    yield string(",")
    g_suffix = yield regex("[^,]+").optional()
    p_suffix = yield plural_suffix.optional()
    return DeclinationSuffixes(g_suffix, p_suffix)


@generate
def single_gender_spec():
    """
    Parses a simple gender specification like `m`, `m.weak` or `m,,^e`.
    """
    (gender, plural_flag) = yield gender_and_plural_flag
    alternative_genders = yield alternative_gender.many()
    gender_attributes = yield gender_attribute.many()
    usage_attributes = yield dotted_usage_attribute.many()
    suffixes = yield declination_suffixes.optional()
    return GenderSpec(
        gender,
        plural_flag,
        alternative_genders,
        gender_attributes,
        usage_attributes,
        suffixes,
    )


@generate
def multi_gender_spec_part():
    """
    Parses a single element of a multi-gender specification like `<m,s,en>`.
    """
    yield string("<")
    spec = yield single_gender_spec
    yield string(">")
    return spec


@generate
def additional_multi_gender_spec_part():
    """
    Parses a not-first element of a multi-gender specification like `,<m,s,en>`.
    """
    yield string(",")
    spec = yield multi_gender_spec_part
    return spec


@generate
def multi_gender_spec():
    """
    Parses a multi-gender specification like `((<m,s,en>,<m.weak.[less common]>,<m.[rare]>))`.
    """
    yield string("((")
    first_part = yield multi_gender_spec_part
    other_parts = yield additional_multi_gender_spec_part.many()
    yield string("))")
    return [first_part] + other_parts


@generate
def plus_gender_spec():
    """
    Parses the "plus" gender specification, `+`.
    """
    yield string("+")
    return [GenderSpec(
        gender="m",
        plural_flag=False,
        alternative_genders=[],
        gender_attributes=["plus"],
        usage_attributes=[],
        suffixes=None,
    )]


@generate
def toponym_gender_spec():
    """
    Parses the `toponym` gender specification.
    """
    yield string("toponym")
    return [GenderSpec(
        gender="n",
        plural_flag=False,
        alternative_genders=[],
        gender_attributes=["sg", "toponym"],
        usage_attributes=[],
        suffixes=None,
    )]


@generate
def complete_gender_spec():
    """
    Parses a gender specification which is either a single or a multi-gender specification.
    """
    # always return a list
    single = single_gender_spec.map(lambda spec: [spec])
    result = yield (
        single
        | multi_gender_spec
        | plus_gender_spec
        | toponym_gender_spec
    )
    return result


def parse_complete_gender_spec(spec: str) -> list[GenderSpec]:
    try:
        return complete_gender_spec.parse(spec)
    except ParseError as pe:
        raise ValueError(f"failed to parse gender spec {spec!r}") from pe
