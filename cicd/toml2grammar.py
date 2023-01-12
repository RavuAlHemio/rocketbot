#!/usr/bin/env python3
from collections import defaultdict
import os
import sys
from typing import Any, DefaultDict, List, NamedTuple, Optional, Set, Tuple
import toml


class SentencePart(NamedTuple):
    text: str
    options: Tuple[str, ...]


def quote_grammar(s: str) -> str:
    ret = ['"']
    for c in s:
        if c == '\\':
            ret.append("\\\\")
        elif c == '"':
            ret.append("\\\"")
        else:
            ret.append(c)
    ret.append('"')
    return "".join(ret)


def load_toml(file_name: str) -> Any:
    with open(file_name, "r", encoding="utf-8") as f:
        return toml.load(f)


def validate_flags(all_flags, flags) -> None:
    for flag_list in flags:
        if flag_list != sorted(flag_list):
            raise ValueError(f"sorting error: flag list {flag_list} not sorted")
        for flag in flag_list:
            if flag not in all_flags:
                raise ValueError(f"flag error: unknown flag {flag}")


def validate_grammar(grammar_toml: Any) -> None:
    top_level_categories: List[str] = grammar_toml["metadata"]["top_level_categories"]
    if top_level_categories != sorted(top_level_categories):
        raise ValueError("metadata.top_level_categories is not sorted")

    flags = grammar_toml["metadata"].get("flags", {})

    previous_category: Optional[str] = None
    previous_left: Optional[str] = None
    for entry in grammar_toml["entries"]:
        if previous_category is not None:
            if previous_category > entry["category"]:
                raise ValueError(f"sorting error: {entry} is sorted after element with category {previous_category}")

            if previous_category != entry["category"]:
                previous_left = None

        previous_category = entry["category"]

        lower_lefts = [l.lower() for l in entry["lefts"]]
        if lower_lefts != sorted(lower_lefts):
            raise ValueError(f"sorting error: lefts {entry['lefts']} are not sorted")
        lower_rights = [r.lower() for r in entry["rights"]]
        if lower_rights != sorted(lower_rights):
            raise ValueError(f"sorting error: rights {entry['rights']} are not sorted")

        if entry["lefts"]:
            if previous_left is not None:
                if entry["lefts"][0].lower() <= previous_left.lower():
                    raise ValueError(f"sorting error: left {entry['lefts'][0]} sorted after {previous_left}")
            previous_left = entry["lefts"][0]

        lefts_flags = entry.get("lefts_flags", None)
        if lefts_flags is not None:
            if len(lefts_flags) != len(entry["lefts"]):
                raise ValueError(f"flag error: lefts {entry['lefts']} have {len(lefts_flags)} flags")
            validate_flags(flags, lefts_flags)

        rights_flags = entry.get("rights_flags", None)
        if rights_flags is not None:
            if len(rights_flags) != len(entry["rights"]):
                raise ValueError(f"flag error: rights {entry['rights']} have {len(rights_flags)} flags")
            validate_flags(flags, rights_flags)

        lefts_add_categories = entry.get("lefts_add_categories", None)
        if lefts_add_categories is not None:
            if len(lefts_add_categories) != len(entry["lefts"]):
                raise ValueError(f"category error: lefts {entry['lefts']} have {len(lefts_add_categories)} additional categories")
            for categs in lefts_add_categories:
                if categs != sorted(categs):
                    raise ValueError(f"category error: additional category list {categs} is not sorted")
                if any(c == entry["category"] for c in categs):
                    raise ValueError(f"category error: lefts {entry['lefts']} have an additional category {lefts_add_categories} that is already the base category")

        rights_add_categories = entry.get("rights_add_categories", None)
        if rights_add_categories is not None:
            if len(rights_add_categories) != len(entry["rights"]):
                raise ValueError(f"flag error: rights {entry['rights']} have {len(rights_add_categories)} additional categories")
            for categs in rights_add_categories:
                if categs != sorted(categs):
                    raise ValueError(f"category error: additional category list {categs} is not sorted")
                if any(c == entry["category"] for c in categs):
                    raise ValueError(f"category error: rights {entry['rights']} have an additional category {rights_add_categories} that is already the base category")


def compile_grammar(grammar_name: str, grammar_toml: Any) -> List[str]:
    ret: List[str] = ["// generated from a TOML file -- no sense in editing this!", ""]

    flag_to_opt = grammar_toml["metadata"]["flags"]
    glue = grammar_toml["metadata"]["glue"]

    category_lefts: DefaultDict[str, Set[SentencePart]] = defaultdict(set)
    category_rights: DefaultDict[str, Set[SentencePart]] = defaultdict(set)

    for entry in grammar_toml["entries"]:
        category = entry["category"]

        lefts = entry["lefts"]
        lefts_flags = entry.get("lefts_flags", None)
        if lefts_flags is None:
            lefts_flags = [[] for _ in lefts]
        lefts_add_categories = entry.get("lefts_add_categories", None)
        if lefts_add_categories is None:
            lefts_add_categories = [[] for _ in lefts]
        lefts_opts = [sorted(flag_to_opt[f] for f in flags) for flags in lefts_flags]

        rights = entry["rights"]
        rights_flags = entry.get("rights_flags", None)
        if rights_flags is None:
            rights_flags = [[] for _ in rights]
        rights_add_categories = entry.get("rights_add_categories", None)
        if rights_add_categories is None:
            rights_add_categories = [[] for _ in rights]
        rights_opts = [sorted(flag_to_opt[f] for f in flags) for flags in rights_flags]

        for left, opts, add_categs in zip(lefts, lefts_opts, lefts_add_categories):
            opts_tuple = tuple(opts)
            category_lefts[category].add(SentencePart(text=left, options=opts_tuple))
            for add_categ in add_categs:
                category_lefts[add_categ].add(SentencePart(text=left, options=opts_tuple))

        for right, opts, add_categs in zip(rights, rights_opts, rights_add_categories):
            opts_tuple = tuple(opts)
            category_rights[category].add(SentencePart(text=right, options=opts_tuple))
            for add_categ in add_categs:
                category_rights[add_categ].add(SentencePart(text=right, options=opts_tuple))

    top_level_options = " | ".join(f"{tlc}_pair" for tlc in grammar_toml["metadata"]["top_level_categories"])
    ret.append(f"{grammar_name} : {top_level_options} ;")
    ret.append("")
    ret.append(f"_glue : {quote_grammar(glue)} ;")
    ret.append("")

    for tlc in grammar_toml["metadata"]["top_level_categories"]:
        ret.append(f"{tlc}_pair : {tlc}_left _glue {tlc}_right ;")

    ret.append("")

    all_categories = set(category_lefts.keys()) | set(category_rights.keys())

    for category in sorted(all_categories):
        lefts = category_lefts.get(category, None)
        rights = category_rights.get(category, None)

        for end, phrases in (("left", lefts), ("right", rights)):
            if phrases is not None:
                ret.append(f"{category}_{end}")
                first = True
                for phrase in sorted(phrases):
                    if first:
                        symbol = ":"
                        first = False
                    else:
                        symbol = "|"
                    flag_str = "".join(f" !opt_{f}" for f in sorted(phrase.options))
                    ret.append(f"    {symbol}{flag_str} {quote_grammar(phrase.text)}")

                ret.append("    ;")

    return ret


def save_grammar(grammar_path: str, compiled_grammar: List[str]):
    with open(grammar_path, "w", encoding="utf-8") as f:
        for line in compiled_grammar:
            f.write(line)
            f.write("\n")


def main():
    if len(sys.argv) < 2:
        prog = sys.argv[0] if len(sys.argv) > 0 else "toml2grammar.py"
        print(f"Usage: {prog} GRAMMAR.toml...")

    for arg in sys.argv[1:]:
        if arg.lower().endswith(".toml"):
            grammar_path: str = arg[:-len(".toml")] + ".grammar"
        else:
            grammar_path = arg + ".grammar"
        grammar_name = os.path.splitext(os.path.basename(arg))[0]
        grammar_toml = load_toml(arg)

        validate_grammar(grammar_toml)
        compiled_grammar = compile_grammar(grammar_name, grammar_toml)

        save_grammar(grammar_path, compiled_grammar)


if __name__ == "__main__":
    main()
