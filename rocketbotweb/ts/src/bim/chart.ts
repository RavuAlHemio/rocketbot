import { Chart, ChartData, ChartDataset, LineControllerChartOptions } from 'chart.js/auto';
import { SankeyController, Flow } from 'chartjs-chart-sankey';

Chart.register(SankeyController, Flow);

interface ByDayOfWeekData {
    riders: string[];
    riderToWeekdayToCount: { [rider: string]: number[] };
}

interface ByRideCountGroupData {
    riders: string[];
    rideCountGroupNames: string[];
    riderToGroupToCount: { [rider: string]: number[] };
}

interface LatestRiderSankeyData {
    from: string;
    to: string;
    count: number;
}

interface LastRiderPieData {
    companyToTypeToLastRiderToCount: {
        [company: string]: {
            [vehicleType: string]: {
                [lastRider: string]: number
            }
        }
    };
}

export module RocketBotWeb.Bim.Charting {
    function doSetUpByDayOfWeek() {
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }

        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: ByDayOfWeekData = JSON.parse(dataString);
        const datasets: ChartDataset[] = [];
        for (const rider of data.riders) {
            datasets.push({
                label: rider,
                data: data.riderToWeekdayToCount[rider],
            });
        }

        new Chart(canvas, {
            type: "bar",
            data: {
                labels: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
                datasets: datasets,
            },
        });
    }

    function doSetUpByRideCountGroup() {
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }

        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: ByRideCountGroupData = JSON.parse(dataString);
        const datasets: ChartDataset[] = [];
        for (const rider of data.riders) {
            datasets.push({
                label: rider,
                data: data.riderToGroupToCount[rider],
            });
        }

        const chart = new Chart(canvas, {
            type: "bar",
            data: {
                labels: data.rideCountGroupNames,
                datasets: datasets,
            },
            options: {
                scales: {
                    y: {
                        ticks: {
                            format: {
                                minimumFractionDigits: 0,
                            }
                        }
                    },
                },
            },
        });

        const logPlotCheckbox = <HTMLInputElement|null>document.getElementById("bim-charting-log-plot-checkbox");
        if (logPlotCheckbox !== null) {
            logPlotCheckbox.addEventListener("change", () => {
                chart.options.scales!.y!.type = logPlotCheckbox.checked ? "logarithmic" : "linear";
                chart.update();
            });
        }
    }

    function doSetUpLatestRiderCount() {
        const canvas = <HTMLCanvasElement|null>document.getElementById("sankey-canvas");
        if (canvas === null) {
            return;
        }

        const dataString = document.getElementById("sankey-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: LatestRiderSankeyData[] = JSON.parse(dataString);
        let labels = {};
        for (let datum of data) {
            // labels: strip leading Enter and Escape symbols from from and to values
            labels[datum.from] = datum.from.substring(1);
            labels[datum.to] = datum.to.substring(1);
        }

        const chart = new Chart(canvas, {
            type: "sankey",
            data: {
                datasets: [
                    {
                        data: data,
                        labels: labels,
                    },
                ],
            },
        });
    }

    function doSetUpLastRiderPie() {
        const ALL_VALUE: string = "\u0018";

        // set up controls
        const controls = <HTMLParagraphElement|null>document.getElementById("pie-controls");
        if (controls === null) {
            return;
        }

        const companyLabel = document.createElement("label");
        companyLabel.appendChild(document.createTextNode("Company: "));
        const companySelect = document.createElement("select");
        companyLabel.appendChild(companySelect);
        controls.appendChild(companyLabel);

        controls.appendChild(document.createTextNode(" \u00B7 "));

        const typeLabel = document.createElement("label");
        typeLabel.appendChild(document.createTextNode("Type: "));
        const typeSelect = document.createElement("select");
        typeLabel.appendChild(typeSelect);
        controls.appendChild(typeLabel);

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: LastRiderPieData = JSON.parse(dataString);

        const allCompanies: string[] = Object.keys(data.companyToTypeToLastRiderToCount);
        allCompanies.sort();

        const allTypes: string[] = [];
        for (let company of allCompanies) {
            for (let tp of Object.keys(data.companyToTypeToLastRiderToCount[company])) {
                if (allTypes.indexOf(tp) === -1) {
                    allTypes.push(tp);
                }
            }
        }
        allTypes.sort();

        // pre-populate options
        const allCompaniesOption = document.createElement("option");
        allCompaniesOption.value = ALL_VALUE;
        allCompaniesOption.textContent = "(all)";
        companySelect.appendChild(allCompaniesOption);
        for (let company of allCompanies) {
            const companyOption = document.createElement("option");
            companyOption.value = company;
            companyOption.textContent = company;
            companySelect.appendChild(companyOption);
        }
        companySelect.selectedIndex = 0;

        const allTypesOption = document.createElement("option");
        allTypesOption.value = ALL_VALUE;
        allTypesOption.textContent = "(all)";
        typeSelect.appendChild(allTypesOption);
        for (let tp of allTypes) {
            const typeOption = document.createElement("option");
            typeOption.value = tp;
            typeOption.textContent = tp;
            typeSelect.appendChild(typeOption);
        }
        typeSelect.selectedIndex = 0;

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "pie",
            data: {
                datasets: [],
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"pie">, data: LastRiderPieData,
            companySelect: HTMLSelectElement, typeSelect: HTMLSelectElement,
            allCompanies: string[], allTypes: string[],
        ) {
            const considerCompanies: string[] = (companySelect.value === ALL_VALUE)
                ? allCompanies
                : [companySelect.value];
            const considerTypes: string[] = (typeSelect.value === ALL_VALUE)
                ? allTypes
                : [typeSelect.value];

            // does the selected company even have this type?
            if (considerCompanies.length === 1 && considerTypes.length === 1) {
                const companyTypes = Object.keys(data.companyToTypeToLastRiderToCount[considerCompanies[0]]);
                if (companyTypes.indexOf(considerTypes[0]) === -1) {
                    // no; switch over to "all types"
                    considerTypes.length = 0;
                    considerTypes.push(...allTypes);
                }
            }

            // reduce types to those of the given company
            while (typeSelect.lastChild !== null) {
                typeSelect.removeChild(typeSelect.lastChild);
            }
            const allTypesOption = document.createElement("option");
            allTypesOption.value = ALL_VALUE;
            allTypesOption.textContent = "(all)";
            typeSelect.appendChild(allTypesOption);

            const consideredTypes: string[] = [];
            for (let companyName of considerCompanies) {
                let companyTypes: string[] = Object.keys(data.companyToTypeToLastRiderToCount[companyName]);
                for (let tp of companyTypes) {
                    if (consideredTypes.indexOf(tp) === -1) {
                        consideredTypes.push(tp);
                    }
                }
            }
            consideredTypes.sort();
            for (let tp of consideredTypes) {
                const typeOption = document.createElement("option");
                typeOption.value = tp;
                typeOption.textContent = tp;
                typeSelect.appendChild(typeOption);
            }

            if (considerTypes.length === 1) {
                typeSelect.value = considerTypes[0];
            } else {
                typeSelect.selectedIndex = 0;
            }

            // collect the counts
            const riderToLastVehicleCount: { [rider: string]: number } = {};
            for (let companyName of considerCompanies) {
                const typeToLastRiderToCount = data.companyToTypeToLastRiderToCount[companyName];
                for (let tp of considerTypes) {
                    const lastRiderToCount = typeToLastRiderToCount[tp];
                    if (lastRiderToCount === undefined) {
                        continue;
                    }

                    for (let rider of Object.keys(lastRiderToCount)) {
                        if (riderToLastVehicleCount[rider] === undefined) {
                            riderToLastVehicleCount[rider] = 0;
                        }
                        riderToLastVehicleCount[rider] += lastRiderToCount[rider];
                    }
                }
            }

            // give this data to the chart
            const dataRiders: string[] = Object.keys(riderToLastVehicleCount);
            dataRiders.sort();
            const dataValues: number[] = dataRiders.map(r => riderToLastVehicleCount[r]);
            chart.data = {
                datasets: [
                    {
                        data: dataValues,
                    },
                ],
                labels: dataRiders,
            };
            chart.update();
        }

        // link up events
        companySelect.addEventListener("change", () => updateChart(chart, data, companySelect, typeSelect, allCompanies, allTypes));
        typeSelect.addEventListener("change", () => updateChart(chart, data, companySelect, typeSelect, allCompanies, allTypes));

        // perform initial chart update
        updateChart(chart, data, companySelect, typeSelect, allCompanies, allTypes);
    }

    export function setUpByDayOfWeek() {
        document.addEventListener("DOMContentLoaded", doSetUpByDayOfWeek);
    }

    export function setUpByRideCountGroup() {
        document.addEventListener("DOMContentLoaded", doSetUpByRideCountGroup);
    }

    export function setUpLatestRiderCount() {
        document.addEventListener("DOMContentLoaded", doSetUpLatestRiderCount);
    }

    export function setUpLastRiderPie() {
        document.addEventListener("DOMContentLoaded", doSetUpLastRiderPie);
    }
}
