export type KsefIdentifiable = {
    nip?: string;
    pesel?: string;
    regon?: string;
};

export type AppContractorUpdate = Partial<Omit<AppContractor, "id">>;

export type AppContractor = {
    id: string;
    // Customer data
    name: string;
    nip?: string;
    pesel?: string;
    regon?: string;
    internalIdentifier?: string;
    // Address
    countryCode: string;
    addressL1: string;
    addressL2?: string;
    // Customer metadata
    localGovernmentUnit?: number;
    vatGroup?: number;
    notes?: string;
    // DBA
    createdAt?: string;
    updatedAt?: string;
};