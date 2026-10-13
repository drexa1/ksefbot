export type KsefContractor = {
    source: "CEIDG" | "KRS" | "VAT-LB";
    name: string;
    nip: string;
    regon?: string;
    countryCode: string;
    addressLine: string;
    active: boolean;
};

export class ServiceUnavailableError extends Error {
    readonly status = 503;
    constructor(message: string, readonly details?: string) {
        super(message);
    }
}