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

    export function setUpByDayOfWeek() {
        document.addEventListener("DOMContentLoaded", doSetUpByDayOfWeek);
    }

    export function setUpByRideCountGroup() {
        document.addEventListener("DOMContentLoaded", doSetUpByRideCountGroup);
    }

    export function setUpLatestRiderCount() {
        document.addEventListener("DOMContentLoaded", doSetUpLatestRiderCount);
    }
}
