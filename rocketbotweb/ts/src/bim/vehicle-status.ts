export module VehicleStatus {
    let data: StatusData|null = null;

    interface StatusData {
        timestamp: string;
        vehicles: {
            [vehicleNumber: string]: VehicleEntry|undefined;
        };
    }

    interface VehicleEntry {
        state: "unridden"|"ridden-by-someone-else"|"ridden-by-you"|"ridden-by-you-recently";
        my_last_ride_opt: RiderAndTime|null;
        other_last_ride_opt: RiderAndTime|null;
        fixed_coupling: string[];
    }

    interface RiderAndTime {
        rider: string;
        time: string;
    }

    function createChildElement(parent: Element, tag: string): HTMLElement {
        const elem = document.createElement(tag);
        parent.appendChild(elem);
        return elem;
    }
    function createDivChild(parent: Element): HTMLDivElement {
        return <HTMLDivElement>createChildElement(parent, "div");
    }
    function createLabelChild(parent: Element): HTMLLabelElement {
        return <HTMLLabelElement>createChildElement(parent, "label");
    }
    function createInputChild(parent: Element): HTMLInputElement {
        return <HTMLInputElement>createChildElement(parent, "input");
    }
    function createSpanChild(parent: Element): HTMLSpanElement {
        return <HTMLSpanElement>createChildElement(parent, "span");
    }
    function createTextChild(parent: Element, content: string): Text {
        const text = document.createTextNode(content);
        parent.appendChild(text);
        return text;
    }

    function parseRustChronoUtcTimestamp(rustTimestamp: string): Date {
        const re = /^([0-9]+)-([0-9]+)-([0-9]+)T([0-9]+):([0-9]+):([0-9]+)(?:\.([0-9]*))?Z$/;
        const match = re.exec(rustTimestamp);
        if (match === null) {
            throw "failed to match";
        }

        let msString = match[7];
        while (msString.length < 3) {
            msString += "0";
        }
        msString = msString.substring(0, 3);

        return new Date(Date.UTC(
            +match[1],
            (+match[2]) - 1,
            +match[3],
            +match[4],
            +match[5],
            +match[6],
            +msString,
        ));
    }

    function leftZeroPad(numberString: string, toDigits: number): string {
        while (numberString.length < toDigits) {
            numberString = "0" + numberString;
        }
        return numberString;
    }

    function niceTimeFormat(timestamp: Date): string {
        const year = leftZeroPad("" + timestamp.getFullYear(), 4);
        const month = leftZeroPad("" + (timestamp.getMonth() + 1), 2);
        const day = leftZeroPad("" + timestamp.getDate(), 2);
        const hour = leftZeroPad("" + timestamp.getHours(), 2);
        const minute = leftZeroPad("" + timestamp.getMinutes(), 2);
        const second = leftZeroPad("" + timestamp.getSeconds(), 2);
        return `${year}-${month}-${day} ${hour}:${minute}:${second}`;
    }

    function appendRide(vehicleDiv: HTMLDivElement, rider: string, time: Date, my: boolean) {
        const rideDiv = createDivChild(vehicleDiv);
        rideDiv.classList.add("ride");
        rideDiv.classList.add(my ? "my" : "other");

        const timeSpan = createSpanChild(rideDiv);
        timeSpan.classList.add("timestamp");
        timeSpan.textContent = niceTimeFormat(time);

        const riderSpan = createSpanChild(rideDiv);
        riderSpan.classList.add("rider");
        riderSpan.textContent = rider;
    }

    function appendLookedUpVehicle(vehicleNumber: string, vehiclesDiv: HTMLDivElement, recurseToFixed: boolean) {
        if (data === null) {
            return;
        }

        const vehicle = data.vehicles[vehicleNumber];
        if (vehicle === undefined) {
            return;
        }

        const vehicleDiv = createDivChild(vehiclesDiv);
        vehicleDiv.classList.add("vehicle");
        vehicleDiv.classList.add(vehicle.state);

        const numberSpan = createSpanChild(vehicleDiv);
        numberSpan.classList.add("number");
        numberSpan.textContent = vehicleNumber;

        if (vehicle.my_last_ride_opt !== null) {
            const myTime = parseRustChronoUtcTimestamp(vehicle.my_last_ride_opt.time);
            if (vehicle.other_last_ride_opt !== null) {
                const otherTime = parseRustChronoUtcTimestamp(vehicle.other_last_ride_opt.time);
                if (myTime >= otherTime) {
                    appendRide(vehicleDiv, vehicle.my_last_ride_opt.rider, myTime, true);
                    appendRide(vehicleDiv, vehicle.other_last_ride_opt.rider, otherTime, false);
                } else {
                    appendRide(vehicleDiv, vehicle.other_last_ride_opt.rider, otherTime, false);
                    appendRide(vehicleDiv, vehicle.my_last_ride_opt.rider, myTime, true);
                }
            } else {
                appendRide(vehicleDiv, vehicle.my_last_ride_opt.rider, myTime, true);
            }
        } else if (vehicle.other_last_ride_opt !== null) {
            const otherTime = parseRustChronoUtcTimestamp(vehicle.other_last_ride_opt.time);
            appendRide(vehicleDiv, vehicle.other_last_ride_opt.rider, otherTime, false);
        }

        if (!recurseToFixed) {
            return;
        }

        if (vehicle.fixed_coupling.length > 0) {
            const coupledHeaderDiv = createDivChild(vehiclesDiv);
            coupledHeaderDiv.classList.add("coupled-header");
            coupledHeaderDiv.textContent = "coupled:";

            for (let i = 0; i < vehicle.fixed_coupling.length; i++) {
                const coupledVehicle = vehicle.fixed_coupling[i];
                appendLookedUpVehicle(coupledVehicle, vehiclesDiv, false);
            }
        }
    }

    function lookUpVehicle(vehicleNumberInput: HTMLInputElement, vehiclesDiv: HTMLDivElement) {
        while (vehiclesDiv.firstChild !== null) {
            vehiclesDiv.removeChild(vehiclesDiv.firstChild);
        }
        appendLookedUpVehicle(vehicleNumberInput.value, vehiclesDiv, true);
    }

    async function doSetUp() {
        const contentDiv = <HTMLDivElement|null>document.getElementById("content");
        if (contentDiv === null) {
            return;
        }
        contentDiv.textContent = "Loading data...";

        // obtain vehicle database
        let dataUrl = window.location.toString();
        dataUrl += (dataUrl.indexOf("?") === -1) ? "?" : "&";
        dataUrl += "action=data";
        const dataResponse = await fetch(dataUrl);
        const myData = await dataResponse.json();
        if (myData === null || myData === undefined) {
            contentDiv.textContent = ":-(";
            return;
        }
        data = myData;
        if (data === null) {
            throw "how did this happen?!";
        }

        while (contentDiv.lastChild !== null) {
            contentDiv.removeChild(contentDiv.lastChild);
        }

        const timestampText = niceTimeFormat(parseRustChronoUtcTimestamp(data.timestamp));
        const timestampBlock = createDivChild(contentDiv);
        timestampBlock.classList.add("timestamp");
        timestampBlock.textContent = `Data loaded: ${timestampText}`;

        const form = createDivChild(contentDiv);
        form.classList.add("form");

        const vehicleNumberLabel = createLabelChild(form);
        createTextChild(vehicleNumberLabel, "Vehicle number: ");
        const vehicleNumberInput = createInputChild(vehicleNumberLabel);

        const vehiclesDiv = createDivChild(contentDiv);
        vehiclesDiv.classList.add("vehicles");

        vehicleNumberInput.addEventListener("input", () => lookUpVehicle(vehicleNumberInput, vehiclesDiv));
        vehicleNumberInput.focus();
    }

    export function setUp() {
        document.addEventListener("DOMContentLoaded", doSetUp);
    }
}
