import { Chart, ChartData, ChartDataset, LineControllerChartOptions } from 'chart.js/auto';

interface ByDayOfWeekData {
    riders: string[];
    riderToWeekdayToCount: { [rider: string]: number[] };
}

interface ByVehicleRideCountGroupData {
    riders: string[];
    rideCountGroupNames: string[];
    riderToGroupToCount: { [rider: string]: number[] };
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

    function doSetUpByVehicleRideCountGroup() {
        const canvas = <HTMLCanvasElement|null>document.getElementById("chart-canvas");
        if (canvas === null) {
            return;
        }

        const dataString = document.getElementById("chart-data")?.textContent;
        if (dataString === null || dataString === undefined) {
            return;
        }
        const data: ByVehicleRideCountGroupData = JSON.parse(dataString);
        const datasets: ChartDataset[] = [];
        for (const rider of data.riders) {
            datasets.push({
                label: rider,
                data: data.riderToGroupToCount[rider],
            });
        }

        new Chart(canvas, {
            type: "bar",
            data: {
                labels: data.rideCountGroupNames,
                datasets: datasets,
            },
        });
    }

    export function setUpByDayOfWeek() {
        document.addEventListener("DOMContentLoaded", doSetUpByDayOfWeek);
    }

    export function setUpByVehicleRideCountGroup() {
        document.addEventListener("DOMContentLoaded", doSetUpByVehicleRideCountGroup);
    }
}