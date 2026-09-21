export type AppUserUpdate = Partial<Omit<AppUser, "id">>;

export type AppUser = {
    // Identification data
    id: string;
    language: "en" | "pl";
    phone: string;
    email: string;
    googleSubject: string;
    companyLogo?: Uint8Array;
    // Contractor data
    contractorId: string;
    // Application
    tier: number;
    apiKey?: string;
    // KSeF integration
    ksefApiToken?: string;
    // Invoicing defaults
    defaultItemName?: string;
    defaultHourlyRate?: number;
    settlementType: "monthly" | "quarterly";
    // Banking integration
    bankName?: string;
    bankAccountNumber?: string;
    bankApiToken?: string;
    // DBA
    createdAt?: string;
    updatedAt?: string;
};