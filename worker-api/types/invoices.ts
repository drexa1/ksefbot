export type AppInvoice = {
    id: string;
    // Parties
    type: "sales" | "purchase";
    customerId?: string;
    // Raw data
    rawXml: string;
    jsonData: string;
    notes?: string;
    // DBA
    createdAt?: string;
    updatedAt?: string;
};