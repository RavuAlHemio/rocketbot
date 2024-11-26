import { Charting } from "./bim/chart";
import { Drilldown } from "./bim/drilldown";
import { VehicleStatus } from "./bim/vehicle-status";

// "globals are evil"
declare global {
    interface Window { RocketBotWeb: any; }
}
window.RocketBotWeb = {
    Bim: {
        Charting: Charting,
        Drilldown: Drilldown,
        VehicleStatus: VehicleStatus,
    },
};
