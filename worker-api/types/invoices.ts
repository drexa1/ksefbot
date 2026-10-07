export type AppInvoice = {
    id: string;
    // Parties
    type: "sales" | "purchase";
    issueDate: string;
    customerId?: string;
    // Raw data
    rawXml: string;
    jsonData: string;
    notes?: string;
    // DBA
    createdAt?: string;
    updatedAt?: string;
};

export type InvoicesSyncPeriod = {
    ownerId: string;
    type: "sales" | "purchase";
    dateFrom: string;
    dateTo: string;
    // DBA
    createdAt: string;
    updatedAt?: string;
};

export type InvoicesBackfillStatus = {
    ownerId: string;
    status: "queued" | "running" | "completed" | "failed";
    invoicesDownloaded: number;
    error?: string;
    // DBA
    createdAt: string;
    updatedAt?: string;
};
