export namespace Drilldown {
    function addRemoveButton(parent: HTMLElement, elementToRemove?: HTMLElement): HTMLInputElement {
        if (elementToRemove === undefined) {
            elementToRemove = parent;
        }

        const removeButton = document.createElement("input");
        removeButton.type = "button";
        removeButton.value = "\u2212";
        removeButton.addEventListener("click", () => elementToRemove.parentElement?.removeChild(elementToRemove));
        parent.appendChild(removeButton);
        return removeButton;
    }

    function addGroupRow(groupingDivElement: HTMLElement, addParagraphElement: HTMLElement, options: string[], selectedOption: string|null) {
        const paragraphElement = document.createElement("p");
        groupingDivElement.insertBefore(paragraphElement, addParagraphElement);

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

        addRemoveButton(paragraphElement);
    }

    function addFilterRow(filteringDivElement: HTMLElement, addParagraphElement: HTMLElement, options: string[], selectedPair: [string, string]|null) {
        const paragraphElement = document.createElement("p");
        paragraphElement.classList.add("filter-row");
        filteringDivElement.insertBefore(paragraphElement, addParagraphElement);

        const selectElement = document.createElement("select");
        selectElement.classList.add("filter-key");
        paragraphElement.appendChild(selectElement);

        paragraphElement.appendChild(document.createTextNode(" = "));

        const textElement = document.createElement("input");
        textElement.type = "text";
        textElement.classList.add("filter-value");
        paragraphElement.appendChild(textElement);

        for (const option of options) {
            const optionElement = document.createElement("option");
            optionElement.innerText = option;
            if (selectedPair !== null && option === selectedPair[0]) {
                optionElement.selected = true;
            }
            selectElement.appendChild(optionElement);
        }

        if (selectedPair !== null) {
            textElement.value = selectedPair[1];
        }

        addRemoveButton(paragraphElement);
    }

    function makeDataDiv(form: HTMLElement, className: string): HTMLDivElement {
        const divElement = document.createElement("div");
        divElement.classList.add(className);
        form.appendChild(divElement);
        return divElement;
    }

    function makeAddButton(dataDiv: HTMLElement, label: string): [HTMLParagraphElement, HTMLInputElement] {
        const addParagraph = document.createElement("p");
        dataDiv.appendChild(addParagraph);

        const addButton = document.createElement("input");
        addButton.type = "button";
        addButton.value = label;
        addParagraph.appendChild(addButton);

        return [addParagraph, addButton];
    }

    function modifyFormSubmission(form: HTMLFormElement, event: SubmitEvent) {
        // any filters?
        const filterRows = Array.from(form.querySelectorAll(".filter-row"));
        if (filterRows.length === 0) {
            // no; just do the default thing
            return;
        }

        // construct key-value pairs
        const filters: string[] = [];
        for (const filterRow of filterRows) {
            const keySelect = <HTMLSelectElement|null>filterRow.querySelector("select.filter-key");
            const valueInput = <HTMLInputElement|null>filterRow.querySelector("input.filter-value");

            if (keySelect === null || valueInput === null) {
                // ?!
                return;
            }

            filters.push(`${keySelect.value}=${valueInput.value}`);
        }

        // replace filter rows with inputs
        for (const filterRow of filterRows) {
            filterRow.parentElement?.removeChild(filterRow);
        }
        for (const filter of filters) {
            const filterElement = document.createElement("input");
            filterElement.type = "hidden";
            filterElement.name = "filter";
            filterElement.value = filter;
            form.appendChild(filterElement);
        }

        // the new fields are submitted, so no preventDefault
    }

    function doSetUp(options: string[]) {
        const drilldownControls = <HTMLDivElement|null>document.getElementById("bim-drilldown-controls");
        if (drilldownControls === null) {
            return;
        }

        const searchString = window.location.search;
        const groupByColumns: string[] = [];
        const filterColumns: [string, string][] = [];
        if (searchString.startsWith("?")) {
            const searchPairs = new URLSearchParams(searchString.substring(1));
            for (const [searchKey, searchValue] of searchPairs) {
                if (searchKey === "group") {
                    groupByColumns.push(searchValue);
                } else if (searchKey === "filter") {
                    const equalsIndex = searchValue.indexOf("=");
                    if (equalsIndex === -1) {
                        continue;
                    }
                    const filterKey = searchValue.substring(0, equalsIndex);
                    const filterValue = searchValue.substring(equalsIndex + 1);
                    filterColumns.push([filterKey, filterValue]);
                }
            }
        }

        const form = document.createElement("form");
        form.method = "get";
        drilldownControls.appendChild(form);

        const filteringDivElement = makeDataDiv(form, "filters");
        const groupingDivElement = makeDataDiv(form, "groups");

        const [addFilterParagraphElement, addFilterButton] = makeAddButton(filteringDivElement, "+filter");
        addFilterButton.addEventListener("click", () => addFilterRow(filteringDivElement, addFilterParagraphElement, options, null));

        const [addGroupParagraphElement, addGroupButton] = makeAddButton(groupingDivElement, "+group");
        addGroupButton.addEventListener("click", () => addGroupRow(groupingDivElement, addGroupParagraphElement, options, null));

        for (const [filterKey, filterValue] of filterColumns) {
            addFilterRow(filteringDivElement, addFilterParagraphElement, options, [filterKey, filterValue]);
        }
        for (const groupByColumn of groupByColumns) {
            addGroupRow(groupingDivElement, addGroupParagraphElement, options, groupByColumn);
        }

        const submitParagraphElement = document.createElement("p");
        form.appendChild(submitParagraphElement);

        const submitButton = document.createElement("input");
        submitButton.type = "submit";
        submitButton.value = "pivot";
        submitParagraphElement.appendChild(submitButton);

        form.addEventListener("submit", event => modifyFormSubmission(form, event));
    }

    export function setUp(options: string[]) {
        document.addEventListener("DOMContentLoaded", () => doSetUp(options));
    }
}
