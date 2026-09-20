export type AppTaxRecordDb = Omit<AppTaxRecord, "purchasesSummary"> & { purchasesSummary: string };

export type AppTaxRecord = {
    from: string;
    to: string;
    // Tax record
    brutIncome: number;
    vatPercentage: number;
    vatAmount: number;
    netBeforeObligations: number;
    // Obligations
    taxRate: number;
    incomeTax: number;
    healthInsuranceBase: number;
    healthInsuranceRate: number;
    healthContribution: number;
    // Purchases deductions
    purchasesSummary: PurchaseSummary[];
    // Total after obligations and purchases deductions
    totalCleanRevenue: number;
    notes: string;
    // DBA
    createdAt?: string;
    updatedAt?: string;
};

export type PurchaseSummary = {
    InvoiceNumber: string;
    TotalGrossAmount: number;
    TotalVatAmount: number;
}

export type TaxRecordObligations = {
    vatPercentage: number;
    vatAmount: number;
    netBeforeObligations: number;
    taxRate: number;
    incomeTax: number;
    healthInsuranceBase: number;
    healthInsuranceRate: number;
    healthContribution: number;
    purchasesDeductions: number;
    purchasesSummary: PurchaseSummary[];
}