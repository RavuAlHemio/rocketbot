import { RocketBotWeb } from "./bim/chart";

// "globals are evil"
declare global {
    interface Window { RocketBotWeb: any; }
}
window.RocketBotWeb = RocketBotWeb;
