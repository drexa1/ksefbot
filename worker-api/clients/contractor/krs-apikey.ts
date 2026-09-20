const KrsPositions = [193, 8, 327, 501, 112, 74, 409, 226, 16, 306];
const TimestampPositions = [492, 141, 364, 78, 259, 12, 430, 384, 97, 503, 67, 35, 471, 218];
const ChecksumPositions = [24, 46, 174, 345];
const ShiftMarkerPosition = 11;

export function encodeToken(krs: string, timestamp: string): string {
    if (krs.length > 10)
        throw new Error("KRS must be max 10 digits.");
    krs = krs.padStart(10, "0");
    const date = new Date(timestamp);
    if (isNaN(date.getTime()))
        throw new Error("Invalid timestamp format.");
    const formattedTimestamp = formatTimestamp(date);
    // Random 512-digit buffer
    const s = Array.from({ length: 512 }, () => Math.floor(10 * Math.random()).toString());
    // Last 4 digits are always zero
    for (let i = 508; i < 512; i++) {
        s[i] = "0";
    }
    // Insert KRS
    KrsPositions.forEach((position, index) => s[position] = krs[index]);
    // Insert timestamp
    TimestampPositions.forEach((position, index) => s[position] = formattedTimestamp[index]);
    // Random shift: 1..9
    const shift = randomInt(1, 10);
    s[ShiftMarkerPosition] = shift.toString();
    // Create space for checksum
    for (const position of ChecksumPositions) {
        shiftRight(s, position);
        s[position] = "0";
    }
    // Calculate checksum
    const checksum = s.reduce((sum, digit) => sum + Number(digit), 0).toString().padStart(4, "0");
    // Store checksum
    ChecksumPositions.forEach((position, index) => s[position] = checksum[index]);
    // Final circular rotation
    circularRight(s, shift);
    return s.join("");
}

function formatTimestamp(date: Date): string {
    const pad = (value: number): string => value.toString().padStart(2, "0");
    return (
        date.getFullYear().toString() +
        pad(date.getMonth() + 1) +
        pad(date.getDate()) +
        pad(date.getHours()) +
        pad(date.getMinutes()) +
        pad(date.getSeconds())
    );
}

function shiftRight(array: string[], position: number): void {
    for (let i = array.length - 1; i > position; i--) {
        array[i] = array[i - 1];
    }
    array[position] = "0";
}

function circularRight(array: string[], amount: number): void {
    const n = amount % array.length;
    if (n === 0) {
        return;
    }
    const copy = [...array];
    for (let i = 0; i < array.length; i++) {
        array[(i + n) % array.length] = copy[i];
    }
}

function randomInt(min: number, max: number): number {
    return Math.floor(Math.random() * (max - min)) + min;
}

// ---------------------------------------------------------
// Example
// ---------------------------------------------------------
// const timestamp = new Date().toISOString().slice(0, 19);
// const token = encodeToken("0000000000", timestamp);
// console.log(token);