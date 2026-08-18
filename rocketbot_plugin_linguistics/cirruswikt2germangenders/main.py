import bz2
from collections.abc import Iterable
import enum
import io
import json
import sys
import tomllib

import mwparserfromhell


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

    @property
    def is_empty(self) -> bool:
        return self.flag == GenderFlag(0)


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

def read_to_newline(bio: io.BufferedIOBase) -> bytearray:
    buffer = bytearray()
    while not buffer.endswith(b"\n"):
        buffer.append(0x00)
        now_read = bio.readinto(memoryview(buffer)[-1:])
        if now_read == 0:
            # EOF
            buffer.pop()
            return buffer
    return buffer

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
            if param_one in ("m", "m.weak", "m,,^e", "((<m,s,en>,<m.weak.[less common]>,<m.[rare]>))"):
                flag_holder.flag |= GenderFlag.MASCULINE
            elif param_one in ("f",):
                flag_holder.flag |= GenderFlag.FEMININE
            elif param_one in ("n", "n,(e)s,^er"):
                flag_holder.flag |= GenderFlag.NEUTER
            elif param_one in ("m.sg",):
                flag_holder.flag |= GenderFlag.MASCULINE | GenderFlag.SINGULARE_TANTUM
            elif param_one in ("f.sg",):
                flag_holder.flag |= GenderFlag.FEMININE | GenderFlag.SINGULARE_TANTUM
            elif param_one in ("n.sg",):
                flag_holder.flag |= GenderFlag.NEUTER | GenderFlag.SINGULARE_TANTUM
            else:
                raise ValueError(f"unknown de-noun gender parameters {param_ones}")

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
            if gender_param == "m":
                flag_holder.flag |= GenderFlag.MASCULINE
            else:
                raise ValueError(f"TODO: head gender parameters {gender_params}")

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


def main():
    if len(sys.argv) == 1:
        config_path = "config.toml"
    elif len(sys.argv) == 2:
        config_path = sys.argv[1]
    else:
        print(f"Usage: {sys.argv[0]} [CONFIG.TOML]", file=sys.stderr)
        sys.exit(1)

    with open(config_path, "rb") as f:
        config = tomllib.load(f)

    for content_json_bz2_path in config["content_json_bz2_paths"]:
        with bz2.BZ2File(content_json_bz2_path, "r") as bz2_file:
            with io.BufferedReader(bz2_file) as bz2_buffer:
                while (json_line_bs := read_to_newline(bz2_buffer)):
                    json_line_str = json_line_bs.decode("utf-8")
                    json_line = json.loads(json_line_str)
                    wikitext = json_line.get("source_text", None)
                    if wikitext is None:
                        continue

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
                        print(json_line["title"])
                        holder = FlagHolder()
                        de_noun_genders(section, holder)
                        head_genders(section, holder)
                        if holder.is_empty:
                            # check if we can skip this entry
                            skippable = False
                            if not skippable and is_head_benign(section):
                                skippable = True
                            if not skippable and has_benign_template(section):
                                skippable = True

                            if not skippable:
                                print(section)
                                raise ValueError("unknown word construct")


if __name__ == "__main__":
    main()
