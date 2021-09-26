#!/usr/bin/env python3
from collections import defaultdict
import os
import sys


GLUE = " "


def grammar_quote(s):
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


def generate_grammar(tsv_path):
    if tsv_path.lower().endswith(".tsv"):
        grammar_path = tsv_path[:-len(".tsv")] + ".grammar"
    else:
        grammar_path = tsv_path + ".grammar"
    grammar_name = os.path.splitext(os.path.basename(grammar_path))[0]

    category_to_heads = defaultdict(list)
    category_to_tails = defaultdict(list)

    with open(tsv_path, "r", encoding="utf-8") as f:
        for i, ln in enumerate(f.readlines()):
            if i == 0:
                # header
                continue

            ln = ln.rstrip("\r\n")

            (category, head, tail) = ln.split("\t")
            if not head or not tail:
                # incomplete; skip
                continue
            category_to_heads[category].append(head)
            category_to_tails[category].append(tail)

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

        category_pair_or = " | ".join(f"{category}_pair" for category in categories)
        print(f"{grammar_name} : {category_pair_or} ;", file=f)
        print("", file=f)

        for category in categories:
            print(f"{category}_pair : {category}_head {grammar_quote(GLUE)} {category}_tail ;", file=f)

        print("", file=f)

        for category in categories:
            for (piece, category_to_ends) in (("head", category_to_heads), ("tail", category_to_tails)):
                print(f"{category}_{piece}", file=f)

                first = True
                for head in category_to_ends[category]:
                    if first:
                        symbol = ":"
                        first = False
                    else:
                        symbol = "|"

                    print(f"    {symbol} {grammar_quote(head)}", file=f)

                print("    ;", file=f)


def main():
    for tsv_path in sys.argv[1:]:
        generate_grammar(tsv_path)


if __name__ == "__main__":
    main()
