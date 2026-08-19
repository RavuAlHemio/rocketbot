import mwparserfromhell

from .common import FlagHolder, GenderFlag
from .gender_parsing import GenderSpec, parse_complete_gender_spec


LANGUAGE_CODES = frozenset({"de"})
OK_TO_SKIP_HEAD_TYPES = frozenset({
    "adjective form",
    "interjection",
    "noun form",
    "numeral",
    "past participle",
    "phrase",
    "present participle",
    "pronoun form",
    "proper noun",
    "proper noun form",
    "verb form",
})
OK_TO_SKIP_TEMPLATES = frozenset({
    "de-adj",
    "de-adv",
    "de-proper noun",
    "de-verb",
#
    "abbreviation of",
#    "de-adj form of",
#    "form of",
#    "infl of",
#    "inflection of",
#    "noun form of",
#    "past participle of",
#    "present participle of",
#    "verb form of",
})

def process_gender_specs(gender_specs: list[GenderSpec], flag_holder: FlagHolder):
    for gender_spec in gender_specs:
        if gender_spec.gender == "m":
            flag_holder.flag |= GenderFlag.MASCULINE
        elif gender_spec.gender == "f":
            flag_holder.flag |= GenderFlag.FEMININE
        elif gender_spec.gender == "n":
            flag_holder.flag |= GenderFlag.NEUTER

        if "sg" in gender_spec.gender_attributes:
            flag_holder.flag |= GenderFlag.SINGULARE_TANTUM


def de_noun_genders(section: mwparserfromhell.wikicode.Wikicode, flag_holder: FlagHolder) -> None:
    dn_calls = [
        tc
        for tc in section.ifilter_templates()
        if tc.name == "de-noun"
    ]
    for dn_call in dn_calls:
        param_ones = [
            str(param.value)
            for param in dn_call.params
            if param.name == "1"
        ]
        if not param_ones:
            continue
        for param_one in param_ones:
            gender_specs = parse_complete_gender_spec(param_one)
            process_gender_specs(gender_specs, flag_holder)

def head_genders(section: mwparserfromhell.wikicode.Wikicode, flag_holder: FlagHolder) -> None:
    head_calls = [
        tc
        for tc in section.ifilter_templates()
        if tc.name == "head"
    ]
    for head_call in head_calls:
        gender_params = [
            str(param.value)
            for param in head_call.params
            if param.name == "g"
        ]
        if not gender_params:
            continue
        for gender_param in gender_params:
            gender_specs = parse_complete_gender_spec(gender_param)
            process_gender_specs(gender_specs, flag_holder)

def is_head_benign(section: mwparserfromhell.wikicode.Wikicode) -> bool:
    head_calls = [tc for tc in section.ifilter_templates() if tc.name == "head"]
    head_types = {
        str(param.value)
        for tc in head_calls
        for param in tc.params
        if param.name == "2"
    }
    return any(head_type in OK_TO_SKIP_HEAD_TYPES for head_type in head_types)

def has_benign_template(section: mwparserfromhell.wikicode.Wikicode) -> bool:
    template_call_names = {str(tc.name) for tc in section.ifilter_templates()}
    return any(tcn in OK_TO_SKIP_TEMPLATES for tcn in template_call_names)

def process_page(title: str, wikitext: str) -> None:
    parsed = mwparserfromhell.parse(wikitext)
    for section in parsed.get_sections():
        if not section.nodes:
            continue
        if not isinstance(section.nodes[0], mwparserfromhell.nodes.heading.Heading):
            continue
        if section.nodes[0].level != 2:
            continue
        if section.nodes[0].title.strip() != "German":
            continue

        # assemble the genders
        holder = FlagHolder()
        try:
            de_noun_genders(section, holder)
            head_genders(section, holder)
        except ValueError:
            print(f"failed to parse genders of {title!r}")
            raise

        if holder.is_empty:
            # check if we can skip this entry
            skippable = False
            if not skippable and is_head_benign(section):
                skippable = True
            if not skippable and has_benign_template(section):
                skippable = True

            if not skippable:
                print(section)
                raise ValueError(f"{title}: unknown word construct")
            continue
        print(title, repr(holder.flag))
