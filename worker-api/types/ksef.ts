export interface KsefContextIdentifier {
    type: "Nip" | "InternalId" | "NipVatUe" | "PeppolId"
    value: string
}

export type KsefAuthenticationStatus = {
    status: {
        code: number
        description: string
        details?: string[]
    };
};

export type InvoiceEncryptionData = {
    cipherKey: Uint8Array
    cipherIv: Uint8Array
    encryptedSymmetricKey: string
    initializationVector: string
};

export type KsefInvoiceQueryResult = {
    hasMore: boolean
    isTruncated: boolean
    invoices: KsefInvoiceMetadata[]
};

export type KsefInvoiceMetadata = {
    ksefNumber: string
    invoiceNumber?: string
    sellerNip?: string
    issueDate?: string
    grossAmount?: number
};

export interface SubmissionStatus {
    ordinalNumber: number
    invoiceNumber: string
    ksefNumber: string
    referenceNumber: string
    invoiceHash: string
    acquisitionDate: string
    invoicingDate: string
    permanentStorageDate: string
    upoDownloadUrl: string
    upoDownloadUrlExpirationDate: string
    invoicingMode: string
}