import {AppContractor} from "./contractors";

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

/// Model with details for invoice generation
export type InvoiceInput = {
    customerId: string;
    invoiceNumber?: string;
    hours?: number;
    customer?: AppContractor;
    customerEmail?: string;
    additionalEntity?: "none" | "jst" | "gv" | "other";
    issueDate?: string;
    issuePlace?: string;
    postingDate?: string;
    deliveryDate?: string;
    markingMpp?: boolean;
    markingMk?: boolean;
    markingFp?: boolean;
    markingTp?: boolean;
    priceType?: "net" | "gross";
    items?: {
        name: string;
        quantity: number;
        unit: string;
        unitPrice: number;
        vatRate: "23" | "8" | "5" | "0" | "ZW";
        gtu?: string;
        procedure?: string;
        additionalInformation?: string;
    }[];
    paymentType?: "1" | "2" | "3" | "4" | "5" | "6" | "7" | "*";
    bankAccount?: string;
    invoicePaid?: boolean;
    paymentDays?: number;
    paymentDeadline?: string;
    paymentTerm?: {
        periodLength: number;
        periodUnit: string;
        startingEvent: string;
    };
    paymentLink?: string;
    ksefPaymentId?: string;
    footers?: string[];
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

export type InvoicesBackfill = {
    ownerId: string;
    status: "queued" | "running" | "throttled" | "completed" | "failed";
    invoicesDownloaded: number;
    error?: string;
    // DBA
    createdAt: string;
    updatedAt?: string;
};
