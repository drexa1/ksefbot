export type KsefContractor = {
    source: "CEIDG" | "KRS" | "VAT-LB";
    name: string;
    nip: string;
    regon?: string;
    countryCode: string;
    addressLine: string;
    active: boolean;
};