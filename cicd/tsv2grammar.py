#!/usr/bin/env python3
from collections import defaultdict
import os
import sys
from typing import DefaultDict, NamedTuple


GLUE: str = " "


class GrammarVariant(NamedTuple):
    text: str
    subversive: bool


def grammar_quote(s: str) -> str:
    ret = ['"']
    for c in s:
        if c == '"':
            ret.append('\\"')
        elif c == "\\":
            ret.append('\\\\')
        else:
            ret.append(c)
    ret.append('"')
    return "".join(ret)


def generate_grammar(tsv_path: str) -> None:
    if tsv_path.lower().endswith(".tsv"):
        grammar_path: str = tsv_path[:-len(".tsv")] + ".grammar"
    else:
        grammar_path = tsv_path + ".grammar"
    grammar_name: str = os.path.splitext(os.path.basename(grammar_path))[0]

    category_to_heads: DefaultDict[str, list[GrammarVariant]] = defaultdict(list)
    category_to_tails: DefaultDict[str, list[GrammarVariant]] = defaultdict(list)

    with open(tsv_path, "r", encoding="utf-8") as f:
        for i, ln in enumerate(f.readlines()):
            if i == 0 and ln.startswith("group\tleft\tright"):
                # header
                continue

            ln = ln.rstrip("\r\n")

            (category, head, tail, *more) = ln.split("\t")
            if head:
                category_to_heads[category].append(GrammarVariant(head, False))
            if tail:
                category_to_tails[category].append(GrammarVariant(tail, False))

            if len(more) > 0 and more[0]:
                # subversive head
                category_to_heads[category].append(GrammarVariant(more[0], True))
            if len(more) > 1 and more[1]:
                category_to_tails[category].append(GrammarVariant(more[1], True))

    # sort
    for heads in category_to_heads.values():
        heads.sort()
    for tails in category_to_tails.values():
        tails.sort()
    categories = sorted(category_to_heads.keys())

    # output the grammar
    with open(grammar_path, "w", encoding="utf-8") as f:
        print("// generated from a TSV file -- no sense in editing this!", file=f)
        print("", file=f)

        category_pair_strings = []
        first = True
        for category in categories:
            if first:
                symbol = ":"
                first = False
            else:
                symbol = "|"
            category_pair_strings.append(f" {symbol}<{len(category_to_heads[category])}> {category}_pair")
        category_pairs = "".join(category_pair_strings)

        print(f"{grammar_name}{category_pairs} ;", file=f)
        print("", file=f)

        for category in categories:
            print(f"{category}_pair : {category}_head {grammar_quote(GLUE)} {category}_tail ;", file=f)

        print("", file=f)

        for category in categories:
            for (piece, category_to_ends) in (("head", category_to_heads), ("tail", category_to_tails)):
                print(f"{category}_{piece}", file=f)

                first = True
                for end in category_to_ends[category]:
                    if first:
                        symbol = ":"
                        first = False
                    else:
                        symbol = "|"

                    if end.subversive:
                        subversive_option = " !opt_s"
                    else:
                        subversive_option = ""

                    print(f"    {symbol}{subversive_option} {grammar_quote(end.text)}", file=f)

                print("    ;", file=f)


def main():
    for tsv_path in sys.argv[1:]:
        generate_grammar(tsv_path)


if __name__ == "__main__":
    main()
