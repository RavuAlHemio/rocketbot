export namespace Drilldown {
    function addSelectRow(form: HTMLElement, addParagraphElement: HTMLElement, options: string[], selectedOption: string|null) {
        const paragraphElement = document.createElement("p");
        form.insertBefore(paragraphElement, addParagraphElement);

        const selectElement = document.createElement("select");
        selectElement.name = "group";
        paragraphElement.appendChild(selectElement);

        for (const option of options) {
            const optionElement = document.createElement("option");
            optionElement.innerText = option;
            if (option === selectedOption) {
                optionElement.selected = true;
            }
            selectElement.appendChild(optionElement);
        }

        const removeButton = document.createElement("input");
        removeButton.type = "button";
        removeButton.value = "\u2212";
        removeButton.addEventListener("click", () => paragraphElement.parentElement?.removeChild(paragraphElement));
        paragraphElement.appendChild(removeButton);
    }

    function doSetUp(options: string[]) {
        const drilldownControls = <HTMLDivElement|null>document.getElementById("bim-drilldown-controls");
        if (drilldownControls === null) {
            return;
        }

        const searchString = window.location.search;
        const groupByColumns: string[] = [];
        if (searchString.startsWith("?")) {
            const searchPairs = new URLSearchParams(searchString.substring(1));
            for (const [searchKey, searchValue] of searchPairs) {
                if (searchKey === "group") {
                    groupByColumns.push(searchValue);
                }
            }
        }

        const form = document.createElement("form");
        form.method = "get";
        drilldownControls.appendChild(form);

        const addParagraphElement = document.createElement("p");
        form.appendChild(addParagraphElement);

        for (const groupByColumn of groupByColumns) {
            addSelectRow(form, addParagraphElement, options, groupByColumn);
        }

        const addButton = document.createElement("input");
        addButton.type = "button";
        addButton.value = "+";
        addButton.addEventListener("click", () => addSelectRow(form, addParagraphElement, options, null));
        addParagraphElement.appendChild(addButton);

        addParagraphElement.appendChild(document.createTextNode(" "));

        const submitButton = document.createElement("input");
        submitButton.type = "submit";
        submitButton.value = "pivot";
        addParagraphElement.appendChild(submitButton);
    }

    export function setUp(options: string[]) {
        document.addEventListener("DOMContentLoaded", () => doSetUp(options));
    }
}
