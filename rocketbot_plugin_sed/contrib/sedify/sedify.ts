namespace Sedify {
    interface ConstString {
        type: "const";
        constString: string;
    }

    interface OriginalReference {
        type: "origref";
        startBefore: number;
        endBefore: number;
    }

    type ReplacementPiece = ConstString | OriginalReference;

    class Sedifier {
        public dotsExceptMostCommon: boolean = true;

        private getCharacterFrequencies(str: string): object {
            const ret = {};
            for (let i = 0; i < str.length; i++) {
                const c = str.charAt(i);
                if (ret[c] === undefined) {
                    ret[c] = 0;
                }
                ret[c]++;
            }
            return ret;
        }

        private getMostCommonCharacter(str: string): string {
            const frequencies = this.getCharacterFrequencies(str);
            let mostChar = "";
            let mostCount = 0;
            for (let i = 0; i < str.length; i++) {
                const c = str.charAt(i);
                if (mostCount < frequencies[c]) {
                    mostChar = c;
                    mostCount = frequencies[c];
                }
            }
            return mostChar;
        }

        private getLongestInitialNeedle(haystack: string, needlework: string): [string, number] {
            let needle = "";
            let pos = 0;
            while (needle.length < needlework.length) {
                const tryNeedle = needle + needlework[needle.length];
                const tryPos = haystack.indexOf(tryNeedle);
                if (tryPos === -1) {
                    return [needle, pos];
                }
                needle = tryNeedle;
                pos = tryPos;
            }
            return [needle, pos];
        }

        public makeSed(fromStr: string, toStr: string): string {
            const fromMostCommon = this.getMostCommonCharacter(fromStr);

            // first, assemble the replacement string
            let toStrNeedle = toStr;
            const replacements: ReplacementPiece[] = [];
            while (toStrNeedle.length > 0) {
                const [initialNeedle, posInHaystack] = this.getLongestInitialNeedle(fromStr, toStrNeedle);
                if (initialNeedle.length === 0) {
                    // okay, the first character is not in the original string; just append it verbatim
                    replacements.push({
                        type: "const",
                        constString: toStrNeedle.charAt(0),
                    });
                    toStrNeedle = toStrNeedle.substring(1);
                } else {
                    // we can do this using a reference
                    replacements.push({
                        type: "origref",
                        startBefore: posInHaystack,
                        endBefore: posInHaystack + initialNeedle.length,
                    });
                    toStrNeedle = toStrNeedle.substring(initialNeedle.length);
                }
            }

            // next, give numbers to the references
            // 1. collect all unique references
            const knownRefs: OriginalReference[] = [];
            for (let i = 0; i < replacements.length; i++) {
                switch (replacements[i].type) {
                    case "const":
                        break;
                    case "origref":
                        const origRefReplacement = <OriginalReference>replacements[i];
                        let found = false;
                        for (let j = 0; j < knownRefs.length; j++) {
                            if (knownRefs[j].startBefore === origRefReplacement.startBefore && knownRefs[j].endBefore === origRefReplacement.endBefore) {
                                found = true;
                                break;
                            }
                        }
                        if (!found) {
                            knownRefs.push(origRefReplacement);
                        }
                        break;
                }
            }

            // 2. sort them in ascending order by startBefore, then descending order by endBefore
            knownRefs.sort((l, r) => {
                if (l.startBefore < r.startBefore) {
                    return -1;
                }
                if (l.startBefore > r.startBefore) {
                    return 1;
                }

                // descending order => reverse arguments
                if (r.endBefore < l.endBefore) {
                    return -1;
                }
                if (r.endBefore > l.endBefore) {
                    return 1;
                }

                return 0;
            });

            // 3. construct the replacement string by looking up the references
            let replacementString = "";
            for (let i = 0; i < replacements.length; i++) {
                switch (replacements[i].type) {
                    case "const":
                        const constReplacement = <ConstString>replacements[i];
                        replacementString += constReplacement.constString;
                        break;
                    case "origref":
                        const origRefReplacement = <OriginalReference>replacements[i];
                        // find this replacement in the sorted list
                        let knownIndex = -1;
                        for (let j = 0; j < knownRefs.length; j++) {
                            if (knownRefs[j].startBefore === origRefReplacement.startBefore && knownRefs[j].endBefore === origRefReplacement.endBefore) {
                                knownIndex = j;
                                break;
                            }
                        }
                        if (knownIndex === -1) {
                            throw new Error(`replacement with startBefore=${origRefReplacement.startBefore} and endBefore=${origRefReplacement.endBefore} did not make it into knownRefs ${JSON.stringify(knownRefs)}`);
                        }
                        // note: since index 0 is the whole original string, the actual replacement index is 1 greater
                        if (knownIndex + 1 > 9) {
                            // special syntax: "\g999;"
                            replacementString += `\\g${knownIndex+1};`;
                        } else {
                            replacementString += `\\${knownIndex+1}`;
                        }
                        break;
                }
            }

            // 4. construct the search string from dots and the most common character, interpolating parentheses as required
            let searchString = "";
            for (let i = 0; i < fromStr.length; i++) {
                for (let j = 0; j < knownRefs.length; j++) {
                    if (knownRefs[j].endBefore === i) {
                        searchString += ")";
                    }
                }
                for (let j = 0; j < knownRefs.length; j++) {
                    if (knownRefs[j].startBefore === i) {
                        searchString += "(";
                    }
                }

                const c = fromStr.charAt(i);
                if (c !== fromMostCommon && this.dotsExceptMostCommon) {
                    searchString += ".";
                } else {
                    searchString += c;
                }
            }

            for (let j = 0; j < knownRefs.length; j++) {
                if (knownRefs[j].endBefore === fromStr.length) {
                    searchString += ")";
                }
            }

            return `s/${searchString}/${replacementString}/`;
        }
    }

    function submitMakeSedForm(event: SubmitEvent) {
        event.preventDefault();

        const fromInput = <HTMLInputElement|null>document.getElementById("sedify-from");
        const toInput = <HTMLInputElement|null>document.getElementById("sedify-to");
        const resultPre = <HTMLPreElement|null>document.getElementById("sedify-result");
        if (fromInput === null || toInput === null || resultPre === null) {
            return;
        }

        const sedifier = new Sedifier();

        const dotsInput = <HTMLInputElement|null>document.getElementById("sedify-dots");
        if (dotsInput !== null && !dotsInput.checked) {
            sedifier.dotsExceptMostCommon = false;
        }

        resultPre.innerText = sedifier.makeSed(fromInput.value, toInput.value);
    }

    function setUp() {
        const form = <HTMLFormElement|null>document.getElementById("sedify-form");
        if (form === null) {
            return;
        }
        form.addEventListener("submit", submitMakeSedForm);
    }

    document.addEventListener("DOMContentLoaded", () => {
        setUp();
    });
}
