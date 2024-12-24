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

interface ByTypeData {
    companyToVehicleTypeToRiderToCount: {
        [company: string]: {
            [vehicleType: string]: {
                [rider: string]: number;
            };
        };
    };
    companyToVehicleTypeToCount: {
        [company: string]: {
            [vehicleType: string]: number;
        };
    };
}

interface LatestRiderSankeyData {
    from: string;
    to: string;
    count: number;
}

type RiderToCount = {
    [rider: string]: number
};

type CompanyToRiderToCount = {
    [company: string]: {
        [rider: string]: number
    }
};

type CompanyToTypeToRiderToCount = {
    [company: string]: {
        [vehicleType: string]: {
            [rider: string]: number
        }
    }
};

interface FirstRiderPieData {
    companyToRiderToFirstRides: CompanyToRiderToCount;
    riderToTotalFirstRides: RiderToCount;
}

interface LastRiderPieData {
    companyToTypeToLastRiderToCount: CompanyToTypeToRiderToCount;
    companyToTypeToLastRiderToCountRidden: CompanyToTypeToRiderToCount;
}

type FixedCouplingData = {
    [frontVehicleType: string]: {
        [rider: string]: number[]
    }
};

interface GlobalStatsData {
    totalRides: number;
    companyToTotalRides: { [company: string]: number };
}

interface FixedMonopoliesOverTimeData {
    riderToTimestampToMonopolies: {
        [rider: string]: {
            [timestamp: string]: {
                count: number;
                points: number;
            }
        }
    }
}

interface LastRiderHistogramByFixedPosData {
    leadingTypeToRiderToCounts: {
        [leadingType: string]: {
            [rider: string]: number[];
        }
    }
}

interface DepotLastRiderPieData {
    companyToDepotToRiderToLastRides: {
        [company: string]: {
            [depot: string]: {
                [rider: string]: number;
            }
        }
    }
}

export namespace Charting {
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
        const totalWeekdayToCount: number[] = [];
        for (const rider of data.riders) {
            const weekdayToCount = data.riderToWeekdayToCount[rider];
            while (totalWeekdayToCount.length < weekdayToCount.length) {
                totalWeekdayToCount.push(0);
            }
            for (let i = 0; i < weekdayToCount.length; i++) {
                totalWeekdayToCount[i] += weekdayToCount[i];
            }
            datasets.push({
                label: rider,
                data: weekdayToCount,
                yAxisID: "yRegular",
            });
        }
        datasets.push({
            label: "(total)",
            data: totalWeekdayToCount,
            yAxisID: "yTotal",
        });

        new Chart(canvas, {
            type: "bar",
            data: {
                labels: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
                datasets: datasets,
            },
            options: {
                maintainAspectRatio: false,
                scales: {
                    "yRegular": {
                        display: true,
                        position: "left",
                        title: {
                            display: true,
                            text: "rides (single rider)",
                        },
                    },
                    "yTotal": {
                        display: true,
                        position: "right",
                        title: {
                            display: true,
                            text: "rides (total)",
                        },
                    },
                },
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
                maintainAspectRatio: false,
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

    function doSetUpByType() {
        // set up controls
        const controls = <HTMLParagraphElement|null>document.getElementById("histogram-controls");
        if (controls === null) {
            return;
        }

        const companyLabel = document.createElement("label");
        companyLabel.appendChild(document.createTextNode("Company: "));
        const companySelect = document.createElement("select");
        companyLabel.appendChild(companySelect);
        controls.appendChild(companyLabel);

        controls.appendChild(document.createTextNode(" \u00B7 "));

        const logPlotLabel = document.createElement("label");
        const logPlotCheckbox = document.createElement("input");
        logPlotCheckbox.type = "checkbox";
        logPlotLabel.appendChild(logPlotCheckbox);
        logPlotLabel.appendChild(document.createTextNode(" log plot"));
        controls.appendChild(logPlotLabel);

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: ByTypeData = JSON.parse(dataString);

        const allCompanies: string[] = [];
        for (const company of Object.keys(data.companyToVehicleTypeToRiderToCount)) {
            allCompanies.push(company);
        }
        allCompanies.sort();

        // pre-populate options
        for (let company of allCompanies) {
            const companyOption = document.createElement("option");
            companyOption.value = company;
            companyOption.textContent = company;
            companySelect.appendChild(companyOption);
        }
        companySelect.selectedIndex = 0;

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "bar",
            data: {
                datasets: [],
            },
            options: {
                maintainAspectRatio: false,
                scales: {
                    "yRegular": {
                        position: "left",
                        title: {
                            display: true,
                            text: "rides (single rider)",
                        },
                        ticks: {
                            format: {
                                minimumFractionDigits: 0,
                            },
                        },
                    },
                    "yAll": {
                        position: "right",
                        title: {
                            display: true,
                            text: "rides (all riders)",
                        },
                        ticks: {
                            format: {
                                minimumFractionDigits: 0,
                            },
                        },
                    },
                },
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"bar">, data: ByTypeData,
            companySelect: HTMLSelectElement,
        ) {
            const vehicleTypeToRiderToCount = data.companyToVehicleTypeToRiderToCount[companySelect.value];
            const vehicleTypeToCount = data.companyToVehicleTypeToCount[companySelect.value];

            const allVehicleTypes: string[] = [];
            const allRiders: string[] = [];
            for (const vehicleType of Object.keys(vehicleTypeToRiderToCount)) {
                allVehicleTypes.push(vehicleType);
                const riderToCount = vehicleTypeToRiderToCount[vehicleType];
                for (const rider of Object.keys(riderToCount)) {
                    if (allRiders.indexOf(rider) === -1) {
                        allRiders.push(rider);
                    }
                }
            }
            allVehicleTypes.sort();
            allRiders.sort();

            // collect the counts
            const datasets: object[] = [];
            for (const rider of allRiders) {
                const numbers: number[] = [];
                for (const vehicleType of allVehicleTypes) {
                    let number = vehicleTypeToRiderToCount[vehicleType][rider];
                    if (number === undefined) {
                        number = 0;
                    }
                    numbers.push(number);
                }

                datasets.push({
                    label: rider,
                    data: numbers,
                    yAxisID: "yRegular",
                });
            }

            const totalNumbers: number[] = [];
            for (const vehicleType of allVehicleTypes) {
                let number = vehicleTypeToCount[vehicleType];
                if (number === undefined) {
                    number = 0;
                }
                totalNumbers.push(number);
            }
            datasets.push({
                label: "(all)",
                data: totalNumbers,
                yAxisID: "yAll",
            });

            // give this data to the chart
            chart.data = {
                datasets: datasets,
                labels: allVehicleTypes,
            };
            chart.update();
        }

        // link up events
        companySelect.addEventListener("change", () => updateChart(
            chart, data, companySelect,
        ));
        logPlotCheckbox.addEventListener("change", () => {
            chart.options.scales!.yRegular!.type = logPlotCheckbox.checked ? "logarithmic" : "linear";
            chart.options.scales!.yAll!.type = logPlotCheckbox.checked ? "logarithmic" : "linear";
            chart.update();
        });

        // perform initial chart update
        updateChart(
            chart, data, companySelect,
        );
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

    function doSetUpFirstRiderPie() {
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

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: FirstRiderPieData = JSON.parse(dataString);

        const allCompanies: string[] = Object.keys(data.companyToRiderToFirstRides);
        allCompanies.sort();

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
            options: {
                maintainAspectRatio: false,
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"pie">, data: FirstRiderPieData,
            companySelect: HTMLSelectElement,
            allCompanies: string[],
        ) {
            const riderToCount = (companySelect.value === ALL_VALUE)
                ? data.riderToTotalFirstRides
                : data.companyToRiderToFirstRides[companySelect.value];

            // give this data to the chart
            const dataRiders: string[] = Object.keys(riderToCount);
            dataRiders.sort();
            const dataValues: number[] = dataRiders.map(r => riderToCount[r]);
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
        companySelect.addEventListener("change", () => updateChart(
            chart, data,
            companySelect,
            allCompanies,
        ));

        // perform initial chart update
        updateChart(
            chart, data,
            companySelect,
            allCompanies,
        );
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

        controls.appendChild(document.createTextNode(" \u00B7 "));

        const riddenOnlyLabel = document.createElement("label");
        const riddenOnlyCheckbox = document.createElement("input");
        riddenOnlyCheckbox.type = "checkbox";
        riddenOnlyCheckbox.checked = true;
        riddenOnlyLabel.appendChild(riddenOnlyCheckbox);
        riddenOnlyLabel.appendChild(document.createTextNode(" ridden only"));
        controls.appendChild(riddenOnlyLabel);

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
            options: {
                maintainAspectRatio: false,
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"pie">, data: LastRiderPieData,
            companySelect: HTMLSelectElement, typeSelect: HTMLSelectElement,
            riddenOnlyCheckbox: HTMLInputElement,
            allCompanies: string[], allTypes: string[],
        ) {
            const considerCompanies: string[] = (companySelect.value === ALL_VALUE)
                ? allCompanies
                : [companySelect.value];
            const considerTypes: string[] = (typeSelect.value === ALL_VALUE)
                ? allTypes
                : [typeSelect.value];
            const companyToTypeToLastRiderToCount = riddenOnlyCheckbox.checked
                ? data.companyToTypeToLastRiderToCountRidden
                : data.companyToTypeToLastRiderToCount;

            // does the selected company even have this type?
            if (considerCompanies.length === 1 && considerTypes.length === 1) {
                const companyTypes = Object.keys(companyToTypeToLastRiderToCount[considerCompanies[0]]);
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
                let companyTypes: string[] = Object.keys(companyToTypeToLastRiderToCount[companyName]);
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
                const typeToLastRiderToCount = companyToTypeToLastRiderToCount[companyName];
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
        companySelect.addEventListener("change", () => updateChart(
            chart, data,
            companySelect, typeSelect, riddenOnlyCheckbox,
            allCompanies, allTypes,
        ));
        typeSelect.addEventListener("change", () => updateChart(
            chart, data,
            companySelect, typeSelect, riddenOnlyCheckbox,
            allCompanies, allTypes,
        ));
        riddenOnlyCheckbox.addEventListener("change", () => updateChart(
            chart, data,
            companySelect, typeSelect, riddenOnlyCheckbox,
            allCompanies, allTypes,
        ));

        // perform initial chart update
        updateChart(
            chart, data,
            companySelect, typeSelect, riddenOnlyCheckbox,
            allCompanies, allTypes,
        );
    }

    function doSetUpFixedCouplingMemberUsage() {
        const ALL_VALUE: string = "\u0018";

        // set up controls
        const controls = <HTMLParagraphElement|null>document.getElementById("histogram-controls");
        if (controls === null) {
            return;
        }

        const typeLabel = document.createElement("label");
        typeLabel.appendChild(document.createTextNode("Front type: "));
        const typeSelect = document.createElement("select");
        typeLabel.appendChild(typeSelect);
        controls.appendChild(typeLabel);

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const frontVehicleTypeToRiderToCounts: FixedCouplingData = JSON.parse(dataString);

        const allFrontVehicleTypes: string[] = Object.keys(frontVehicleTypeToRiderToCounts);
        allFrontVehicleTypes.sort();

        // pre-populate options
        for (let frontVehicleType of allFrontVehicleTypes) {
            const typeOption = document.createElement("option");
            typeOption.value = frontVehicleType;
            typeOption.textContent = frontVehicleType;
            typeSelect.appendChild(typeOption);
        }
        typeSelect.selectedIndex = 0;

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "bar",
            data: {
                datasets: [],
            },
            options: {
                maintainAspectRatio: false,
                scales: {
                    "yRegular": {
                        position: "left",
                        title: {
                            display: true,
                            text: "rides (single rider)",
                        },
                        ticks: {
                            format: {
                                minimumFractionDigits: 0,
                            },
                        },
                    },
                    "yAll": {
                        position: "right",
                        title: {
                            display: true,
                            text: "rides (all riders)",
                        },
                        ticks: {
                            format: {
                                minimumFractionDigits: 0,
                            },
                        },
                    },
                },
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"bar">, frontVehicleTypeToRiderToCounts: FixedCouplingData,
            typeSelect: HTMLSelectElement,
        ) {
            const riderToNumbers = frontVehicleTypeToRiderToCounts[typeSelect.value];
            const typeRiders = Object.keys(riderToNumbers);
            typeRiders.sort();

            // collect the counts
            const datasets: object[] = [];
            const labels: string[] = [];
            for (let rider of typeRiders) {
                const riderName = (rider === ALL_VALUE) ? "(all)" : rider;
                const numbers = riderToNumbers[rider];

                while (labels.length < numbers.length) {
                    labels.push(`vehicle ${labels.length + 1}`);
                }

                datasets.push({
                    label: riderName,
                    data: numbers,
                    yAxisID: (rider === ALL_VALUE) ? "yAll" : "yRegular",
                });
            }

            // give this data to the chart
            chart.data = {
                datasets: datasets,
                labels: labels,
            };
            chart.update();
        }

        // link up events
        typeSelect.addEventListener("change", () => updateChart(
            chart, frontVehicleTypeToRiderToCounts, typeSelect,
        ));

        // perform initial chart update
        updateChart(
            chart, frontVehicleTypeToRiderToCounts, typeSelect,
        );
    }

    function doSetUpGlobalStats() {
        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: GlobalStatsData = JSON.parse(dataString);

        // filter out companies with less than a hundredth of the rides of the largest company
        const reducedCompanies: { [company: string]: number } = {};
        let otherRides: number = 0;
        const rideCounts = Object.keys(data.companyToTotalRides)
            .map(c => data.companyToTotalRides[c]);
        if (rideCounts.length === 0) {
            return;
        }
        rideCounts.sort((a, b) => a - b);
        const maxValue = rideCounts[rideCounts.length - 1];
        const minValue = maxValue / 500;
        for (let company of Object.keys(data.companyToTotalRides)) {
            const rides = data.companyToTotalRides[company];
            if (rides >= minValue) {
                reducedCompanies[company] = rides;
            } else {
                otherRides += rides;
            }
        }
        const reducedCompanyNames: string[] = Object.keys(reducedCompanies);
        reducedCompanyNames.sort((l, r) => reducedCompanies[r] - reducedCompanies[l]);
        const reducedCompanyValues: number[] = reducedCompanyNames.map(c => reducedCompanies[c]);
        reducedCompanyNames.push("(other)");
        reducedCompanyValues.push(otherRides);

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "pie",
            data: {
                datasets: [
                    {
                        data: reducedCompanyValues,
                    },
                ],
                labels: reducedCompanyNames,
            },
            options: {
                maintainAspectRatio: false,
            },
        });
    }

    function doSetUpFixedMonopoliesOverTime() {
        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: FixedMonopoliesOverTimeData = JSON.parse(dataString);

        const riders = Object.keys(data.riderToTimestampToMonopolies);
        const timestamps = Object.keys(data.riderToTimestampToMonopolies[riders[0]]);
        const labels: string[] = timestamps;
        const countDatasets: object[] = [];
        const pointDatasets: object[] = [];
        for (let rider of riders) {
            const timestampToMonopolies = data.riderToTimestampToMonopolies[rider];

            const countSeries: number[] = [];
            const pointSeries: number[] = [];
            for (let timestamp of Object.keys(timestampToMonopolies)) {
                countSeries.push(timestampToMonopolies[timestamp].count);
                pointSeries.push(timestampToMonopolies[timestamp].points);
            }

            countDatasets.push({
                label: rider,
                data: countSeries,
            });
            pointDatasets.push({
                label: rider,
                data: pointSeries,
            });
        }

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "line",
            data: {
                datasets: countDatasets,
                labels: labels,
            },
            options: {
                maintainAspectRatio: false,
            },
        });

        // add controls
        const controls = <HTMLParagraphElement|null>document.getElementById("chart-controls");
        if (controls === null) {
            return;
        }

        const pointsLabel = <HTMLLabelElement>document.createElement("label");
        controls.appendChild(pointsLabel);

        const pointsCheckbox = <HTMLInputElement>document.createElement("input");
        pointsCheckbox.type = "checkbox";
        pointsLabel.append(pointsCheckbox);
        pointsLabel.append(document.createTextNode(" points"));

        pointsCheckbox.addEventListener("change", () => {
            chart.data.datasets = (pointsCheckbox.checked)
                ? pointDatasets
                : countDatasets
            ;
            chart.update();
        });
    }

    function doSetUpLastRiderHistogramByFixedPos() {
        // set up controls
        const controls = <HTMLParagraphElement|null>document.getElementById("histogram-controls");
        if (controls === null) {
            return;
        }

        const typeLabel = document.createElement("label");
        typeLabel.appendChild(document.createTextNode("Front type: "));
        const typeSelect = document.createElement("select");
        typeLabel.appendChild(typeSelect);
        controls.appendChild(typeLabel);

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: LastRiderHistogramByFixedPosData = JSON.parse(dataString);

        const allFrontVehicleTypes: string[] = Object.keys(data.leadingTypeToRiderToCounts);
        allFrontVehicleTypes.sort();

        // pre-populate options
        for (let frontVehicleType of allFrontVehicleTypes) {
            const typeOption = document.createElement("option");
            typeOption.value = frontVehicleType;
            typeOption.textContent = frontVehicleType;
            typeSelect.appendChild(typeOption);
        }
        typeSelect.selectedIndex = 0;

        // set up empty chart
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }
        const chart = new Chart(canvas, {
            type: "bar",
            data: {
                datasets: [],
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"bar">, data: LastRiderHistogramByFixedPosData,
            typeSelect: HTMLSelectElement,
        ) {
            const riderToCounts = data.leadingTypeToRiderToCounts[typeSelect.value];
            const typeRiders = Object.keys(riderToCounts);
            typeRiders.sort();

            // collect the counts
            const datasets: object[] = [];
            const labels: string[] = [];
            for (let rider of typeRiders) {
                const numbers = riderToCounts[rider];

                while (labels.length < numbers.length) {
                    labels.push(`vehicle ${labels.length + 1}`);
                }

                datasets.push({
                    label: rider,
                    data: numbers,
                });
            }

            // give this data to the chart
            chart.data = {
                datasets: datasets,
                labels: labels,
            };
            chart.update();
        }

        // link up events
        typeSelect.addEventListener("change", () => updateChart(
            chart, data, typeSelect,
        ));

        // perform initial chart update
        updateChart(
            chart, data, typeSelect,
        );
    }

    function doSetUpDepotLastRiderPie() {
        const NO_DEPOT_VALUE: string = "\u0018";

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

        const depotLabel = document.createElement("label");
        depotLabel.appendChild(document.createTextNode("Depot: "));
        const depotSelect = document.createElement("select");
        depotLabel.appendChild(depotSelect);
        controls.appendChild(depotLabel);

        // load data
        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: DepotLastRiderPieData = JSON.parse(dataString);

        const allCompanies: string[] = Object.keys(data.companyToDepotToRiderToLastRides);
        allCompanies.sort();

        // pre-populate options
        for (let company of allCompanies) {
            const companyOption = document.createElement("option");
            companyOption.value = company;
            companyOption.textContent = company;
            companySelect.appendChild(companyOption);
        }
        companySelect.selectedIndex = 0;

        const firstCompanyDepots = Object.keys(data.companyToDepotToRiderToLastRides[allCompanies[0]]);
        for (let depot of firstCompanyDepots) {
            const depotOption = document.createElement("option");
            depotOption.value = depot;
            depotOption.textContent = (depot !== NO_DEPOT_VALUE) ? depot : "(none)";
            depotSelect.appendChild(depotOption);
        }
        depotSelect.selectedIndex = 0;

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
            options: {
                maintainAspectRatio: false,
            },
        });

        // define update function
        function updateChart(
            chart: Chart<"pie">, data: DepotLastRiderPieData,
            companySelect: HTMLSelectElement, depotSelect: HTMLSelectElement,
        ) {
            const selectedCompany = companySelect.value;
            const initialSelectedDepot = depotSelect.value;

            // reduce depots to those of the given company
            while (depotSelect.lastChild !== null) {
                depotSelect.removeChild(depotSelect.lastChild);
            }
            const companyDepots: string[] = Object.keys(data.companyToDepotToRiderToLastRides[selectedCompany]);
            for (let depot of companyDepots) {
                const depotOption = document.createElement("option");
                depotOption.value = depot;
                depotOption.textContent = (depot !== NO_DEPOT_VALUE) ? depot : "(none)";
                depotSelect.appendChild(depotOption);
            }

            const depotIndex = companyDepots.indexOf(initialSelectedDepot);
            if (depotIndex !== -1) {
                depotSelect.selectedIndex = depotIndex;
            } else {
                depotSelect.selectedIndex = 0;
            }
            const selectedDepot = depotSelect.value;

            // collect the counts
            const riderToLastVehicleCount = data.companyToDepotToRiderToLastRides[selectedCompany][selectedDepot];

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
        companySelect.addEventListener("change", () => updateChart(
            chart, data,
            companySelect, depotSelect,
        ));
        depotSelect.addEventListener("change", () => updateChart(
            chart, data,
            companySelect, depotSelect,
        ));

        // perform initial chart update
        updateChart(
            chart, data,
            companySelect, depotSelect,
        );
    }

    export function setUpByDayOfWeek() {
        document.addEventListener("DOMContentLoaded", doSetUpByDayOfWeek);
    }

    export function setUpByRideCountGroup() {
        document.addEventListener("DOMContentLoaded", doSetUpByRideCountGroup);
    }

    export function setUpByType() {
        document.addEventListener("DOMContentLoaded", doSetUpByType);
    }

    export function setUpLatestRiderCount() {
        document.addEventListener("DOMContentLoaded", doSetUpLatestRiderCount);
    }

    export function setUpFirstRiderPie() {
        document.addEventListener("DOMContentLoaded", doSetUpFirstRiderPie);
    }

    export function setUpLastRiderPie() {
        document.addEventListener("DOMContentLoaded", doSetUpLastRiderPie);
    }

    export function setUpFixedCouplingMemberUsage() {
        document.addEventListener("DOMContentLoaded", doSetUpFixedCouplingMemberUsage);
    }

    export function setUpGlobalStats() {
        document.addEventListener("DOMContentLoaded", doSetUpGlobalStats);
    }

    export function setUpFixedMonopoliesOverTime() {
        document.addEventListener("DOMContentLoaded", doSetUpFixedMonopoliesOverTime);
    }

    export function setUpLastRiderHistogramByFixedPos() {
        document.addEventListener("DOMContentLoaded", doSetUpLastRiderHistogramByFixedPos);
    }

    export function setUpDepotLastRiderPie() {
        document.addEventListener("DOMContentLoaded", doSetUpDepotLastRiderPie);
    }
}
